use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Error;

/// Text, a JSON object, or a JSON array. Top-level null is rejected.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonContent {
    String(String),
    Array(Vec<Value>),
    Object(serde_json::Map<String, Value>),
}

impl JsonContent {
    pub fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Null => Err(Error::sdk("state must be a string, object, or array")),
            Value::String(text) => Ok(Self::String(text)),
            Value::Array(items) => Ok(Self::Array(items)),
            Value::Object(map) => Ok(Self::Object(map)),
            other => Err(Error::sdk(format!(
                "state must be a string, object, or array, not {}",
                json_kind(&other)
            ))),
        }
    }
}

impl From<String> for JsonContent {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for JsonContent {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<serde_json::Map<String, Value>> for JsonContent {
    fn from(value: serde_json::Map<String, Value>) -> Self {
        Self::Object(value)
    }
}

impl From<Vec<Value>> for JsonContent {
    fn from(value: Vec<Value>) -> Self {
        Self::Array(value)
    }
}

/// Accepts a string, object, array, or [`JsonContent`].
pub trait IntoState {
    fn into_state(self) -> Result<JsonContent, Error>;
}

impl IntoState for JsonContent {
    fn into_state(self) -> Result<JsonContent, Error> {
        Ok(self)
    }
}

impl IntoState for String {
    fn into_state(self) -> Result<JsonContent, Error> {
        Ok(JsonContent::String(self))
    }
}

impl IntoState for &str {
    fn into_state(self) -> Result<JsonContent, Error> {
        Ok(JsonContent::String(self.to_string()))
    }
}

impl IntoState for Value {
    fn into_state(self) -> Result<JsonContent, Error> {
        JsonContent::from_value(self)
    }
}

impl TryFrom<Value> for JsonContent {
    type Error = Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        Self::from_value(value)
    }
}

impl From<JsonContent> for Value {
    fn from(value: JsonContent) -> Self {
        match value {
            JsonContent::String(text) => Value::String(text),
            JsonContent::Array(items) => Value::Array(items),
            JsonContent::Object(map) => Value::Object(map),
        }
    }
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
