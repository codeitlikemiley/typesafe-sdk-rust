//! Rust SDK for [TypeSafe AI](https://typesafe.ai).
//!
//! # Quickstart
//!
//! ```no_run
//! use std::collections::HashMap;
//! use typesafe_sdk::{Choice, Question, Questions, TypeSafeClient};
//! use serde_json::json;
//!
//! let client = TypeSafeClient::new(
//!     Some("your-api-key".into()),
//!     None,
//!     None,
//!     None,
//!     None,
//!     None,
//! ).unwrap();
//! let mut questions: Questions = HashMap::new();
//! questions.insert(
//!     "category".into(),
//!     Question::Choice(Choice::new(
//!         Some(json!("What is this ticket about?")),
//!         HashMap::from([
//!             ("billing".into(), None),
//!             ("technical".into(), None),
//!             ("other".into(), None),
//!         ]),
//!     )),
//! );
//! let response = client.system_one(
//!     json!({"document": "I was charged twice. Please fix this ASAP."}),
//!     &questions,
//!     None,
//!     None,
//!     None,
//!     None,
//!     None,
//! ).unwrap();
//! let choices = response.choices();
//! println!("{}", choices["category"].choice);
//! ```

pub mod constants;
mod config;
mod error;
mod retry;
mod transport;
mod types;
mod client;

pub use client::{AsyncModels, AsyncTypeSafeClient, Models, TypeSafeClient};
pub use constants::{
    API_KEY_ENV, BASE_URL_ENV, DEFAULT_BASE_URL, DEFAULT_MODEL, DEFAULT_MODEL_ENV,
    DEFAULT_TIMEOUT_SECS, LOG_LEVEL_ENV,
};
pub use error::{
    TypeSafeApiConnectionError, TypeSafeApiError, TypeSafeApiResponseValidationError,
    TypeSafeApiTimeoutError, TypeSafeAuthenticationError, TypeSafeBadRequestError, TypeSafeError,
    TypeSafeInternalServerError, TypeSafeNotFoundError, TypeSafePermissionDeniedError,
    TypeSafeRateLimitError, TypeSafeUnprocessableEntityError,
};
pub use retry::RetryPolicy;
pub use types::{
    Answer, Choice, ChoiceAnswer, JsonContent, JsonValue, ListModelsResponse, ModelMetadata,
    Noul, NoulAnswer, NoulCriteria, Question, Questions, Score, ScoreAnswer, SystemOneResponse,
    Usage,
};
