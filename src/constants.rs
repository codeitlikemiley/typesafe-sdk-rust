//! Public environment-variable names and client defaults.

/// Environment variable for the API key.
pub const API_KEY_ENV: &str = "TYPESAFE_API_KEY";
/// Environment variable for the API base URL.
pub const BASE_URL_ENV: &str = "TYPESAFE_BASE_URL";
/// Environment variable for the default model.
pub const DEFAULT_MODEL_ENV: &str = "TYPESAFE_DEFAULT_MODEL";
/// Environment variable for the logging level.
pub const LOG_LEVEL_ENV: &str = "TYPESAFE_LOG_LEVEL";

/// Default API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
/// Default model name.
pub const DEFAULT_MODEL: &str = "jev-latest";
/// Default timeout in seconds for each HTTP operation.
pub const DEFAULT_TIMEOUT_SECS: f64 = 10.0;

pub(crate) const SYSTEM_ONE_PATH: &str = "/v1/systemone";
pub(crate) const MODELS_PATH: &str = "/v1/models";
pub(crate) const SDK_NAME: &str = "typesafe-sdk";
pub(crate) const JSON_CONTENT_TYPE: &str = "application/json";
pub(crate) const MAX_ERROR_BODY_LENGTH: usize = 200;

pub(crate) const AUTHORIZATION_HEADER: &str = "Authorization";
pub(crate) const ACCEPT_HEADER: &str = "Accept";
pub(crate) const CONTENT_TYPE_HEADER: &str = "Content-Type";
pub(crate) const USER_AGENT_HEADER: &str = "User-Agent";
pub(crate) const SDK_HEADER: &str = "X-TypeSafe-SDK";
pub(crate) const RUNTIME_HEADER: &str = "X-TypeSafe-Runtime";
pub(crate) const RETRY_COUNT_HEADER: &str = "X-TypeSafe-Retry-Count";
pub(crate) const REQUEST_ID_HEADER: &str = "x-typesafe-request-id";
pub(crate) const RETRY_AFTER_HEADER: &str = "retry-after";
pub(crate) const RETRY_AFTER_MS_HEADER: &str = "retry-after-ms";
