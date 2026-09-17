use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::Error;
use crate::json::JsonContent;

/// Optional yes and no descriptions for a noul question.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct NoulCriteria {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub true_meaning: Option<JsonContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_meaning: Option<JsonContent>,
}

impl NoulCriteria {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn yes(mut self, value: impl Into<JsonContent>) -> Self {
        self.true_meaning = Some(value.into());
        self
    }

    pub fn no(mut self, value: impl Into<JsonContent>) -> Self {
        self.false_meaning = Some(value.into());
        self
    }

    fn to_json(&self) -> Value {
        let mut map = Map::new();
        if let Some(value) = &self.true_meaning {
            map.insert("true".to_string(), Value::from(value.clone()));
        }
        if let Some(value) = &self.false_meaning {
            map.insert("false".to_string(), Value::from(value.clone()));
        }
        Value::Object(map)
    }
}

/// A named question sent to System One.
#[derive(Clone, Debug, PartialEq)]
pub enum Question {
    Noul {
        instructions: Option<JsonContent>,
        criteria: Option<NoulCriteria>,
    },
    Choice {
        instructions: Option<JsonContent>,
        criteria: IndexMap<String, Option<JsonContent>>,
    },
    Score {
        instructions: Option<JsonContent>,
        criteria: Vec<JsonContent>,
    },
    Raw(Map<String, Value>),
}

impl Question {
    pub fn noul(instructions: impl Into<JsonContent>) -> Self {
        Self::Noul {
            instructions: Some(instructions.into()),
            criteria: None,
        }
    }

    pub fn noul_bare() -> Self {
        Self::Noul {
            instructions: None,
            criteria: None,
        }
    }

    pub fn with_noul_criteria(self, criteria: NoulCriteria) -> Self {
        match self {
            Self::Noul { instructions, .. } => Self::Noul {
                instructions,
                criteria: Some(criteria),
            },
            other => other,
        }
    }

    pub fn choice(
        instructions: impl Into<JsonContent>,
        criteria: impl IntoIterator<Item = (impl Into<String>, Option<JsonContent>)>,
    ) -> Self {
        Self::Choice {
            instructions: Some(instructions.into()),
            criteria: criteria
                .into_iter()
                .map(|(name, description)| (name.into(), description))
                .collect(),
        }
    }

    pub fn score(
        instructions: impl Into<JsonContent>,
        criteria: impl IntoIterator<Item = impl Into<JsonContent>>,
    ) -> Self {
        Self::Score {
            instructions: Some(instructions.into()),
            criteria: criteria.into_iter().map(Into::into).collect(),
        }
    }

    pub fn raw(value: Map<String, Value>) -> Self {
        Self::Raw(value)
    }

    pub fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Object(map) => Ok(Self::Raw(map)),
            _ => Err(Error::sdk(
                "Question must be a question object or a dictionary with a nonempty string \"type\".",
            )),
        }
    }

    pub(crate) fn to_wire(&self, name: &str) -> Result<Value, Error> {
        match self {
            Self::Noul {
                instructions,
                criteria,
            } => {
                let mut map = Map::new();
                map.insert("type".to_string(), Value::String("noul".to_string()));
                if let Some(instructions) = instructions {
                    map.insert(
                        "instructions".to_string(),
                        Value::from(instructions.clone()),
                    );
                }
                if let Some(criteria) = criteria {
                    map.insert("criteria".to_string(), criteria.to_json());
                }
                Ok(Value::Object(map))
            }
            Self::Choice {
                instructions,
                criteria,
            } => {
                let mut map = Map::new();
                map.insert("type".to_string(), Value::String("choice".to_string()));
                if let Some(instructions) = instructions {
                    map.insert(
                        "instructions".to_string(),
                        Value::from(instructions.clone()),
                    );
                }
                let mut criteria_map = Map::new();
                for (label, description) in criteria {
                    criteria_map.insert(
                        label.clone(),
                        description.clone().map(Value::from).unwrap_or(Value::Null),
                    );
                }
                map.insert("criteria".to_string(), Value::Object(criteria_map));
                Ok(Value::Object(map))
            }
            Self::Score {
                instructions,
                criteria,
            } => {
                if criteria.is_empty() {
                    return Err(Error::sdk(format!(
                        "Score question \"{name}\" has no criteria; at least one score is required."
                    )));
                }
                let mut map = Map::new();
                map.insert("type".to_string(), Value::String("score".to_string()));
                if let Some(instructions) = instructions {
                    map.insert(
                        "instructions".to_string(),
                        Value::from(instructions.clone()),
                    );
                }
                map.insert(
                    "criteria".to_string(),
                    Value::Array(criteria.iter().cloned().map(Value::from).collect()),
                );
                Ok(Value::Object(map))
            }
            Self::Raw(map) => {
                validate_raw(name, map)?;
                Ok(Value::Object(map.clone()))
            }
        }
    }
}

impl Serialize for Question {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_wire("question")
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

pub(crate) fn normalize_questions<I, K>(questions: I) -> Result<IndexMap<String, Value>, Error>
where
    I: IntoIterator<Item = (K, Question)>,
    K: Into<String>,
{
    let mut out = IndexMap::new();
    for (name, question) in questions {
        let name = name.into();
        out.insert(name.clone(), question.to_wire(&name)?);
    }
    if out.is_empty() {
        return Err(Error::sdk("At least one question is required."));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_questions_omit_default_fields() {
        let noul = Question::noul_bare().to_wire("q").unwrap();
        assert_eq!(noul, serde_json::json!({"type": "noul"}));
        let choice = Question::choice("Tone?", [("calm", None)])
            .to_wire("q")
            .unwrap();
        assert_eq!(
            choice,
            serde_json::json!({"type": "choice", "instructions": "Tone?", "criteria": {"calm": null}})
        );
    }

    #[test]
    fn raw_score_without_criteria_fails() {
        let raw = serde_json::json!({"type": "score", "instructions": "?"})
            .as_object()
            .unwrap()
            .clone();
        let error = Question::raw(raw).to_wire("rating").unwrap_err();
        assert!(error.to_string().contains("requires \"criteria\""));
    }
}

fn validate_raw(name: &str, map: &Map<String, Value>) -> Result<(), Error> {
    let type_name = match map.get("type") {
        Some(Value::String(text)) if !text.is_empty() => text.as_str(),
        _ => {
            return Err(Error::sdk(format!(
                "Question \"{name}\" must be a question object or a dictionary with a nonempty string \"type\"."
            )));
        }
    };
    if matches!(type_name, "choice" | "score") && !map.contains_key("criteria") {
        return Err(Error::sdk(format!(
            "Question \"{name}\" requires \"criteria\"."
        )));
    }
    if type_name == "score" {
        match map.get("criteria") {
            Some(Value::Array(items)) if items.is_empty() => {
                return Err(Error::sdk(format!(
                    "Score question \"{name}\" has no criteria; at least one score is required."
                )));
            }
            _ => {}
        }
    }
    Ok(())
}
