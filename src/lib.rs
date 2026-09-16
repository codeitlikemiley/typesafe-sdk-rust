//! Rust client for the TypeSafe AI API.
//!
//! Send named questions about text or structured state to System One, then read
//! typed answers. List available models with [`Client::models`].
//!
//! Set `TYPESAFE_API_KEY` or pass `api_key` to [`Client::builder`].

mod answer;
mod client;
mod config;
mod constants;
mod error;
mod json;
mod question;
mod request;
mod retry;

#[cfg(feature = "blocking")]
pub mod blocking;

pub use answer::{
    Answer, ChoiceAnswer, ListModelsResponse, ModelMetadata, NoulAnswer, ScoreAnswer,
    SystemOneResponse, Usage,
};
pub use client::{questions, Client, ClientBuilder, ModelsOpts, SystemOneOpts};

/// Python SDK name. Same type as [`Client`].
pub type TypeSafeClient = Client;
pub use constants::{
    API_KEY_ENV, BASE_URL_ENV, DEFAULT_BASE_URL, DEFAULT_MODEL, DEFAULT_MODEL_ENV,
    DEFAULT_TIMEOUT_SECS, LOG_LEVEL_ENV,
};
pub use error::{ApiError, ApiErrorKind, Error, ErrorBody};
pub use json::{IntoState, JsonContent};
pub use question::{NoulCriteria, Question};
pub use retry::RetryPolicy;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
