use std::collections::BTreeMap;
use std::fmt;

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::error::{Error, deserialize_body, validation_error};
use crate::json::JsonContent;

#[derive(Clone, Debug, PartialEq, Serialize)]
/// Yes/no answer with a calibrated probability.
///
/// Deserializes with the client's decoder. An error names the field, such as
/// `Invalid answer data at 'noul'.`
pub struct NoulAnswer {
    /// Probability from 0.0 to 1.0 that the answer is yes.
    pub noul: f64,
}

impl NoulAnswer {
    /// Builds a noul answer from the yes probability.
    pub fn new(noul: f64) -> Self {
        Self { noul }
    }
}

impl<'de> Deserialize<'de> for NoulAnswer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        decode_with(deserializer, "answer", noul_fields)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// Selected label with per-label probabilities.
///
/// Deserializes with the client's decoder. An error names the field, such as
/// `Invalid answer data at 'probabilities.calm'.`
pub struct ChoiceAnswer {
    /// Selected criteria label.
    pub choice: String,
    /// Probability from 0.0 to 1.0 assigned to `choice`.
    pub confidence: f64,
    /// Map from each criteria label to its probability from 0.0 to 1.0.
    pub probabilities: IndexMap<String, f64>,
}

impl ChoiceAnswer {
    /// Builds a choice answer. `probabilities` keeps the given label order.
    pub fn new<C, I, K>(choice: C, confidence: f64, probabilities: I) -> Self
    where
        C: Into<String>,
        I: IntoIterator<Item = (K, f64)>,
        K: Into<String>,
    {
        Self {
            choice: choice.into(),
            confidence,
            probabilities: probabilities
                .into_iter()
                .map(|(label, probability)| (label.into(), probability))
                .collect(),
        }
    }
}

impl<'de> Deserialize<'de> for ChoiceAnswer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        decode_with(deserializer, "answer", choice_fields)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// Score answer with rubric legend and per-score probabilities.
///
/// On the wire, `legend` and `probabilities` are objects keyed by the score
/// index as a string, such as `{"0": "bad", "1": "good"}`. Deserializes with
/// the client's decoder. An error names the field, such as
/// `Invalid answer data at 'probabilities.1'.`
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

impl ScoreAnswer {
    /// Builds a score answer. `legend` and `probabilities` are keyed by score index.
    pub fn new<L, C, P>(score: f64, confidence: f64, legend: L, probabilities: P) -> Self
    where
        L: IntoIterator<Item = (u32, C)>,
        C: Into<JsonContent>,
        P: IntoIterator<Item = (u32, f64)>,
    {
        Self {
            score,
            confidence,
            legend: legend
                .into_iter()
                .map(|(index, text)| (index, text.into()))
                .collect(),
            probabilities: probabilities.into_iter().collect(),
        }
    }
}

impl<'de> Deserialize<'de> for ScoreAnswer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        decode_with(deserializer, "answer", score_fields)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
/// Typed answer for one named question.
///
/// Serializes with its `type` tag first, such as `{"type":"noul","noul":0.98}`.
/// Deserializing an answer of any other `type` is an error. Inside a
/// [`SystemOneResponse`], such answers are skipped instead, as the client does.
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

impl<'de> Deserialize<'de> for Answer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = Value::deserialize(deserializer)?;
        match decode_answer(&raw) {
            Ok(Some(answer)) => Ok(answer),
            Ok(None) => Err(serde::de::Error::custom(format!(
                "unrecognized answer type {:?}",
                answer_type(&raw)
            ))),
            Err(path) => Err(serde::de::Error::custom(format!(
                "Invalid answer data at '{path}'."
            ))),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
/// Token counts reported with a System One response.
///
/// A `None` count is left out when serialized. Deserializes with the client's
/// decoder: a missing or `null` count reads as `None`, other keys are ignored,
/// and an error names the count, such as `Invalid usage data at 'input_tokens'.`
pub struct Usage {
    /// Input tokens used. `None` when the server omits the count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i64>,
    /// Output tokens used. `None` when the server omits the count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i64>,
}

impl Usage {
    /// Builds token counts. Pass `None` for a count the server does not report.
    pub fn new(input_tokens: Option<i64>, output_tokens: Option<i64>) -> Self {
        Self {
            input_tokens,
            output_tokens,
        }
    }
}

impl<'de> Deserialize<'de> for Usage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        decode_with(deserializer, "usage", usage_fields)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// One model entry from the models endpoint.
///
/// Deserializes with the client's decoder: other keys are ignored, and an
/// error names the field, such as `Invalid model data at 'name'.`
pub struct ModelMetadata {
    /// Model name passed as `model` to System One.
    pub name: String,
    /// Human-readable model description.
    pub description: String,
    /// Model release date as reported by the server.
    pub release_date: String,
}

impl ModelMetadata {
    /// Builds one model entry.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        release_date: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            release_date: release_date.into(),
        }
    }
}

impl<'de> Deserialize<'de> for ModelMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        decode_with(deserializer, "model", model_fields)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// Models endpoint response: the wire object `{"models": [...]}`.
///
/// [`Client::models`](crate::Client::models) returns it with the request ID and
/// raw body. A server builds one with [`ListModelsResponse::new`] and serializes
/// it. Deserializing uses the client's decoder, so a malformed entry is an error
/// naming its path, such as `Invalid response data at 'models[0].name'.`
pub struct ListModelsResponse {
    /// Model entries, in the order served.
    pub models: Vec<ModelMetadata>,
    #[serde(skip)]
    request_id: Option<String>,
    #[serde(skip)]
    raw_body: bytes::Bytes,
}

impl<'de> Deserialize<'de> for ListModelsResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let models = decode_model_list(&value).map_err(invalid_response)?;
        Ok(Self::new(models))
    }
}

impl ListModelsResponse {
    /// Builds a models listing, for example in a server that serves the models
    /// endpoint.
    ///
    /// A listing built here or deserialized with serde has no request ID and an
    /// empty `raw_body`. Only a listing returned by `Client` carries them.
    pub fn new(models: impl IntoIterator<Item = ModelMetadata>) -> Self {
        Self {
            models: models.into_iter().collect(),
            request_id: None,
            raw_body: bytes::Bytes::new(),
        }
    }

    /// Returns the `x-typesafe-request-id` response header. Errors when the header is
    /// absent, and always on a listing from `new` or serde.
    pub fn request_id(&self) -> Result<&str, Error> {
        self.request_id
            .as_deref()
            .ok_or_else(|| Error::sdk("The response did not include a request ID."))
    }

    /// Returns the raw response body bytes exactly as received. Empty on a
    /// listing from `new` or serde.
    pub fn raw_body(&self) -> &[u8] {
        &self.raw_body
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
/// System One response with decoded answers keyed by question name.
///
/// Serializes to the wire object `{"model": ..., "usage": ..., "answers": ...}`.
/// Deserializing uses the client's decoder, so an answer of an unknown `type`
/// is skipped with a warning and a malformed field is an error naming its path.
pub struct SystemOneResponse {
    /// Name of the model that produced the answers.
    pub model: String,
    /// Token counts for the call.
    pub usage: Usage,
    /// Decoded answers keyed by question name.
    pub answers: IndexMap<String, Answer>,
    #[serde(skip)]
    request_id: Option<String>,
    #[serde(skip)]
    raw_body: bytes::Bytes,
}

impl<'de> Deserialize<'de> for SystemOneResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let (model, usage, answers) = decode_fields(&value).map_err(invalid_response)?;
        Ok(Self::new(model, usage, answers))
    }
}

impl SystemOneResponse {
    /// Builds a response, for example in a server that serves System One.
    ///
    /// A response built here or deserialized with serde has no request ID and
    /// an empty `raw_body`. Only a response returned by `Client` carries them.
    pub fn new<M, I, K>(model: M, usage: Usage, answers: I) -> Self
    where
        M: Into<String>,
        I: IntoIterator<Item = (K, Answer)>,
        K: Into<String>,
    {
        Self {
            model: model.into(),
            usage,
            answers: answers
                .into_iter()
                .map(|(name, answer)| (name.into(), answer))
                .collect(),
            request_id: None,
            raw_body: bytes::Bytes::new(),
        }
    }

    /// Returns the `x-typesafe-request-id` response header. Errors when the header is
    /// absent, and always on a response from `new` or serde.
    pub fn request_id(&self) -> Result<&str, Error> {
        self.request_id
            .as_deref()
            .ok_or_else(|| Error::sdk("The response did not include a request ID."))
    }

    /// Returns the raw response body bytes exactly as received. Empty on a
    /// response from `new` or serde.
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

/// The serde error for a response body the client would reject at `path`. The
/// same text as the client's `ApiError` message.
fn invalid_response<E: serde::de::Error>(path: String) -> E {
    E::custom(format!("Invalid response data at '{path}'."))
}

/// Deserializes a JSON object and decodes it with one of the client's field
/// decoders, so serde and `Client` read it the same way. An error reads
/// `Invalid {what} data at '{path}'.`
fn decode_with<'de, D, T>(
    deserializer: D,
    what: &str,
    decode: fn(&Map<String, Value>) -> Result<T, String>,
) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
{
    let object = Map::<String, Value>::deserialize(deserializer)?;
    decode(&object)
        .map_err(|path| serde::de::Error::custom(format!("Invalid {what} data at '{path}'.")))
}

pub(crate) fn decode_system_one(
    status: u16,
    headers: http::HeaderMap,
    endpoint: Option<String>,
    body: bytes::Bytes,
) -> Result<SystemOneResponse, Error> {
    let fields = serde_json::from_slice::<Value>(&body)
        .map_err(|_| "model".to_string())
        .and_then(|parsed| decode_fields(&parsed));
    match fields {
        Ok((model, usage, answers)) => Ok(SystemOneResponse {
            model,
            usage,
            answers,
            request_id: crate::error::header_str(&headers, crate::constants::REQUEST_ID_HEADER)
                .map(str::to_string),
            raw_body: body,
        }),
        Err(path) => Err(validation_error(
            status,
            deserialize_body(&body),
            headers,
            endpoint,
            path,
        )),
    }
}

/// Decodes a System One response object. Errors with the path of the first bad field.
fn decode_fields(parsed: &Value) -> Result<(String, Usage, IndexMap<String, Answer>), String> {
    let object = parsed.as_object().ok_or_else(|| "model".to_string())?;
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| "model".to_string())?
        .to_string();
    let usage = match object.get("usage") {
        None => Usage::default(),
        Some(Value::Object(usage)) => {
            usage_fields(usage).map_err(|path| format!("usage.{path}"))?
        }
        Some(_) => return Err("usage".to_string()),
    };
    let answers = match object.get("answers") {
        Some(Value::Object(map)) => decode_answers(map)?,
        None => IndexMap::new(),
        Some(_) => return Err("answers".to_string()),
    };
    Ok((model, usage, answers))
}

pub(crate) fn decode_models(
    status: u16,
    headers: http::HeaderMap,
    endpoint: Option<String>,
    body: bytes::Bytes,
) -> Result<ListModelsResponse, Error> {
    let models = serde_json::from_slice::<Value>(&body)
        .map_err(|_| "models".to_string())
        .and_then(|parsed| decode_model_list(&parsed));
    match models {
        Ok(models) => Ok(ListModelsResponse {
            models,
            request_id: crate::error::header_str(&headers, crate::constants::REQUEST_ID_HEADER)
                .map(str::to_string),
            raw_body: body,
        }),
        Err(path) => Err(validation_error(
            status,
            deserialize_body(&body),
            headers,
            endpoint,
            path,
        )),
    }
}

/// Decodes a models listing. Errors with the path of the first bad field, such
/// as `models[0].name`.
fn decode_model_list(parsed: &Value) -> Result<Vec<ModelMetadata>, String> {
    let Some(Value::Array(items)) = parsed.get("models") else {
        return Err("models".to_string());
    };
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let object = item.as_object().ok_or_else(|| format!("models[{index}]"))?;
            model_fields(object).map_err(|path| format!("models[{index}].{path}"))
        })
        .collect()
}

/// Reads the counts of a `usage` object. Errors with the key of a bad count.
fn usage_fields(object: &Map<String, Value>) -> Result<Usage, String> {
    Ok(Usage {
        input_tokens: optional_i64(object, "input_tokens")?,
        output_tokens: optional_i64(object, "output_tokens")?,
    })
}

fn optional_i64(object: &Map<String, Value>, key: &str) -> Result<Option<i64>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_u64().map(|value| value as i64))
            .ok_or_else(|| key.to_string())
            .map(Some),
        Some(_) => Err(key.to_string()),
    }
}

/// Reads one model entry. Errors with the key of a missing or non-string field.
fn model_fields(object: &Map<String, Value>) -> Result<ModelMetadata, String> {
    Ok(ModelMetadata {
        name: required_str(object, "name")?,
        description: required_str(object, "description")?,
        release_date: required_str(object, "release_date")?,
    })
}

fn required_str(object: &Map<String, Value>, key: &str) -> Result<String, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| key.to_string())
}

fn decode_answers(map: &Map<String, Value>) -> Result<IndexMap<String, Answer>, String> {
    let mut answers = IndexMap::new();
    for (name, raw) in map {
        match decode_answer(raw).map_err(|path| format!("answers.{name}.{path}"))? {
            Some(answer) => {
                answers.insert(name.clone(), answer);
            }
            None => {
                let other = answer_type(raw);
                log::warn!("Ignoring answer {name:?} with unrecognized type {other:?}");
            }
        }
    }
    Ok(answers)
}

/// Decodes one answer object. `Ok(None)` is an answer of a `type` this crate does
/// not know. Error paths are relative to the answer, such as `noul`.
fn decode_answer(raw: &Value) -> Result<Option<Answer>, String> {
    let object = raw.as_object().ok_or_else(|| "type".to_string())?;
    let tag = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| "type".to_string())?;
    let answer = match tag {
        "noul" => Answer::Noul(noul_fields(object)?),
        "choice" => Answer::Choice(choice_fields(object)?),
        "score" => Answer::Score(score_fields(object)?),
        _ => return Ok(None),
    };
    Ok(Some(answer))
}

fn noul_fields(object: &Map<String, Value>) -> Result<NoulAnswer, String> {
    let noul = object
        .get("noul")
        .and_then(Value::as_f64)
        .ok_or_else(|| "noul".to_string())?;
    Ok(NoulAnswer { noul })
}

fn choice_fields(object: &Map<String, Value>) -> Result<ChoiceAnswer, String> {
    let choice = object
        .get("choice")
        .and_then(Value::as_str)
        .ok_or_else(|| "choice".to_string())?
        .to_string();
    let confidence = object
        .get("confidence")
        .and_then(Value::as_f64)
        .ok_or_else(|| "confidence".to_string())?;
    let probabilities = object
        .get("probabilities")
        .and_then(Value::as_object)
        .ok_or_else(|| "probabilities".to_string())?;
    let probabilities = probabilities
        .iter()
        .map(|(key, value)| {
            value
                .as_f64()
                .map(|number| (key.clone(), number))
                .ok_or_else(|| format!("probabilities.{key}"))
        })
        .collect::<Result<IndexMap<_, _>, _>>()?;
    Ok(ChoiceAnswer {
        choice,
        confidence,
        probabilities,
    })
}

fn score_fields(object: &Map<String, Value>) -> Result<ScoreAnswer, String> {
    let score = object
        .get("score")
        .and_then(Value::as_f64)
        .ok_or_else(|| "score".to_string())?;
    let confidence = object
        .get("confidence")
        .and_then(Value::as_f64)
        .ok_or_else(|| "confidence".to_string())?;
    let legend = int_content_map(object.get("legend"), "legend")?;
    let probabilities = int_f64_map(object.get("probabilities"), "probabilities")?;
    Ok(ScoreAnswer {
        score,
        confidence,
        legend,
        probabilities,
    })
}

/// The `type` of an answer object `decode_answer` did not recognize.
fn answer_type(raw: &Value) -> &str {
    raw.get("type").and_then(Value::as_str).unwrap_or_default()
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
