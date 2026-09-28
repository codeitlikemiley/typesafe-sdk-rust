//! System One wire types, for callers and for servers.
//!
//! [`Client`](crate::Client) serializes a [`SystemOneRequest`] and decodes a
//! [`SystemOneResponse`] or a models list. A server that serves the same contract
//! uses the same types in the other direction: it deserializes the request and
//! serializes the response it builds with the constructors. Mount the handlers at
//! [`SYSTEM_ONE_PATH`] and [`MODELS_PATH`] under your base URL.
//!
//! Serialization matches the JSON the client sends and expects. Deserialization
//! is as lenient as the client. Unknown request keys stay in
//! [`SystemOneRequest::extra`], a question of unknown `type` stays
//! [`Question::Raw`], and an answer of unknown `type` inside a response is skipped
//! with a warning.
//!
//! ```
//! use typesafe_sdk::wire::{
//!     Answer, ModelMetadata, ModelsResponse, NoulAnswer, Question, SystemOneRequest,
//!     SystemOneResponse, Usage,
//! };
//!
//! let body = br#"{"state":"I was charged twice.","model":"jev-latest","questions":{"billing":{"type":"noul","instructions":"Is this about billing?"}}}"#;
//! let request: SystemOneRequest = serde_json::from_slice(body)?;
//! assert!(matches!(request.questions["billing"], Question::Noul { .. }));
//!
//! let answers = request
//!     .questions
//!     .keys()
//!     .map(|name| (name.clone(), Answer::Noul(NoulAnswer::new(0.97))));
//! let model = request.model.as_deref().unwrap_or("jev-latest");
//! let response = SystemOneResponse::new(model, Usage::new(Some(12), Some(1)), answers);
//! assert_eq!(
//!     serde_json::to_string(&response)?,
//!     r#"{"model":"jev-latest","usage":{"input_tokens":12,"output_tokens":1},"answers":{"billing":{"type":"noul","noul":0.97}}}"#
//! );
//!
//! let models = ModelsResponse::new([ModelMetadata::new("jev-latest", "Fast model", "2026-08-01")]);
//! assert_eq!(
//!     serde_json::to_string(&models)?,
//!     r#"{"models":[{"name":"jev-latest","description":"Fast model","release_date":"2026-08-01"}]}"#
//! );
//! # Ok::<(), serde_json::Error>(())
//! ```

use indexmap::IndexMap;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

pub use crate::answer::{
    Answer, ChoiceAnswer, ModelMetadata, NoulAnswer, ScoreAnswer, SystemOneResponse, Usage,
};
pub use crate::constants::{MODELS_PATH, SYSTEM_ONE_PATH};
pub use crate::json::JsonContent;
pub use crate::question::{NoulCriteria, Question};

/// Body of `POST /v1/systemone`.
///
/// Serializes as `state`, then `model` (left out when `None`), then `questions`,
/// then the `extra` keys in order. An `extra` key named `state`, `model`, or
/// `questions` replaces that field in place, the same last-write-wins rule as
/// [`SystemOneOpts::extra_body`](crate::SystemOneOpts::extra_body), so the output
/// never repeats a key. Deserializing never puts those three names in `extra`.
///
/// Deserializing requires `state` and at least one question, and each question
/// must be one the client would send (see [`Question`]). A `null` `model` reads
/// as `None`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SystemOneRequest {
    /// Text, JSON object, or JSON array the questions are about.
    pub state: JsonContent,
    /// Model to answer with. `None` leaves `model` out, so the server picks.
    #[serde(default)]
    pub model: Option<String>,
    /// Named questions, in the order they were given.
    #[serde(deserialize_with = "nonempty_questions")]
    pub questions: IndexMap<String, Question>,
    /// Other top-level keys, such as provider options, kept in order.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Serialize for SystemOneRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        match self.extra.get("state") {
            Some(state) => map.serialize_entry("state", state)?,
            None => map.serialize_entry("state", &self.state)?,
        }
        match (self.extra.get("model"), &self.model) {
            (Some(model), _) => map.serialize_entry("model", model)?,
            (None, Some(model)) => map.serialize_entry("model", model)?,
            (None, None) => {}
        }
        match self.extra.get("questions") {
            Some(questions) => map.serialize_entry("questions", questions)?,
            None => map.serialize_entry("questions", &self.questions)?,
        }
        for (key, value) in &self.extra {
            if !matches!(key.as_str(), "state" | "model" | "questions") {
                map.serialize_entry(key, value)?;
            }
        }
        map.end()
    }
}

fn nonempty_questions<'de, D>(deserializer: D) -> Result<IndexMap<String, Question>, D::Error>
where
    D: Deserializer<'de>,
{
    let questions = IndexMap::<String, Question>::deserialize(deserializer)?;
    if questions.is_empty() {
        return Err(serde::de::Error::custom(
            "At least one question is required.",
        ));
    }
    Ok(questions)
}

/// Body of `GET /v1/models`: `{"models": [...]}`.
///
/// [`Client::models`](crate::Client::models) returns
/// [`ListModelsResponse`](crate::ListModelsResponse), which adds the request ID
/// and raw body. This is the plain wire shape, for a server to serialize.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelsResponse {
    /// Model entries, in the order served.
    pub models: Vec<ModelMetadata>,
}

impl ModelsResponse {
    /// Builds a models listing.
    pub fn new(models: impl IntoIterator<Item = ModelMetadata>) -> Self {
        Self {
            models: models.into_iter().collect(),
        }
    }
}
