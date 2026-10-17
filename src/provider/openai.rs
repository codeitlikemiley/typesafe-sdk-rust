//! OpenAI Decisions, `POST {base}/v1/decisions`. Experimental: the shapes here
//! come from the public guide, which has no API reference.
//!
//! The request is `{"model", "input", "questions": [...]}` with each question
//! carrying its `name`. The response is `{"answers": [...]}` with `predicate`,
//! `choice`, `score`, and `refusal` answers, each with its `name`.

use indexmap::IndexMap;
use serde_json::{Map, Value, json};

use crate::answer::{
    Answer, ChoiceAnswer, NoulAnswer, RefusalAnswer, ScoreAnswer, SystemOneResponse, Usage,
    answer_type, finish_response, usage_fields,
};
use crate::error::Error;
use crate::json::JsonContent;
use crate::question::{Question, typed};
use crate::wire::SystemOneRequest;

const PROVIDER: &str = "OpenAI Decisions";

/// What decoding needs from the request: each question's name, in order, with
/// the criteria of a score question for its legend.
pub(crate) struct Plan {
    model: String,
    questions: Vec<(String, Option<Vec<JsonContent>>)>,
}

/// The Decisions body for `request`, or `Error::Unsupported` for the first
/// part it cannot express.
pub(crate) fn encode(request: &SystemOneRequest) -> Result<(Vec<u8>, Plan), Error> {
    let input = match &request.state {
        JsonContent::String(text) => text.clone(),
        _ => {
            return Err(Error::unsupported(
                PROVIDER,
                "structured state: state must be a string",
            ));
        }
    };
    let mut questions = Vec::with_capacity(request.questions.len());
    let mut plan = Vec::with_capacity(request.questions.len());
    for (name, question) in &request.questions {
        let (wire, levels) = encode_question(name, question)?;
        questions.push(wire);
        plan.push((name.clone(), levels));
    }
    let mut body = Map::new();
    body.insert(
        "model".to_string(),
        json!(request.model.clone().unwrap_or_default()),
    );
    body.insert("input".to_string(), Value::String(input));
    body.insert("questions".to_string(), Value::Array(questions));
    for (key, value) in &request.extra {
        body.insert(key.clone(), value.clone());
    }
    let bytes = serde_json::to_vec(&body)
        .map_err(|_| Error::sdk("The request body could not be encoded as JSON"))?;
    Ok((
        bytes,
        Plan {
            model: super::effective_model(request),
            questions: plan,
        },
    ))
}

fn encode_question(
    name: &str,
    question: &Question,
) -> Result<(Value, Option<Vec<JsonContent>>), Error> {
    let question = match question {
        Question::Raw(map) => typed(map).ok_or_else(|| {
            Error::unsupported(
                PROVIDER,
                format!(
                    "raw questions that are not a plain noul, choice, or score (question \"{name}\")"
                ),
            )
        })?,
        other => other.clone(),
    };
    match question {
        Question::Noul {
            instructions,
            criteria,
        } => {
            if criteria.is_some() {
                return Err(Error::unsupported(
                    PROVIDER,
                    format!("noul criteria (question \"{name}\")"),
                ));
            }
            let instructions = text(name, instructions.as_ref())?;
            Ok((
                json!({"type": "predicate", "name": name, "instructions": instructions}),
                None,
            ))
        }
        Question::Choice {
            instructions,
            criteria,
        } => {
            let instructions = text(name, instructions.as_ref())?;
            let choices = criteria
                .iter()
                .map(|(label, description)| {
                    let mut choice = Map::new();
                    choice.insert("value".to_string(), Value::String(label.clone()));
                    if let Some(description) = description {
                        choice.insert(
                            "description".to_string(),
                            Value::String(description_text(name, description)?),
                        );
                    }
                    Ok(Value::Object(choice))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            Ok((
                json!({"type": "choice", "name": name, "instructions": instructions, "choices": choices}),
                None,
            ))
        }
        Question::Score {
            instructions,
            criteria,
        } => {
            let instructions = text(name, instructions.as_ref())?;
            let levels = criteria
                .iter()
                .enumerate()
                .map(|(index, description)| {
                    Ok(json!({
                        "label": index.to_string(),
                        "description": description_text(name, description)?,
                    }))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            Ok((
                json!({"type": "score", "name": name, "instructions": instructions, "levels": levels}),
                Some(criteria),
            ))
        }
        Question::Box { .. } => Err(Error::unsupported(
            PROVIDER,
            format!("box questions (question \"{name}\")"),
        )),
        Question::Raw(_) => unreachable!("typed() never returns Question::Raw"),
    }
}

fn text(name: &str, instructions: Option<&JsonContent>) -> Result<String, Error> {
    match instructions {
        Some(JsonContent::String(text)) => Ok(text.clone()),
        Some(_) => Err(Error::unsupported(
            PROVIDER,
            format!("structured instructions (question \"{name}\"): instructions must be a string"),
        )),
        None => Err(Error::unsupported(
            PROVIDER,
            format!("questions without instructions (question \"{name}\")"),
        )),
    }
}

fn description_text(name: &str, description: &JsonContent) -> Result<String, Error> {
    match description {
        JsonContent::String(text) => Ok(text.clone()),
        _ => Err(Error::unsupported(
            PROVIDER,
            format!(
                "structured criteria descriptions (question \"{name}\"): descriptions must be strings"
            ),
        )),
    }
}

pub(crate) fn decode(
    status: u16,
    headers: http::HeaderMap,
    endpoint: Option<String>,
    body: bytes::Bytes,
    plan: &Plan,
) -> Result<SystemOneResponse, Error> {
    let fields = serde_json::from_slice::<Value>(&body)
        .map_err(|_| "answers".to_string())
        .and_then(|parsed| decode_fields(&parsed, plan));
    finish_response(status, headers, endpoint, body, fields)
}

type Fields = (String, Usage, IndexMap<String, Answer>);

fn decode_fields(parsed: &Value, plan: &Plan) -> Result<Fields, String> {
    let object = parsed.as_object().ok_or_else(|| "answers".to_string())?;
    let model = match object.get("model") {
        Some(Value::String(model)) => model.clone(),
        _ => plan.model.clone(),
    };
    let usage = match object.get("usage") {
        None | Some(Value::Null) => Usage::default(),
        Some(Value::Object(usage)) => {
            usage_fields(usage).map_err(|path| format!("usage.{path}"))?
        }
        Some(_) => return Err("usage".to_string()),
    };
    let items = match object.get("answers") {
        Some(Value::Array(items)) => items,
        _ => return Err("answers".to_string()),
    };
    let mut received = IndexMap::new();
    for (index, item) in items.iter().enumerate() {
        let at = |field: &str| format!("answers[{index}].{field}");
        let answer = item
            .as_object()
            .ok_or_else(|| format!("answers[{index}]"))?;
        let kind = answer
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| at("type"))?;
        // A refusal's `name` is optional in the guide's Java sample. Without
        // one, it belongs to the question at the same position.
        let name = match answer.get("name").and_then(Value::as_str) {
            Some(name) => name.to_string(),
            None if kind == "refusal" => plan
                .questions
                .get(index)
                .map(|(name, _)| name.clone())
                .ok_or_else(|| at("name"))?,
            None => return Err(at("name")),
        };
        let levels = plan
            .questions
            .iter()
            .find(|(question, _)| *question == name)
            .and_then(|(_, levels)| levels.as_ref());
        let decoded = match kind {
            "predicate" => Answer::Noul(NoulAnswer::new(number(answer, "probability", &at)?)),
            "choice" => Answer::Choice(choice(answer, &at)?),
            "score" => Answer::Score(score(answer, levels, &at)?),
            "refusal" => Answer::Refusal(RefusalAnswer::new(answer.clone())),
            _ => {
                let other = answer_type(item);
                log::warn!("Ignoring answer {name:?} with unrecognized type {other:?}");
                continue;
            }
        };
        received.insert(name, decoded);
    }
    // The request's question order, then any answer for a name not asked.
    let mut answers = IndexMap::with_capacity(received.len());
    for (name, _) in &plan.questions {
        if let Some(answer) = received.shift_remove(name) {
            answers.insert(name.clone(), answer);
        }
    }
    answers.extend(received);
    Ok((model, usage, answers))
}

fn number(
    object: &Map<String, Value>,
    key: &str,
    at: &dyn Fn(&str) -> String,
) -> Result<f64, String> {
    object
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| at(key))
}

fn probabilities<'a>(
    object: &'a Map<String, Value>,
    at: &dyn Fn(&str) -> String,
) -> Result<impl Iterator<Item = (usize, &'a Map<String, Value>)>, String> {
    let items = object
        .get("probabilities")
        .and_then(Value::as_array)
        .ok_or_else(|| at("probabilities"))?;
    if let Some(bad) = items.iter().position(|item| !item.is_object()) {
        return Err(at(&format!("probabilities[{bad}]")));
    }
    Ok(items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| item.as_object().map(|item| (index, item))))
}

fn choice(
    object: &Map<String, Value>,
    at: &dyn Fn(&str) -> String,
) -> Result<ChoiceAnswer, String> {
    let choice = object
        .get("choice")
        .and_then(Value::as_str)
        .ok_or_else(|| at("choice"))?;
    let confidence = number(object, "confidence", at)?;
    let probabilities = probabilities(object, at)?
        .map(|(index, item)| {
            let value = item
                .get("value")
                .and_then(Value::as_str)
                .ok_or_else(|| at(&format!("probabilities[{index}].value")))?;
            let probability = item
                .get("probability")
                .and_then(Value::as_f64)
                .ok_or_else(|| at(&format!("probabilities[{index}].probability")))?;
            Ok((value.to_string(), probability))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ChoiceAnswer::new(choice, confidence, probabilities))
}

fn score(
    object: &Map<String, Value>,
    levels: Option<&Vec<JsonContent>>,
    at: &dyn Fn(&str) -> String,
) -> Result<ScoreAnswer, String> {
    let score = number(object, "score", at)?;
    let confidence = number(object, "confidence", at)?;
    let probabilities = probabilities(object, at)?
        .map(|(index, item)| {
            let value = item
                .get("value")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| at(&format!("probabilities[{index}].value")))?;
            let probability = item
                .get("probability")
                .and_then(Value::as_f64)
                .ok_or_else(|| at(&format!("probabilities[{index}].probability")))?;
            Ok((value, probability))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let legend = levels
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, level)| (index as u32, level.clone()));
    Ok(ScoreAnswer::new(score, confidence, legend, probabilities))
}
