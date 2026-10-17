use std::marker::PhantomData;
use std::time::{Duration, Instant};

use http::HeaderMap;
use indexmap::IndexMap;
use reqwest::Client as HttpClient;
use serde_json::{Map, Value};

use crate::answer::{ListModelsResponse, SystemOneResponse, decode_models, decode_system_one};
use crate::asset::{AttachGuard, Attached, Image, pending_assets, resolve_assets, snapshot};
use crate::config::{Compression, Config};
use crate::constants::{API_KEY_ENV, JSON_CONTENT_TYPE, MODELS_PATH, SYSTEM_ONE_PATH};
use crate::error::{Error, api_error, deserialize_body, format_endpoint};
use crate::json::IntoState;
use crate::logging::{RedactedHeaders, redact_text};
use crate::multipart::Part;
use crate::question::{Question, normalize_questions};
use crate::request::{Payload, PreparedRequest, prepare, set_retry_count};
use crate::retry::RetryPolicy;
use crate::wire::SystemOneRequest;

/// Asynchronous TypeSafe client.
///
/// Cloning is cheap. Clones share the connection pool and the images attached
/// with [`Client::attach_assets`].
#[derive(Clone)]
pub struct Client {
    config: Config,
    http: HttpClient,
    retry: RetryPolicy,
    attached: Attached,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("config", &self.config)
            .field("http", &self.http)
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
/// Builder state meaning no API key has been supplied yet.
pub struct NoApiKey;

#[derive(Clone, Debug)]
/// Builder state meaning an API key has been supplied.
pub struct ApiKeySet;

#[derive(Clone)]
/// Typestate builder for `Client`. Starts as `ClientBuilder<NoApiKey>`, `api_key()` moves it to `ClientBuilder<ApiKeySet>`, and `build()` exists only on the keyed state.
///
/// `Debug` shows the API key and secret header values as `***`.
pub struct ClientBuilder<S> {
    api_key: Option<String>,
    model: Option<String>,
    retry: Option<RetryPolicy>,
    timeout: Option<Duration>,
    headers: HeaderMap,
    base_url: Option<String>,
    http: Option<HttpClient>,
    compression: Option<Compression>,
    marker: PhantomData<S>,
}

impl<S> std::fmt::Debug for ClientBuilder<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientBuilder")
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("model", &self.model)
            .field("retry", &self.retry)
            .field("timeout", &self.timeout)
            .field("headers", &RedactedHeaders(&self.headers))
            .field("base_url", &self.base_url)
            .field("http", &self.http)
            .field("compression", &self.compression)
            .finish()
    }
}

impl ClientBuilder<NoApiKey> {
    /// Sets the API key and moves the builder to `ClientBuilder<ApiKeySet>`.
    pub fn api_key(self, api_key: impl Into<String>) -> ClientBuilder<ApiKeySet> {
        ClientBuilder {
            api_key: Some(api_key.into()),
            model: self.model,
            retry: self.retry,
            timeout: self.timeout,
            headers: self.headers,
            base_url: self.base_url,
            http: self.http,
            compression: self.compression,
            marker: PhantomData,
        }
    }
}

impl ClientBuilder<ApiKeySet> {
    /// Replaces the API key on an already keyed builder.
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Validates the builder and returns a `Client`. Reads defaults from the environment for unset fields.
    ///
    /// The API key is trimmed. An empty key, or one with characters other than
    /// printable ASCII without whitespace, is an error that does not contain
    /// the key.
    pub fn build(self) -> Result<Client, Error> {
        crate::logging::setup();
        if let Some(retry) = &self.retry {
            retry.validate()?;
        }
        // ApiKeySet is only constructed via api_key(), so the key is always present.
        let api_key = self
            .api_key
            .expect("ApiKeySet guarantees api_key is present");
        let config = Config::resolve(
            &api_key,
            self.base_url,
            self.model,
            self.timeout,
            self.headers,
            self.compression,
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
            attached: Attached::default(),
        })
    }
}

impl<S> ClientBuilder<S> {
    /// Sets the default model, overriding `TYPESAFE_DEFAULT_MODEL` and `jev-latest`.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Sets the retry policy, overriding `RetryPolicy::default()`.
    pub fn retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = Some(retry);
        self
    }

    /// Sets the per-attempt timeout, overriding the 10 second default.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Adds one default header sent with every request. Errors on an invalid name or value.
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Result<Self, Error> {
        let name = http::HeaderName::from_bytes(name.as_ref().as_bytes())
            .map_err(|_| Error::sdk("invalid header name"))?;
        let value = http::HeaderValue::from_str(value.as_ref())
            .map_err(|_| Error::sdk("invalid header value"))?;
        self.headers.insert(name, value);
        Ok(self)
    }

    /// Replaces the full set of default headers sent with every request.
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }

    /// Sets the base URL, overriding `TYPESAFE_BASE_URL` and `https://api.typesafe.ai`.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Sets how request bodies are compressed. Defaults to [`Compression::Zstd`]
    /// with the `zstd` feature, which is on by default, and to
    /// [`Compression::None`] without it. `Compression::Zstd` without the feature
    /// makes `build` fail.
    ///
    /// Bodies of 1024 bytes or more are compressed once per call, reused on
    /// every retry, and sent with `Content-Encoding: zstd`. `GET /v1/models`
    /// has no body and is never compressed. Use `Compression::None` for a
    /// server that does not accept compressed bodies.
    pub fn compression(mut self, compression: Compression) -> Self {
        self.compression = Some(compression);
        self
    }

    /// Uses the given `reqwest` 0.12 client instead of building one from the timeout.
    ///
    /// Pass a clone of a client you already have to share its connection pool,
    /// TLS roots, proxy, and redirect policy. Per-attempt timeouts still apply.
    pub fn http_client(mut self, http: HttpClient) -> Self {
        self.http = Some(http);
        self
    }
}

impl Default for ClientBuilder<NoApiKey> {
    fn default() -> Self {
        Self {
            api_key: None,
            model: None,
            retry: None,
            timeout: None,
            headers: HeaderMap::new(),
            base_url: None,
            http: None,
            compression: None,
            marker: PhantomData,
        }
    }
}

impl Client {
    /// Returns a `ClientBuilder<NoApiKey>`. Call `api_key()` before `build()`.
    pub fn builder() -> ClientBuilder<NoApiKey> {
        ClientBuilder::default()
    }

    /// Builds a `Client` from the environment. Errors when `TYPESAFE_API_KEY` is unset or blank.
    pub fn from_env() -> Result<Self, Error> {
        let api_key = crate::config::resolve_env(None, API_KEY_ENV, None).ok_or_else(|| {
            Error::sdk(format!(
                "No API key was provided. Pass api_key or set the {API_KEY_ENV} environment variable."
            ))
        })?;
        Self::builder().api_key(api_key).build()
    }

    /// Builds a `Client` with the given API key and default model, timeout, and retry policy.
    pub fn new(api_key: impl Into<String>) -> Result<Self, Error> {
        Self::builder().api_key(api_key).build()
    }

    /// Attaches images to this client until the returned guard is dropped.
    ///
    /// While attached, every call on this client, and on its clones, sends an
    /// attached image whose reference appears anywhere in the request, as if it
    /// were in [`SystemOneOpts::images`]. An attached image that no request
    /// references is not sent. Attaching an image that is already attached
    /// holds it again: it stays attached until the last guard holding it is
    /// dropped.
    ///
    /// Unlike the Python SDK, whose client is not cloneable, clones of a
    /// `Client` share their attachments.
    pub fn attach_assets(&self, images: impl IntoIterator<Item = Image>) -> AttachGuard {
        AttachGuard::new(&self.attached, images)
    }

    /// Sends `state` and `questions` to `POST /v1/systemone` with client defaults. Returns decoded answers.
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

    /// Sends `state` and `questions` to `POST /v1/systemone` with per-call `SystemOneOpts`. Returns decoded answers.
    ///
    /// A request that sends images goes as `multipart/form-data`: the JSON body
    /// first, as the part `request`, with an `assets` object naming each image's
    /// part, then each image's bytes. See [`SystemOneOpts::assets`].
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
        let mut body = SystemOneRequest {
            state: state.into_state()?,
            model: Some(
                opts.model
                    .unwrap_or_else(|| self.config.default_model.clone()),
            ),
            questions: normalize_questions(questions)?,
            extra: opts.extra_body.unwrap_or_default(),
        };
        let json = encode_json(&body)?;
        // An `assets` key in `extra_body` replaces the SDK's: the body goes as it is.
        let payload = if body.extra.contains_key("assets") {
            Payload::json(json)
        } else {
            let pending =
                pending_assets(&json, &opts.assets, &opts.images, snapshot(&self.attached));
            if pending.is_empty() {
                // The API rejects an empty `assets`, so it is left out.
                Payload::json(json)
            } else {
                let parts: Vec<String> = (0..pending.len()).map(|i| format!("asset-{i}")).collect();
                let entries = pending
                    .keys()
                    .zip(&parts)
                    .map(|(name, part)| {
                        (
                            name.clone(),
                            serde_json::json!({"type": "image", "part": part}),
                        )
                    })
                    .collect();
                body.extra
                    .insert("assets".to_string(), Value::Object(entries));
                let json = encode_json(&body)?;
                let images = resolve_assets(&pending).await?;
                // The API requires each part to be claimed by exactly one asset,
                // so an image under several names is sent once per name.
                let mut multipart = vec![Part {
                    name: "request",
                    content_type: JSON_CONTENT_TYPE,
                    content: &json,
                    filename: None,
                }];
                multipart.extend(images.iter().zip(&parts).map(|(image, part)| Part {
                    name: part,
                    content_type: image.content_type,
                    content: &image.data,
                    filename: Some(part),
                }));
                let (content_type, bytes) = crate::multipart::encode(&multipart);
                Payload {
                    content_type,
                    bytes,
                }
            }
        };
        let request = prepare(
            &self.config,
            "POST",
            SYSTEM_ONE_PATH,
            Some(payload),
            opts.timeout,
            &opts.extra_headers,
        )?;
        let retry = match &opts.retry {
            Some(policy) => {
                policy.validate()?;
                policy
            }
            None => &self.retry,
        };
        let (status, headers, endpoint, bytes) = self.send(request, retry).await?;
        decode_system_one(status, headers, Some(endpoint), bytes)
    }

    /// Lists models from `GET /v1/models` with client defaults. Returns decoded model entries.
    pub async fn models(&self) -> Result<ListModelsResponse, Error> {
        self.models_opts(ModelsOpts::default()).await
    }

    /// Lists models from `GET /v1/models` with per-call `ModelsOpts`. Returns decoded model entries.
    pub async fn models_opts(&self, opts: ModelsOpts) -> Result<ListModelsResponse, Error> {
        let request = prepare(
            &self.config,
            "GET",
            MODELS_PATH,
            None,
            opts.timeout,
            &opts.extra_headers,
        )?;
        let retry = match &opts.retry {
            Some(policy) => {
                policy.validate()?;
                policy
            }
            None => &self.retry,
        };
        let (status, headers, endpoint, bytes) = self.send(request, retry).await?;
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
                    log::info!(
                        target: "typesafe_sdk",
                        "{} {} retry {}",
                        request.method,
                        request.url,
                        attempt + 1
                    );
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
            .timeout(request.timeout);
        if log::log_enabled!(target: "typesafe_sdk", log::Level::Debug) {
            log::debug!(
                target: "typesafe_sdk",
                "{} {} -> headers={:?}",
                request.method,
                request.url,
                crate::logging::redact(&headers)
            );
        }
        let sent = headers.clone();
        builder = builder.headers(headers);
        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }
        let started = Instant::now();
        let response = builder
            .send()
            .await
            .map_err(map_reqwest_error(request.timeout, &sent))?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let endpoint = format_endpoint(&request.method, &request.url);
        let bytes = response
            .bytes()
            .await
            .map_err(map_reqwest_error(request.timeout, &sent))?;
        let request_id =
            crate::error::header_str(&headers, crate::constants::REQUEST_ID_HEADER).unwrap_or("-");
        log::info!(
            target: "typesafe_sdk",
            "{} {} <- {status} in {:.0}ms (request {request_id})",
            request.method,
            request.url,
            started.elapsed().as_secs_f64() * 1000.0
        );
        Ok((status, headers, endpoint, bytes))
    }
}

/// A timeout, or a connection error whose message has the request's
/// credentials masked.
fn map_reqwest_error(
    timeout: Duration,
    headers: &HeaderMap,
) -> impl FnOnce(reqwest::Error) -> Error + '_ {
    move |error| {
        if error.is_timeout() {
            Error::Timeout { timeout }
        } else {
            Error::Connection {
                message: redact_text(&error.without_url().to_string(), headers),
            }
        }
    }
}

fn encode_json(body: &SystemOneRequest) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(body)
        .map_err(|_| Error::sdk("The request body could not be encoded as JSON"))
}

fn fastrand_jitter() -> f64 {
    fastrand::f64()
}

#[derive(Clone, Default)]
/// Per-call overrides for `system_one_opts`.
///
/// `Debug` shows secret header values in `extra_headers` as `***`.
pub struct SystemOneOpts {
    /// Overrides the client default model. `None` keeps the client model.
    pub model: Option<String>,
    /// Overrides the client retry policy. `None` keeps the client policy.
    pub retry: Option<RetryPolicy>,
    /// Overrides the client per-attempt timeout. `None` keeps the client timeout.
    pub timeout: Option<Duration>,
    /// Extra headers merged over the client headers for one call.
    pub extra_headers: HeaderMap,
    /// Extra body keys shallow-merged over `state`, `model`, and `questions`. Last write wins.
    ///
    /// An `assets` key here replaces the one the client would write: the body
    /// is sent as plain JSON, and no image is read or sent.
    pub extra_body: Option<Map<String, Value>>,
    /// Images by alias, such as `<photo>`, sent with this call.
    ///
    /// Write the alias where the image belongs, as or inside any string in
    /// `state`, question instructions, or criteria. Every alias here is sent,
    /// even one the request does not use. An alias must not be empty or only
    /// whitespace.
    ///
    /// The request's `assets` object lists these aliases in order, then the
    /// reference of each known image the request contains, in order of first
    /// appearance. Known images are those in `images`, those here, and those
    /// attached with [`Client::attach_assets`]. Each entry names its part,
    /// `{"type":"image","part":"asset-N"}`, by position. An image under several
    /// names is sent once per name, and each distinct image is read once.
    ///
    /// Images are read before the first attempt and sent unchanged on every
    /// retry. An error names the alias, such as `Asset '<photo>': Image data is
    /// empty.`
    pub assets: IndexMap<String, Image>,
    /// Images this call puts in the request by reference, the text
    /// [`Image::reference`] returns.
    ///
    /// Each is sent only when its reference appears in the request. A
    /// `<image:…>` text that names no known image stays plain text.
    pub images: Vec<Image>,
}

impl std::fmt::Debug for SystemOneOpts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemOneOpts")
            .field("model", &self.model)
            .field("retry", &self.retry)
            .field("timeout", &self.timeout)
            .field("extra_headers", &RedactedHeaders(&self.extra_headers))
            .field("extra_body", &self.extra_body)
            .field("assets", &self.assets)
            .field("images", &self.images)
            .finish()
    }
}

#[derive(Clone, Default)]
/// Per-call overrides for `models_opts`.
///
/// `Debug` shows secret header values in `extra_headers` as `***`.
pub struct ModelsOpts {
    /// Overrides the client retry policy. `None` keeps the client policy.
    pub retry: Option<RetryPolicy>,
    /// Overrides the client per-attempt timeout. `None` keeps the client timeout.
    pub timeout: Option<Duration>,
    /// Extra headers merged over the client headers for one call.
    pub extra_headers: HeaderMap,
}

impl std::fmt::Debug for ModelsOpts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelsOpts")
            .field("retry", &self.retry)
            .field("timeout", &self.timeout)
            .field("extra_headers", &RedactedHeaders(&self.extra_headers))
            .finish()
    }
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
