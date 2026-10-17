/// Environment variable for the API key.
pub const API_KEY_ENV: &str = "TYPESAFE_API_KEY";

/// Environment variable for the API base URL.
pub const BASE_URL_ENV: &str = "TYPESAFE_BASE_URL";

/// Environment variable for the default model.
pub const DEFAULT_MODEL_ENV: &str = "TYPESAFE_DEFAULT_MODEL";

/// Environment variable that selects the provider for
/// [`Client::from_env`](crate::Client::from_env): `typesafe` (the default),
/// `clef`, or `openai`.
pub const PROVIDER_ENV: &str = "TYPESAFE_PROVIDER";

/// Environment variable for the Cloudflare API token of the Clef provider.
#[cfg(feature = "clef")]
pub const CLOUDFLARE_API_TOKEN_ENV: &str = "CLOUDFLARE_API_TOKEN";

/// Environment variable for the Cloudflare account ID of the Clef provider.
#[cfg(feature = "clef")]
pub const CLOUDFLARE_ACCOUNT_ID_ENV: &str = "CLOUDFLARE_ACCOUNT_ID";

/// Default base URL of the Clef provider, the Cloudflare API.
#[cfg(feature = "clef")]
pub const CLEF_DEFAULT_BASE_URL: &str = "https://api.cloudflare.com/client/v4";

/// Default model of the Clef provider. `clef-flash` is the other Workers AI slug.
#[cfg(feature = "clef")]
pub const CLEF_DEFAULT_MODEL: &str = "clef";

/// Environment variable for the API key of the OpenAI Decisions provider.
#[cfg(feature = "openai-decisions")]
pub const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";

/// Default base URL of the OpenAI Decisions provider.
#[cfg(feature = "openai-decisions")]
pub const OPENAI_DEFAULT_BASE_URL: &str = "https://api.openai.com";

/// Default model of the OpenAI Decisions provider.
#[cfg(feature = "openai-decisions")]
pub const OPENAI_DEFAULT_MODEL: &str = "gpt-6-luna";

/// Path of the OpenAI Decisions endpoint (`POST`), appended to the base URL.
#[cfg(feature = "openai-decisions")]
pub const OPENAI_DECISIONS_PATH: &str = "/v1/decisions";

/// Environment variable for the logging level.
pub const LOG_LEVEL_ENV: &str = "TYPESAFE_LOG_LEVEL";

/// Default API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";

/// Default model name.
pub const DEFAULT_MODEL: &str = "jev-latest";

/// Default timeout in seconds for each HTTP operation.
pub const DEFAULT_TIMEOUT_SECS: f64 = 10.0;

/// Path of the System One endpoint (`POST`), appended to the base URL.
pub const SYSTEM_ONE_PATH: &str = "/v1/systemone";

/// Path of the models endpoint (`GET`), appended to the base URL.
pub const MODELS_PATH: &str = "/v1/models";

pub(crate) const SDK_NAME: &str = "typesafe-sdk";
pub(crate) const JSON_CONTENT_TYPE: &str = "application/json";
pub(crate) const MAX_ERROR_BODY_LENGTH: usize = 200;
pub(crate) const AUTHORIZATION_HEADER: &str = "authorization";
pub(crate) const ACCEPT_HEADER: &str = "accept";
pub(crate) const CONTENT_TYPE_HEADER: &str = "content-type";
#[cfg_attr(not(feature = "zstd"), allow(dead_code))]
pub(crate) const CONTENT_ENCODING_HEADER: &str = "content-encoding";
/// Bodies shorter than this are sent uncompressed.
#[cfg_attr(not(feature = "zstd"), allow(dead_code))]
pub(crate) const MIN_COMPRESSED_BODY_LENGTH: usize = 1024;
pub(crate) const USER_AGENT_HEADER: &str = "user-agent";
pub(crate) const SDK_HEADER: &str = "x-typesafe-sdk";
pub(crate) const RUNTIME_HEADER: &str = "x-typesafe-runtime";
pub(crate) const RETRY_COUNT_HEADER: &str = "x-typesafe-retry-count";
pub(crate) const REQUEST_ID_HEADER: &str = "x-typesafe-request-id";
pub(crate) const RETRY_AFTER_HEADER: &str = "retry-after";
pub(crate) const RETRY_AFTER_MS_HEADER: &str = "retry-after-ms";

pub(crate) const SECRET_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "api-key",
    "cookie",
    "set-cookie",
];
