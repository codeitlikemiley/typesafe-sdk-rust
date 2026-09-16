//! Question and response types for the TypeSafe API.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::error::{
    api_error, ApiFailure, TypeSafeApiError, TypeSafeApiResponseValidationError,
    TypeSafeError,
};
use crate::constants::REQUEST_ID_HEADER;

pub type JsonValue = Value;
pub type JsonContent = Value;

/// Yes/no outcome descriptions.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NoulCriteria {
    #[serde(rename = "true", skip_serializing_if = "Option::is_none")]
    pub yes: Option<JsonContent>,
    #[serde(rename = "false", skip_serializing_if = "Option::is_none")]
    pub no: Option<JsonContent>,
}

/// A yes/no question.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Noul {
    #[serde(rename = "type", default = "noul_type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<JsonContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criteria: Option<NoulCriteria>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn noul_type() -> String {
    "noul".into()
}

impl Noul {
    pub fn new(instructions: Option<JsonContent>) -> Self {
        Self {
            type_: "noul".into(),
            instructions,
            criteria: None,
            extra: Map::new(),
        }
    }

    pub fn with_criteria(instructions: Option<JsonContent>, criteria: Option<NoulCriteria>) -> Self {
        Self {
            type_: "noul".into(),
            instructions,
            criteria,
            extra: Map::new(),
        }
    }
}

/// A choice question.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Choice {
    #[serde(rename = "type", default = "choice_type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<JsonContent>,
    pub criteria: HashMap<String, Option<JsonContent>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn choice_type() -> String {
    "choice".into()
}

impl Choice {
    pub fn new(
        instructions: Option<JsonContent>,
        criteria: HashMap<String, Option<JsonContent>>,
    ) -> Self {
        Self {
            type_: "choice".into(),
            instructions,
            criteria,
            extra: Map::new(),
        }
    }
}

/// A score question.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Score {
    #[serde(rename = "type", default = "score_type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<JsonContent>,
    pub criteria: Vec<JsonContent>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn score_type() -> String {
    "score".into()
}

impl Score {
    pub fn new(instructions: Option<JsonContent>, criteria: Vec<JsonContent>) -> Self {
        Self {
            type_: "score".into(),
            instructions,
            criteria,
            extra: Map::new(),
        }
    }
}

/// A question as a typed variant or raw JSON object.
#[derive(Clone, Debug)]
pub enum Question {
    Noul(Noul),
    Choice(Choice),
    Score(Score),
    Raw(Value),
}

impl Serialize for Question {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Question::Noul(q) => q.serialize(serializer),
            Question::Choice(q) => q.serialize(serializer),
            Question::Score(q) => q.serialize(serializer),
            Question::Raw(v) => v.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Question {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(deserializer)?;
        Ok(Question::Raw(v))
    }
}

pub type Questions = HashMap<String, Question>;

pub fn normalize_questions(questions: &Questions) -> Result<HashMap<String, Value>, TypeSafeError> {
    if questions.is_empty() {
        return Err(TypeSafeError::new("At least one question is required."));
    }
    let mut out = HashMap::new();
    for (name, question) in questions {
        let value = match question {
            Question::Score(s) => {
                validate_score_criteria(name, &s.criteria)?;
                serde_json::to_value(s).map_err(|e| TypeSafeError::new(e.to_string()))?
            }
            Question::Noul(n) => {
                serde_json::to_value(n).map_err(|e| TypeSafeError::new(e.to_string()))?
            }
            Question::Choice(c) => {
                serde_json::to_value(c).map_err(|e| TypeSafeError::new(e.to_string()))?
            }
            Question::Raw(v) => {
                validate_raw_question(name, v)?;
                v.clone()
            }
        };
        out.insert(name.clone(), value);
    }
    Ok(out)
}

fn validate_score_criteria(name: &str, criteria: &[JsonContent]) -> Result<(), TypeSafeError> {
    if criteria.is_empty() {
        return Err(TypeSafeError::new(format!(
            "Score question \"{}\" has no criteria; at least one score is required.",
            name
        )));
    }
    Ok(())
}

fn validate_raw_question(name: &str, v: &Value) -> Result<(), TypeSafeError> {
    let obj = v.as_object().ok_or_else(|| {
        TypeSafeError::new(format!(
            "Question \"{}\" must be a question object or a dictionary with a nonempty string \"type\".",
            name
        ))
    })?;
    let t = obj.get("type").and_then(|t| t.as_str()).filter(|s| !s.is_empty());
    if t.is_none() {
        return Err(TypeSafeError::new(format!(
            "Question \"{}\" must be a question object or a dictionary with a nonempty string \"type\".",
            name
        )));
    }
    let t = t.unwrap();
    if (t == "choice" || t == "score") && !obj.contains_key("criteria") {
        return Err(TypeSafeError::new(format!(
            "Question \"{}\" requires \"criteria\".",
            name
        )));
    }
    if t == "score" {
        let criteria = obj
            .get("criteria")
            .and_then(|c| c.as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[]);
        validate_score_criteria(name, criteria)?;
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NoulAnswer {
    pub noul: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceAnswer {
    pub choice: String,
    pub confidence: f64,
    pub probabilities: HashMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoreAnswer {
    pub score: f64,
    pub confidence: f64,
    pub legend: HashMap<i32, Value>,
    pub probabilities: HashMap<i32, f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Noul(NoulAnswer),
    Choice(ChoiceAnswer),
    Score(ScoreAnswer),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SystemOneResponse {
    pub model: String,
    pub usage: Usage,
    pub answers: HashMap<String, Answer>,
    pub request_id: Option<String>,
}

impl SystemOneResponse {
    pub fn nouls(&self) -> HashMap<String, NoulAnswer> {
        self.answers
            .iter()
            .filter_map(|(k, v)| match v {
                Answer::Noul(a) => Some((k.clone(), a.clone())),
                _ => None,
            })
            .collect()
    }

    pub fn choices(&self) -> HashMap<String, ChoiceAnswer> {
        self.answers
            .iter()
            .filter_map(|(k, v)| match v {
                Answer::Choice(a) => Some((k.clone(), a.clone())),
                _ => None,
            })
            .collect()
    }

    pub fn scores(&self) -> HashMap<String, ScoreAnswer> {
        self.answers
            .iter()
            .filter_map(|(k, v)| match v {
                Answer::Score(a) => Some((k.clone(), a.clone())),
                _ => None,
            })
            .collect()
    }

    pub fn from_http(
        status: u16,
        headers: &HashMap<String, String>,
        body: &[u8],
        endpoint: Option<String>,
    ) -> Result<Self, ApiFailure> {
        if !(200..300).contains(&status) {
            let parsed: Option<Value> = serde_json::from_slice(body).ok();
            let err = api_error(status, parsed, headers.clone(), endpoint);
            return Err(ApiFailure::Api(err));
        }
        decode_system_one(body, headers)
    }
}

fn decode_system_one(
    body: &[u8],
    headers: &HashMap<String, String>,
) -> Result<SystemOneResponse, ApiFailure> {
    let root: Value = serde_json::from_slice(body).map_err(|e| validation_err(headers, body, e.to_string()))?;
    let model = root
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or_else(|| validation_err(headers, body, "model".into()))?
        .to_string();
    let usage: Usage = root
        .get("usage")
        .map(|u| serde_json::from_value(u.clone()).unwrap_or_default())
        .unwrap_or_default();
    let answers_obj = root
        .get("answers")
        .and_then(|v| v.as_object())
        .ok_or_else(|| validation_err(headers, body, "answers".into()))?;

    let mut answers = HashMap::new();
    for (name, raw) in answers_obj {
        let tag = raw
            .get("type")
            .and_then(|t| t.as_str())
            .ok_or_else(|| validation_err(headers, body, format!("answers.{}.type", name)))?;
        match tag {
            "noul" => {
                let noul = raw
                    .get("noul")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| validation_err(headers, body, format!("answers.{}.noul", name)))?;
                answers.insert(name.clone(), Answer::Noul(NoulAnswer { noul }));
            }
            "choice" => {
                let choice = raw
                    .get("choice")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| validation_err(headers, body, format!("answers.{}.choice", name)))?
                    .to_string();
                let confidence = raw
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| validation_err(headers, body, format!("answers.{}.confidence", name)))?;
                let probabilities = parse_string_float_map(
                    raw.get("probabilities"),
                    &format!("answers.{}.probabilities", name),
                    headers,
                    body,
                )?;
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
                let score = raw
                    .get("score")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| validation_err(headers, body, format!("answers.{}.score", name)))?;
                let confidence = raw
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| validation_err(headers, body, format!("answers.{}.confidence", name)))?;
                let legend = parse_int_value_map(
                    raw.get("legend"),
                    &format!("answers.{}.legend", name),
                    headers,
                    body,
                )?;
                let probabilities = parse_int_float_map(
                    raw.get("probabilities"),
                    &format!("answers.{}.probabilities", name),
                    headers,
                    body,
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
            _ => {
                log::warn!("Ignoring answer {:?} with unrecognized type {:?}", name, tag);
            }
        }
    }

    let request_id = headers
        .get(REQUEST_ID_HEADER)
        .cloned()
        .or_else(|| {
            headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(REQUEST_ID_HEADER))
                .map(|(_, v)| v.clone())
        });

    Ok(SystemOneResponse {
        model,
        usage,
        answers,
        request_id,
    })
}

fn validation_err(
    headers: &HashMap<String, String>,
    body: &[u8],
    field_path: String,
) -> ApiFailure {
    let parsed: Option<Value> = serde_json::from_slice(body).ok();
    let err = TypeSafeApiError::new(
        200,
        parsed,
        headers.clone(),
        Some(format!("Invalid response data at {:?}.", field_path)),
        None,
    );
    ApiFailure::Validation(TypeSafeApiResponseValidationError(err))
}

fn parse_string_float_map(
    value: Option<&Value>,
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
) -> Result<HashMap<String, f64>, ApiFailure> {
    let obj = value
        .and_then(|v| v.as_object())
        .ok_or_else(|| validation_err(headers, body, path.to_string()))?;
    let mut out = HashMap::new();
    for (k, v) in obj {
        let f = v
            .as_f64()
            .ok_or_else(|| validation_err(headers, body, path.to_string()))?;
        out.insert(k.clone(), f);
    }
    Ok(out)
}

fn parse_int_float_map(
    value: Option<&Value>,
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
) -> Result<HashMap<i32, f64>, ApiFailure> {
    let obj = value
        .and_then(|v| v.as_object())
        .ok_or_else(|| validation_err(headers, body, path.to_string()))?;
    let mut out = HashMap::new();
    for (k, v) in obj {
        let key: i32 = k
            .parse()
            .map_err(|_| validation_err(headers, body, path.to_string()))?;
        let f = v
            .as_f64()
            .ok_or_else(|| validation_err(headers, body, path.to_string()))?;
        out.insert(key, f);
    }
    Ok(out)
}

fn parse_int_value_map(
    value: Option<&Value>,
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
) -> Result<HashMap<i32, Value>, ApiFailure> {
    let obj = value
        .and_then(|v| v.as_object())
        .ok_or_else(|| validation_err(headers, body, path.to_string()))?;
    let mut out = HashMap::new();
    for (k, v) in obj {
        let key: i32 = k
            .parse()
            .map_err(|_| validation_err(headers, body, path.to_string()))?;
        out.insert(key, v.clone());
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub name: String,
    pub description: String,
    pub release_date: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListModelsResponse {
    pub models: Vec<ModelMetadata>,
    pub request_id: Option<String>,
}

impl ListModelsResponse {
    pub fn from_http(
        status: u16,
        headers: &HashMap<String, String>,
        body: &[u8],
        endpoint: Option<String>,
    ) -> Result<Self, ApiFailure> {
        if !(200..300).contains(&status) {
            let parsed: Option<Value> = serde_json::from_slice(body).ok();
            let err = api_error(status, parsed, headers.clone(), endpoint);
            return Err(ApiFailure::Api(err));
        }
        let root: Value = serde_json::from_slice(body).map_err(|e| validation_err(headers, body, e.to_string()))?;
        let models: Vec<ModelMetadata> = root
            .get("models")
            .map(|m| serde_json::from_value(m.clone()).unwrap_or_default())
            .unwrap_or_default();
        let request_id = headers
            .get(REQUEST_ID_HEADER)
            .cloned()
            .or_else(|| {
                headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(REQUEST_ID_HEADER))
                    .map(|(_, v)| v.clone())
            });
        Ok(ListModelsResponse {
            models,
            request_id,
        })
    }
}
