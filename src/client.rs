use std::time::{Duration, Instant};

use http::HeaderMap;
use indexmap::IndexMap;
use reqwest::Client as HttpClient;
use serde_json::{Map, Value};

use crate::answer::{decode_models, decode_system_one, ListModelsResponse, SystemOneResponse};
use crate::config::Config;
use crate::constants::{MODELS_PATH, SYSTEM_ONE_PATH};
use crate::error::{api_error, deserialize_body, format_endpoint, Error};
use crate::json::IntoState;
use crate::question::{normalize_questions, Question};
use crate::request::{merge_extra_body, prepare, set_retry_count, PreparedRequest};
use crate::retry::RetryPolicy;

/// Asynchronous TypeSafe client.
#[derive(Clone, Debug)]
pub struct Client {
    config: Config,
    http: HttpClient,
    retry: RetryPolicy,
}

#[derive(Clone, Debug, Default)]
pub struct ClientBuilder {
    api_key: Option<String>,
    model: Option<String>,
    retry: Option<RetryPolicy>,
    timeout: Option<Duration>,
    headers: HeaderMap,
    base_url: Option<String>,
    http: Option<HttpClient>,
}

impl ClientBuilder {
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = Some(retry);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Result<Self, Error> {
        let name = http::HeaderName::from_bytes(name.as_ref().as_bytes())
            .map_err(|_| Error::sdk("invalid header name"))?;
        let value = http::HeaderValue::from_str(value.as_ref())
            .map_err(|_| Error::sdk("invalid header value"))?;
        self.headers.insert(name, value);
        Ok(self)
    }

    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn http_client(mut self, http: HttpClient) -> Self {
        self.http = Some(http);
        self
    }

    pub fn build(self) -> Result<Client, Error> {
        if let Some(retry) = &self.retry {
            retry.validate()?;
        }
        let config = Config::resolve(
            self.api_key,
            self.base_url,
            self.model,
            self.timeout,
            self.headers,
        )?;
        let http = match self.http {
            Some(http) => http,
            None => HttpClient::builder()
                .timeout(config.timeout)
                .build()
                .map_err(|error| Error::Connection {
                    message: error.to_string(),
                })?,
        };
        Ok(Client {
            config,
            http,
            retry: self.retry.unwrap_or_default(),
        })
    }
}

impl Client {
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    pub fn from_env() -> Result<Self, Error> {
        Self::builder().build()
    }

    pub fn new(api_key: impl Into<String>) -> Result<Self, Error> {
        Self::builder().api_key(api_key).build()
    }

    pub async fn system_one<S, I, K>(
        &self,
        state: S,
        questions: I,
    ) -> Result<SystemOneResponse, Error>
    where
        S: IntoState,
        I: IntoIterator<Item = (K, Question)>,
        K: Into<String>,
    {
        self.system_one_opts(state, questions, SystemOneOpts::default())
            .await
    }

    pub async fn system_one_opts<S, I, K>(
        &self,
        state: S,
        questions: I,
        opts: SystemOneOpts,
    ) -> Result<SystemOneResponse, Error>
    where
        S: IntoState,
        I: IntoIterator<Item = (K, Question)>,
        K: Into<String>,
    {
        let mut body = Map::new();
        body.insert("state".to_string(), Value::from(state.into_state()?));
        body.insert(
            "model".to_string(),
            Value::String(
                opts.model
                    .unwrap_or_else(|| self.config.default_model.clone()),
            ),
        );
        let questions = normalize_questions(questions)?;
        body.insert(
            "questions".to_string(),
            Value::Object(questions.into_iter().collect()),
        );
        let body = merge_extra_body(body, opts.extra_body);
        let request = prepare(
            &self.config,
            "POST",
            SYSTEM_ONE_PATH,
            Some(&body),
            opts.timeout,
            &opts.extra_headers,
        )?;
        let retry = opts.retry.unwrap_or_else(|| self.retry.clone());
        retry.validate()?;
        let (status, headers, endpoint, bytes) = self.send(request, &retry).await?;
        decode_system_one(status, headers, Some(endpoint), bytes)
    }

    pub async fn models(&self) -> Result<ListModelsResponse, Error> {
        self.models_opts(ModelsOpts::default()).await
    }

    pub async fn models_opts(&self, opts: ModelsOpts) -> Result<ListModelsResponse, Error> {
        let request = prepare(
            &self.config,
            "GET",
            MODELS_PATH,
            None,
            opts.timeout,
            &opts.extra_headers,
        )?;
        let retry = opts.retry.unwrap_or_else(|| self.retry.clone());
        retry.validate()?;
        let (status, headers, endpoint, bytes) = self.send(request, &retry).await?;
        decode_models(status, headers, Some(endpoint), bytes)
    }

    async fn send(
        &self,
        request: PreparedRequest,
        retry: &RetryPolicy,
    ) -> Result<(u16, HeaderMap, String, bytes::Bytes), Error> {
        let started = Instant::now();
        let mut attempt = 0u32;
        loop {
            let mut headers = request.headers.clone();
            set_retry_count(&mut headers, attempt);
            let result = match self.execute(&request, headers).await {
                Ok((status, headers, endpoint, bytes)) if (200..300).contains(&status) => {
                    return Ok((status, headers, endpoint, bytes));
                }
                Ok((status, headers, endpoint, bytes)) => Err(api_error(
                    status,
                    deserialize_body(&bytes),
                    headers,
                    Some(endpoint),
                )),
                Err(error) => Err(error),
            };
            match result {
                Ok(success) => return Ok(success),
                Err(error) if retry.retryable(&error) && attempt < retry.max_retries => {
                    let wait = retry.wait(attempt + 1, &error, fastrand_jitter());
                    if let Some(budget) = retry.timeout {
                        if started.elapsed() + wait >= budget {
                            return Err(error);
                        }
                    }
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn execute(
        &self,
        request: &PreparedRequest,
        headers: HeaderMap,
    ) -> Result<(u16, HeaderMap, String, bytes::Bytes), Error> {
        let mut builder = self
            .http
            .request(
                reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|_| {
                    Error::sdk(format!("unsupported HTTP method {}", request.method))
                })?,
                &request.url,
            )
            .headers(headers)
            .timeout(request.timeout);
        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }
        let response = builder
            .send()
            .await
            .map_err(map_reqwest_error(request.timeout))?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let endpoint = format_endpoint(&request.method, &request.url);
        let bytes = response
            .bytes()
            .await
            .map_err(map_reqwest_error(request.timeout))?;
        Ok((status, headers, endpoint, bytes))
    }
}

fn map_reqwest_error(timeout: Duration) -> impl FnOnce(reqwest::Error) -> Error {
    move |error| {
        if error.is_timeout() {
            Error::Timeout { timeout }
        } else {
            Error::Connection {
                message: error.without_url().to_string(),
            }
        }
    }
}

fn fastrand_jitter() -> f64 {
    fastrand::f64()
}

#[derive(Clone, Debug, Default)]
pub struct SystemOneOpts {
    pub model: Option<String>,
    pub retry: Option<RetryPolicy>,
    pub timeout: Option<Duration>,
    pub extra_headers: HeaderMap,
    pub extra_body: Option<Map<String, Value>>,
}

#[derive(Clone, Debug, Default)]
pub struct ModelsOpts {
    pub retry: Option<RetryPolicy>,
    pub timeout: Option<Duration>,
    pub extra_headers: HeaderMap,
}

/// Convenience constructor for a question map.
pub fn questions<I, K>(items: I) -> IndexMap<String, Question>
where
    I: IntoIterator<Item = (K, Question)>,
    K: Into<String>,
{
    items
        .into_iter()
        .map(|(name, question)| (name.into(), question))
        .collect()
}
