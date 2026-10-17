use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::error::Error;
use crate::json::JsonContent;

/// Optional yes and no descriptions for a noul question.
///
/// On the wire this is `{"true": ..., "false": ...}`, and serde uses the same keys.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NoulCriteria {
    #[serde(rename = "true", skip_serializing_if = "Option::is_none")]
    /// Meaning of a yes answer. Omitted from the wire when `None`.
    pub true_meaning: Option<JsonContent>,
    #[serde(rename = "false", skip_serializing_if = "Option::is_none")]
    /// Meaning of a no answer. Omitted from the wire when `None`.
    pub false_meaning: Option<JsonContent>,
}

impl NoulCriteria {
    /// Returns empty criteria with neither meaning set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the meaning of a yes answer.
    pub fn yes(mut self, value: impl Into<JsonContent>) -> Self {
        self.true_meaning = Some(value.into());
        self
    }

    /// Sets the meaning of a no answer.
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
///
/// Serializes to the wire object `{"type": ..., "instructions": ..., "criteria": ...}`.
/// Deserializing accepts exactly the objects the client would send: a nonempty
/// string `type`; nonempty `criteria` on `choice` and `score`; and on `noul`,
/// `criteria` that is an object if present, and `instructions` or at least one
/// non-null meaning. An object becomes `Noul`, `Choice`, `Score`, or `Box` only
/// when that variant serializes back to the same keys and values in the same
/// order. Typed variants write `type`, `instructions`, `criteria`, and noul
/// criteria `true` before `false`. Anything else, such as an unknown `type`, an
/// extra key, or `instructions` before `type`, stays `Raw`. Either way, a
/// deserialized question serializes back to the same keys and values in the
/// same order.
///
/// Image references, the text [`Image::reference`](crate::Image::reference)
/// returns, may go in any string of `instructions` or `criteria`.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum Question {
    /// Yes/no question. `instructions` asks the question, `criteria` optionally defines yes/no meanings.
    ///
    /// Needs `instructions` or at least one meaning in `criteria`. The client
    /// refuses to send it otherwise.
    Noul {
        /// Question text. `None` sends no instructions, which needs a meaning in `criteria`.
        instructions: Option<JsonContent>,
        /// Optional yes/no meanings. `None` omits criteria from the wire.
        criteria: Option<NoulCriteria>,
    },
    /// One-label question. `criteria` maps each label to an optional description.
    Choice {
        /// Question text.
        instructions: Option<JsonContent>,
        /// Map from label to its description. `None` sends a null description. Must be nonempty.
        criteria: IndexMap<String, Option<JsonContent>>,
    },
    /// Ordered-rubric question. `criteria` holds the rubric entries in score order.
    Score {
        /// Question text.
        instructions: Option<JsonContent>,
        /// Rubric entries in score order. Must be nonempty.
        criteria: Vec<JsonContent>,
    },
    /// Locate something in an image. The answer is an [`Answer::Box`](crate::Answer::Box).
    ///
    /// Serializes to `{"type":"box"}`, with `instructions` after `type` when
    /// set. The request must contain an image and use a vision-language model;
    /// the client does not check either.
    Box {
        /// What to locate in the image. `None` leaves `instructions` out.
        instructions: Option<JsonContent>,
    },
    /// Pass-through question map sent as-is after `type` validation.
    Raw(Map<String, Value>),
}

impl Question {
    /// Builds a noul question with instructions and no criteria.
    pub fn noul(instructions: impl Into<JsonContent>) -> Self {
        Self::Noul {
            instructions: Some(instructions.into()),
            criteria: None,
        }
    }

    /// Builds a noul question with no instructions and no criteria.
    ///
    /// Attach criteria with at least one meaning through `with_noul_criteria`
    /// before sending. A noul with neither is refused, as in the Python SDK.
    pub fn noul_bare() -> Self {
        Self::Noul {
            instructions: None,
            criteria: None,
        }
    }

    /// Attaches `criteria` to a noul question. Returns other variants unchanged.
    pub fn with_noul_criteria(self, criteria: NoulCriteria) -> Self {
        match self {
            Self::Noul { instructions, .. } => Self::Noul {
                instructions,
                criteria: Some(criteria),
            },
            other => other,
        }
    }

    /// Builds a choice question with instructions and label-to-description criteria. Errors at send time when empty.
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

    /// Builds a score question with instructions and ordered rubric criteria. Errors at send time when empty.
    pub fn score(
        instructions: impl Into<JsonContent>,
        criteria: impl IntoIterator<Item = impl Into<JsonContent>>,
    ) -> Self {
        Self::Score {
            instructions: Some(instructions.into()),
            criteria: criteria.into_iter().map(Into::into).collect(),
        }
    }

    /// Builds a box question that locates what `instructions` describes in an image.
    pub fn box_(instructions: impl Into<JsonContent>) -> Self {
        Self::Box {
            instructions: Some(instructions.into()),
        }
    }

    /// Builds a box question with no instructions: `{"type":"box"}`.
    pub fn box_bare() -> Self {
        Self::Box { instructions: None }
    }

    /// Builds a pass-through question from a raw map. Validated at send time.
    pub fn raw(value: Map<String, Value>) -> Self {
        Self::Raw(value)
    }

    /// Wraps a JSON object value as a raw question. Errors on non-object values.
    pub fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Object(map) => Ok(Self::Raw(map)),
            _ => Err(Error::sdk(
                "Question must be a question object or a dictionary with a nonempty string \"type\".",
            )),
        }
    }

    /// Errors when the client must not send this question under `name`.
    pub(crate) fn validate(&self, name: &str) -> Result<(), Error> {
        let invalid = match self {
            Self::Score { criteria, .. } if criteria.is_empty() => Some(Invalid::NoScores),
            Self::Choice { criteria, .. } if criteria.is_empty() => Some(Invalid::NoChoices),
            Self::Noul {
                instructions: None,
                criteria,
            } if criteria.as_ref().is_none_or(|criteria| {
                criteria.true_meaning.is_none() && criteria.false_meaning.is_none()
            }) =>
            {
                Some(Invalid::NoulEmpty)
            }
            Self::Raw(map) => check_raw(map).err(),
            _ => None,
        };
        match invalid {
            Some(invalid) => Err(Error::sdk(invalid.message(Some(name)))),
            None => Ok(()),
        }
    }

    pub(crate) fn to_wire(&self, name: &str) -> Result<Value, Error> {
        self.validate(name)?;
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
            Self::Box { instructions } => {
                let mut map = Map::new();
                map.insert("type".to_string(), Value::String("box".to_string()));
                if let Some(instructions) = instructions {
                    map.insert(
                        "instructions".to_string(),
                        Value::from(instructions.clone()),
                    );
                }
                Ok(Value::Object(map))
            }
            Self::Raw(map) => Ok(Value::Object(map.clone())),
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

impl<'de> Deserialize<'de> for Question {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map = Map::<String, Value>::deserialize(deserializer)?;
        check_raw(&map).map_err(|invalid| serde::de::Error::custom(invalid.message(None)))?;
        Ok(typed(&map).unwrap_or(Self::Raw(map)))
    }
}

/// The typed question that serializes back to exactly `map`: the same keys and
/// values in the same order. `None` when no typed variant does, so the caller
/// keeps `Raw` and a round trip neither drops nor reorders a key.
fn typed(map: &Map<String, Value>) -> Option<Question> {
    if map
        .keys()
        .any(|key| !matches!(key.as_str(), "type" | "instructions" | "criteria"))
    {
        return None;
    }
    let instructions = match map.get("instructions") {
        Some(value) => Some(content(value)?),
        None => None,
    };
    let criteria = map.get("criteria");
    let question = match map.get("type").and_then(Value::as_str)? {
        "noul" => {
            let criteria = match criteria {
                None => None,
                Some(Value::Object(meanings)) => {
                    if meanings.keys().any(|key| key != "true" && key != "false") {
                        return None;
                    }
                    let meaning = |key: &str| match meanings.get(key) {
                        None => Some(None),
                        Some(value) => content(value).map(Some),
                    };
                    Some(NoulCriteria {
                        true_meaning: meaning("true")?,
                        false_meaning: meaning("false")?,
                    })
                }
                Some(_) => return None,
            };
            Question::Noul {
                instructions,
                criteria,
            }
        }
        "choice" => {
            let Some(Value::Object(labels)) = criteria else {
                return None;
            };
            let criteria = labels
                .iter()
                .map(|(label, description)| {
                    let description = match description {
                        Value::Null => None,
                        other => Some(content(other)?),
                    };
                    Some((label.clone(), description))
                })
                .collect::<Option<IndexMap<_, _>>>()?;
            Question::Choice {
                instructions,
                criteria,
            }
        }
        "score" => {
            let Some(Value::Array(items)) = criteria else {
                return None;
            };
            let criteria = items.iter().map(content).collect::<Option<Vec<_>>>()?;
            Question::Score {
                instructions,
                criteria,
            }
        }
        "box" if criteria.is_none() => Question::Box { instructions },
        _ => return None,
    };
    // Holding the same keys and values is not enough: `instructions` before
    // `type`, or `false` before `true`, would come back reordered.
    match question.to_wire("question") {
        Ok(Value::Object(wire)) if same_object(&wire, map) => Some(question),
        _ => None,
    }
}

/// Whether two objects serialize to the same bytes: the same keys in the same
/// order, with values that compare the same way all the way down.
fn same_object(a: &Map<String, Value>, b: &Map<String, Value>) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|((a_key, a_value), (b_key, b_value))| {
            a_key == b_key && same_value(a_value, b_value)
        })
}

fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => same_object(a, b),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_value(a, b))
        }
        _ => a == b,
    }
}

fn content(value: &Value) -> Option<JsonContent> {
    JsonContent::from_value(value.clone()).ok()
}

/// Validates every question under its name, in order. Errors before any network call.
pub(crate) fn normalize_questions<I, K>(questions: I) -> Result<IndexMap<String, Question>, Error>
where
    I: IntoIterator<Item = (K, Question)>,
    K: Into<String>,
{
    let mut out = IndexMap::new();
    for (name, question) in questions {
        let name = name.into();
        question.validate(&name)?;
        out.insert(name, question);
    }
    if out.is_empty() {
        return Err(Error::sdk("At least one question is required."));
    }
    Ok(out)
}

/// Why the client refuses to send a question map. Shared by send-time
/// validation and `Deserialize`, so a server accepts what the client sends.
#[derive(Clone, Copy)]
enum Invalid {
    /// `type` is missing, empty, or not a string.
    Type,
    /// A `choice` or `score` question has no `criteria` key.
    Criteria,
    /// A `score` question has no scores.
    NoScores,
    /// A `choice` question has no choices.
    NoChoices,
    /// A `noul` question's `criteria` is not an object.
    NoulCriteriaType,
    /// A `noul` question has neither instructions nor a described outcome.
    NoulEmpty,
}

impl Invalid {
    fn message(self, name: Option<&str>) -> String {
        let name = name.map(|name| format!(" \"{name}\"")).unwrap_or_default();
        match self {
            Self::Type => format!(
                "Question{name} must be a question object or a dictionary with a nonempty string \"type\"."
            ),
            Self::Criteria => format!("Question{name} requires \"criteria\"."),
            Self::NoScores => {
                format!("Score question{name} has no criteria; at least one score is required.")
            }
            Self::NoChoices => {
                format!("Choice question{name} has no criteria; at least one choice is required.")
            }
            Self::NoulCriteriaType => format!("Noul question{name} criteria must be an object."),
            Self::NoulEmpty => format!(
                "Noul question{name} has neither instructions nor criteria; at least one is required."
            ),
        }
    }
}

fn check_raw(map: &Map<String, Value>) -> Result<(), Invalid> {
    let type_name = match map.get("type") {
        Some(Value::String(text)) if !text.is_empty() => text.as_str(),
        _ => return Err(Invalid::Type),
    };
    if matches!(type_name, "choice" | "score") && !map.contains_key("criteria") {
        return Err(Invalid::Criteria);
    }
    let criteria = map.get("criteria").unwrap_or(&Value::Null);
    match type_name {
        "score" if falsy(criteria) => Err(Invalid::NoScores),
        "choice" if falsy(criteria) => Err(Invalid::NoChoices),
        "noul" => {
            let meanings = match criteria {
                Value::Null => None,
                Value::Object(meanings) => Some(meanings),
                _ => return Err(Invalid::NoulCriteriaType),
            };
            let instructions = map.get("instructions").unwrap_or(&Value::Null);
            let described =
                meanings.is_some_and(|meanings| meanings.values().any(|v| !v.is_null()));
            if instructions.is_null() && !described {
                return Err(Invalid::NoulEmpty);
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Whether Python treats the decoded JSON value as false: `null`, `false`, `0`,
/// `""`, `[]`, or `{}`.
fn falsy(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(flag) => !flag,
        Value::Number(number) => number.as_f64() == Some(0.0),
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_questions_omit_default_fields() {
        let noul = Question::noul_bare()
            .with_noul_criteria(NoulCriteria::new().no("x"))
            .to_wire("q")
            .unwrap();
        assert_eq!(
            noul,
            serde_json::json!({"type": "noul", "criteria": {"false": "x"}})
        );
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
