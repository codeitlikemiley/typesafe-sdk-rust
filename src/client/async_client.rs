use std::collections::HashMap;
use std::time::Duration;

use reqwest::Client;
use serde_json::{Map, Value};

use crate::client::models::{AsyncModels, AsyncModelsSender};
use crate::config::Config;
use crate::constants::SYSTEM_ONE_PATH;
use crate::error::ApiFailure;
use crate::retry::{connection_error, timeout_error, with_retry_async, RetryPolicy};
use crate::transport::{endpoint, prepare, PreparedRequest};
use crate::types::{normalize_questions, ListModelsResponse, Questions, SystemOneResponse, JsonContent};

/// Async HTTP client for the TypeSafe AI API.
pub struct AsyncTypeSafeClient {
    config: Config,
    http: Client,
    retry: RetryPolicy,
}

impl AsyncTypeSafeClient {
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

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn models(&self) -> AsyncModels<'_> {
        AsyncModels::new(self as &dyn AsyncModelsSender)
    }

    pub async fn system_one(
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
            Value::Object(normalized.into_iter().collect()),
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
        self.send_system_one(req, retry).await
    }

    async fn send_system_one(
        &self,
        req: PreparedRequest,
        retry: Option<&RetryPolicy>,
    ) -> Result<SystemOneResponse, Box<dyn std::error::Error + Send + Sync>> {
        let policy = retry.unwrap_or(&self.retry);
        let http = self.http.clone();
        with_retry_async(policy, |attempt| {
            let req = req.clone_for_attempt(attempt);
            let http = http.clone();
            async move { execute_system_one(&http, req).await }
        })
        .await
        .map_err(map_api_failure)
    }

    async fn send_models_impl(
        &self,
        req: PreparedRequest,
        retry: Option<&RetryPolicy>,
    ) -> Result<ListModelsResponse, Box<dyn std::error::Error + Send + Sync>> {
        let policy = retry.unwrap_or(&self.retry);
        let http = self.http.clone();
        with_retry_async(policy, |attempt| {
            let req = req.clone_for_attempt(attempt);
            let http = http.clone();
            async move { execute_models(&http, req).await }
        })
        .await
        .map_err(map_api_failure)
    }
}

impl AsyncModelsSender for AsyncTypeSafeClient {
    fn config(&self) -> &Config {
        &self.config
    }

    fn send_models<'a>(
        &'a self,
        req: PreparedRequest,
        retry: Option<&'a RetryPolicy>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ListModelsResponse, Box<dyn std::error::Error + Send + Sync>>> + Send + 'a>,
    > {
        Box::pin(self.send_models_impl(req, retry))
    }
}

async fn execute_system_one(http: &Client, req: PreparedRequest) -> Result<SystemOneResponse, ApiFailure> {
    let (status, headers, body) = raw_request_async(http, &req).await?;
    SystemOneResponse::from_http(status, &headers, &body, Some(endpoint(&req.method, &req.url)))
}

async fn execute_models(http: &Client, req: PreparedRequest) -> Result<ListModelsResponse, ApiFailure> {
    let (status, headers, body) = raw_request_async(http, &req).await?;
    ListModelsResponse::from_http(status, &headers, &body, Some(endpoint(&req.method, &req.url)))
}

async fn raw_request_async(
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
    let response = builder.send().await.map_err(|e| {
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
    let body = response
        .bytes()
        .await
        .map_err(|e| connection_error(e.to_string()))?
        .to_vec();
    Ok((status, headers, body))
}

fn map_api_failure(err: ApiFailure) -> Box<dyn std::error::Error + Send + Sync> {
    use crate::error::map_status_error;
    match err {
        ApiFailure::Api(e) => map_status_error(e),
        ApiFailure::Connection(e) => Box::new(e),
        ApiFailure::Timeout(e) => Box::new(e),
        ApiFailure::Validation(e) => Box::new(e),
        ApiFailure::Sdk(e) => Box::new(e),
    }
}
