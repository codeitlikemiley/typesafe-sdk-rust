use std::collections::HashMap;
use std::time::Duration;

use reqwest::blocking::Client;
use serde_json::{Map, Value};

use crate::client::models::{Models, ModelsSender};
use crate::config::Config;
use crate::constants::SYSTEM_ONE_PATH;
use crate::error::{map_status_error, ApiFailure};
use crate::retry::{connection_error, timeout_error, with_retry, RetryPolicy};
use crate::transport::{endpoint, prepare, PreparedRequest};
use crate::types::{normalize_questions, ListModelsResponse, Questions, SystemOneResponse, JsonContent};

/// Blocking HTTP client for the TypeSafe AI API.
pub struct TypeSafeClient {
    config: Config,
    http: Client,
    retry: RetryPolicy,
}

impl TypeSafeClient {
    /// Build a client using `TYPESAFE_*` environment variables for unset options.
    pub fn from_env() -> Result<Self, crate::error::TypeSafeError> {
        Self::new(None, None, None, None, None, None)
    }

    pub fn new(
        api_key: Option<String>,
        model: Option<String>,
        retry: Option<RetryPolicy>,
        timeout_secs: Option<f64>,
        headers: Option<HashMap<String, String>>,
        base_url: Option<String>,
    ) -> Result<Self, crate::error::TypeSafeError> {
        let config = Config::resolve(api_key, base_url, model, timeout_secs, headers)?;
        let retry = retry.unwrap_or_default();
        retry.validate()?;
        let http = Client::builder()
            .timeout(Duration::from_secs_f64(config.timeout_secs))
            .build()
            .map_err(|e| crate::error::TypeSafeError::new(e.to_string()))?;
        Ok(Self { config, http, retry })
    }

    pub fn models(&self) -> Models<'_> {
        Models::new(self as &dyn ModelsSender)
    }

    pub fn system_one(
        &self,
        state: JsonContent,
        questions: &Questions,
        model: Option<String>,
        retry: Option<&RetryPolicy>,
        timeout_secs: Option<f64>,
        extra_headers: Option<&HashMap<String, String>>,
        extra_body: Option<&HashMap<String, Option<Value>>>,
    ) -> Result<SystemOneResponse, Box<dyn std::error::Error + Send + Sync>> {
        let normalized = normalize_questions(questions)?;
        let mut body = Map::new();
        body.insert("state".into(), state);
        body.insert(
            "model".into(),
            Value::String(model.unwrap_or_else(|| self.config.default_model.clone())),
        );
        body.insert(
            "questions".into(),
            Value::Object(
                normalized
                    .into_iter()
                    .map(|(k, v)| (k, v))
                    .collect(),
            ),
        );
        if let Some(extra) = extra_body {
            for (k, v) in extra {
                body.insert(k.clone(), v.clone().unwrap_or(Value::Null));
            }
        }
        let req = prepare(
            &self.config,
            "POST",
            SYSTEM_ONE_PATH,
            Some(Value::Object(body)),
            timeout_secs,
            extra_headers,
        )?;
        self.send_system_one(req, retry)
    }

    fn send_system_one(
        &self,
        req: PreparedRequest,
        retry: Option<&RetryPolicy>,
    ) -> Result<SystemOneResponse, Box<dyn std::error::Error + Send + Sync>> {
        let policy = retry.unwrap_or(&self.retry);
        let http = self.http.clone();
        with_retry(policy, |attempt| self.execute(req.clone_for_attempt(attempt), &http))
            .map_err(map_api_failure)
    }

    fn execute(
        &self,
        req: PreparedRequest,
        http: &Client,
    ) -> Result<SystemOneResponse, ApiFailure> {
        match raw_request(http, &req) {
            Ok((status, headers, body)) => {
                SystemOneResponse::from_http(
                    status,
                    &headers,
                    &body,
                    Some(endpoint(&req.method, &req.url)),
                )
            }
            Err(f) => Err(f),
        }
    }

    fn execute_models(
        &self,
        req: PreparedRequest,
        http: &Client,
    ) -> Result<ListModelsResponse, ApiFailure> {
        match raw_request(http, &req) {
            Ok((status, headers, body)) => {
                ListModelsResponse::from_http(
                    status,
                    &headers,
                    &body,
                    Some(endpoint(&req.method, &req.url)),
                )
            }
            Err(f) => Err(f),
        }
    }
}

impl ModelsSender for TypeSafeClient {
    fn config(&self) -> &Config {
        &self.config
    }

    fn send_models(
        &self,
        req: PreparedRequest,
        retry: Option<&RetryPolicy>,
    ) -> Result<ListModelsResponse, Box<dyn std::error::Error + Send + Sync>> {
        let policy = retry.unwrap_or(&self.retry);
        let http = self.http.clone();
        with_retry(policy, |attempt| self.execute_models(req.clone_for_attempt(attempt), &http))
            .map_err(map_api_failure)
    }
}

fn raw_request(
    http: &Client,
    req: &PreparedRequest,
) -> Result<(u16, HashMap<String, String>, Vec<u8>), ApiFailure> {
    let mut builder = http
        .request(req.method.parse().unwrap_or(reqwest::Method::GET), &req.url)
        .timeout(Duration::from_secs_f64(req.timeout_secs));
    for (k, v) in &req.headers {
        builder = builder.header(k, v);
    }
    if let Some(body) = &req.body {
        builder = builder.body(body.clone());
    }
    let response = builder.send().map_err(|e| {
        if e.is_timeout() {
            timeout_error(req.timeout_secs)
        } else {
            connection_error(format!("Connection error: {}", e))
        }
    })?;
    let status = response.status().as_u16();
    let headers: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let body = response.bytes().map_err(|e| connection_error(e.to_string()))?.to_vec();
    Ok((status, headers, body))
}

fn map_api_failure(err: ApiFailure) -> Box<dyn std::error::Error + Send + Sync> {
    match err {
        ApiFailure::Api(e) => map_status_error(e),
        ApiFailure::Connection(e) => Box::new(e),
        ApiFailure::Timeout(e) => Box::new(e),
        ApiFailure::Validation(e) => Box::new(e),
        ApiFailure::Sdk(e) => Box::new(e),
    }
}
