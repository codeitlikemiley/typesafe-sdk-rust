use std::collections::BTreeMap;
use std::fmt;

use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::error::{Error, deserialize_body, validation_error};
use crate::json::JsonContent;

#[derive(Clone, Debug, PartialEq, Deserialize)]
/// Yes/no answer with a calibrated probability.
pub struct NoulAnswer {
    /// Probability from 0.0 to 1.0 that the answer is yes.
    pub noul: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
/// Selected label with per-label probabilities.
pub struct ChoiceAnswer {
    /// Selected criteria label.
    pub choice: String,
    /// Probability from 0.0 to 1.0 assigned to `choice`.
    pub confidence: f64,
    /// Map from each criteria label to its probability from 0.0 to 1.0.
    pub probabilities: IndexMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq)]
/// Score answer with rubric legend and per-score probabilities.
pub struct ScoreAnswer {
    /// Selected score. Follows the order of the question criteria.
    pub score: f64,
    /// Probability from 0.0 to 1.0 assigned to `score`.
    pub confidence: f64,
    /// Map from score index to its rubric text.
    pub legend: BTreeMap<u32, JsonContent>,
    /// Map from score index to its probability from 0.0 to 1.0.
    pub probabilities: BTreeMap<u32, f64>,
}

#[derive(Clone, Debug, PartialEq)]
/// Typed answer for one named question.
pub enum Answer {
    /// Yes/no answer. Holds a `NoulAnswer`.
    Noul(NoulAnswer),
    /// Single-label answer. Holds a `ChoiceAnswer`.
    Choice(ChoiceAnswer),
    /// Rubric score answer. Holds a `ScoreAnswer`.
    Score(ScoreAnswer),
}

impl Answer {
    /// Returns the inner `NoulAnswer` when the answer is `Answer::Noul`, else `None`.
    pub fn as_noul(&self) -> Option<&NoulAnswer> {
        match self {
            Self::Noul(answer) => Some(answer),
            _ => None,
        }
    }

    /// Returns the inner `ChoiceAnswer` when the answer is `Answer::Choice`, else `None`.
    pub fn as_choice(&self) -> Option<&ChoiceAnswer> {
        match self {
            Self::Choice(answer) => Some(answer),
            _ => None,
        }
    }

    /// Returns the inner `ScoreAnswer` when the answer is `Answer::Score`, else `None`.
    pub fn as_score(&self) -> Option<&ScoreAnswer> {
        match self {
            Self::Score(answer) => Some(answer),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
/// Token counts reported with a System One response.
pub struct Usage {
    /// Input tokens used. `None` when the server omits the count.
    pub input_tokens: Option<i64>,
    /// Output tokens used. `None` when the server omits the count.
    pub output_tokens: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
/// One model entry from the models endpoint.
pub struct ModelMetadata {
    /// Model name passed as `model` to System One.
    pub name: String,
    /// Human-readable model description.
    pub description: String,
    /// Model release date as reported by the server.
    pub release_date: String,
}

#[derive(Clone, Debug, PartialEq)]
/// Models endpoint response with decoded entries.
pub struct ListModelsResponse {
    /// Decoded model entries.
    pub models: Vec<ModelMetadata>,
    request_id: Option<String>,
    raw_body: bytes::Bytes,
}

impl ListModelsResponse {
    /// Returns the `x-typesafe-request-id` response header. Errors when the header is absent.
    pub fn request_id(&self) -> Result<&str, Error> {
        self.request_id
            .as_deref()
            .ok_or_else(|| Error::sdk("The response did not include a request ID."))
    }

    /// Returns the raw response body bytes exactly as received.
    pub fn raw_body(&self) -> &[u8] {
        &self.raw_body
    }
}

#[derive(Clone, Debug, PartialEq)]
/// System One response with decoded answers keyed by question name.
pub struct SystemOneResponse {
    /// Name of the model that produced the answers.
    pub model: String,
    /// Token counts for the call.
    pub usage: Usage,
    /// Decoded answers keyed by question name.
    pub answers: IndexMap<String, Answer>,
    request_id: Option<String>,
    raw_body: bytes::Bytes,
}

impl SystemOneResponse {
    /// Returns the `x-typesafe-request-id` response header. Errors when the header is absent.
    pub fn request_id(&self) -> Result<&str, Error> {
        self.request_id
            .as_deref()
            .ok_or_else(|| Error::sdk("The response did not include a request ID."))
    }

    /// Returns the raw response body bytes exactly as received.
    pub fn raw_body(&self) -> &[u8] {
        &self.raw_body
    }

    /// Returns the noul answer called `name`. Errors when the name or type does not match.
    pub fn noul(&self, name: &str) -> Result<&NoulAnswer, Error> {
        self.answers
            .get(name)
            .and_then(Answer::as_noul)
            .ok_or_else(|| Error::sdk(format!("No noul answer named \"{name}\".")))
    }

    /// Returns the choice answer called `name`. Errors when the name or type does not match.
    pub fn choice(&self, name: &str) -> Result<&ChoiceAnswer, Error> {
        self.answers
            .get(name)
            .and_then(Answer::as_choice)
            .ok_or_else(|| Error::sdk(format!("No choice answer named \"{name}\".")))
    }

    /// Returns the score answer called `name`. Errors when the name or type does not match.
    pub fn score(&self, name: &str) -> Result<&ScoreAnswer, Error> {
        self.answers
            .get(name)
            .and_then(Answer::as_score)
            .ok_or_else(|| Error::sdk(format!("No score answer named \"{name}\".")))
    }
}

pub(crate) fn decode_system_one(
    status: u16,
    headers: http::HeaderMap,
    endpoint: Option<String>,
    body: bytes::Bytes,
) -> Result<SystemOneResponse, Error> {
    let parsed: Value = serde_json::from_slice(&body).map_err(|_| {
        validation_error(
            status,
            deserialize_body(&body),
            headers.clone(),
            endpoint.clone(),
            "model".to_string(),
        )
    })?;
    let object = parsed.as_object().ok_or_else(|| {
        validation_error(
            status,
            deserialize_body(&body),
            headers.clone(),
            endpoint.clone(),
            "model".to_string(),
        )
    })?;
    let model = match object.get("model").and_then(Value::as_str) {
        Some(model) => model.to_string(),
        None => {
            return Err(validation_error(
                status,
                deserialize_body(&body),
                headers,
                endpoint,
                "model".to_string(),
            ));
        }
    };
    let usage = object
        .get("usage")
        .cloned()
        .map(decode_usage)
        .transpose()
        .map_err(|path| {
            validation_error(
                status,
                deserialize_body(&body),
                headers.clone(),
                endpoint.clone(),
                path,
            )
        })?
        .unwrap_or_default();
    let answers = match object.get("answers") {
        Some(Value::Object(map)) => decode_answers(map).map_err(|path| {
            validation_error(
                status,
                deserialize_body(&body),
                headers.clone(),
                endpoint.clone(),
                path,
            )
        })?,
        None => IndexMap::new(),
        Some(_) => {
            return Err(validation_error(
                status,
                deserialize_body(&body),
                headers,
                endpoint,
                "answers".to_string(),
            ));
        }
    };
    Ok(SystemOneResponse {
        model,
        usage,
        answers,
        request_id: crate::error::header_str(&headers, crate::constants::REQUEST_ID_HEADER)
            .map(str::to_string),
        raw_body: body,
    })
}

pub(crate) fn decode_models(
    status: u16,
    headers: http::HeaderMap,
    endpoint: Option<String>,
    body: bytes::Bytes,
) -> Result<ListModelsResponse, Error> {
    let parsed: Value = serde_json::from_slice(&body).map_err(|_| {
        validation_error(
            status,
            deserialize_body(&body),
            headers.clone(),
            endpoint.clone(),
            "models".to_string(),
        )
    })?;
    let models = match parsed.get("models") {
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(index, item)| decode_model(index, item))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|path| {
                validation_error(
                    status,
                    deserialize_body(&body),
                    headers.clone(),
                    endpoint.clone(),
                    path,
                )
            })?,
        _ => {
            return Err(validation_error(
                status,
                deserialize_body(&body),
                headers,
                endpoint,
                "models".to_string(),
            ));
        }
    };
    Ok(ListModelsResponse {
        models,
        request_id: crate::error::header_str(&headers, crate::constants::REQUEST_ID_HEADER)
            .map(str::to_string),
        raw_body: body,
    })
}

fn decode_usage(value: Value) -> Result<Usage, String> {
    let object = value.as_object().ok_or_else(|| "usage".to_string())?;
    Ok(Usage {
        input_tokens: optional_i64(object, "usage.input_tokens", "input_tokens")?,
        output_tokens: optional_i64(object, "usage.output_tokens", "output_tokens")?,
    })
}

fn optional_i64(object: &Map<String, Value>, path: &str, key: &str) -> Result<Option<i64>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_u64().map(|value| value as i64))
            .ok_or_else(|| path.to_string())
            .map(Some),
        Some(_) => Err(path.to_string()),
    }
}

fn decode_model(index: usize, value: &Value) -> Result<ModelMetadata, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("models[{index}]"))?;
    Ok(ModelMetadata {
        name: required_str(object, &format!("models[{index}].name"), "name")?,
        description: required_str(
            object,
            &format!("models[{index}].description"),
            "description",
        )?,
        release_date: required_str(
            object,
            &format!("models[{index}].release_date"),
            "release_date",
        )?,
    })
}

fn required_str(object: &Map<String, Value>, path: &str, key: &str) -> Result<String, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| path.to_string())
}

fn decode_answers(map: &Map<String, Value>) -> Result<IndexMap<String, Answer>, String> {
    let mut answers = IndexMap::new();
    for (name, raw) in map {
        let object = raw
            .as_object()
            .ok_or_else(|| format!("answers.{name}.type"))?;
        let tag = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("answers.{name}.type"))?;
        match tag {
            "noul" => {
                let noul = object
                    .get("noul")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("answers.{name}.noul"))?;
                answers.insert(name.clone(), Answer::Noul(NoulAnswer { noul }));
            }
            "choice" => {
                let choice = object
                    .get("choice")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("answers.{name}.choice"))?
                    .to_string();
                let confidence = object
                    .get("confidence")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("answers.{name}.confidence"))?;
                let probabilities = object
                    .get("probabilities")
                    .and_then(Value::as_object)
                    .ok_or_else(|| format!("answers.{name}.probabilities"))?;
                let probabilities = probabilities
                    .iter()
                    .map(|(key, value)| {
                        value
                            .as_f64()
                            .map(|number| (key.clone(), number))
                            .ok_or_else(|| format!("answers.{name}.probabilities.{key}"))
                    })
                    .collect::<Result<IndexMap<_, _>, _>>()?;
                answers.insert(
                    name.clone(),
                    Answer::Choice(ChoiceAnswer {
                        choice,
                        confidence,
                        probabilities,
                    }),
                );
            }
            "score" => {
                let score = object
                    .get("score")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("answers.{name}.score"))?;
                let confidence = object
                    .get("confidence")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("answers.{name}.confidence"))?;
                let legend =
                    int_content_map(object.get("legend"), &format!("answers.{name}.legend"))?;
                let probabilities = int_f64_map(
                    object.get("probabilities"),
                    &format!("answers.{name}.probabilities"),
                )?;
                answers.insert(
                    name.clone(),
                    Answer::Score(ScoreAnswer {
                        score,
                        confidence,
                        legend,
                        probabilities,
                    }),
                );
            }
            other => {
                log::warn!("Ignoring answer {name:?} with unrecognized type {other:?}");
            }
        }
    }
    Ok(answers)
}

fn int_content_map(
    value: Option<&Value>,
    path: &str,
) -> Result<BTreeMap<u32, JsonContent>, String> {
    let object = value
        .and_then(Value::as_object)
        .ok_or_else(|| path.to_string())?;
    let mut out = BTreeMap::new();
    for (key, item) in object {
        let index = key.parse::<u32>().map_err(|_| path.to_string())?;
        let content = JsonContent::from_value(item.clone()).map_err(|_| path.to_string())?;
        out.insert(index, content);
    }
    Ok(out)
}

fn int_f64_map(value: Option<&Value>, path: &str) -> Result<BTreeMap<u32, f64>, String> {
    let object = value
        .and_then(Value::as_object)
        .ok_or_else(|| path.to_string())?;
    let mut out = BTreeMap::new();
    for (key, item) in object {
        let index = key.parse::<u32>().map_err(|_| path.to_string())?;
        let number = item.as_f64().ok_or_else(|| format!("{path}.{key}"))?;
        out.insert(index, number);
    }
    Ok(out)
}

impl fmt::Display for SystemOneResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SystemOneResponse {{ model: {}, answers: {} }}",
            self.model,
            self.answers.len()
        )
    }
}
