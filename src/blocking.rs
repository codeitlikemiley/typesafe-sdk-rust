use std::time::Duration;

use crate::answer::{ListModelsResponse, SystemOneResponse};
use crate::client::{ApiKeySet, ClientBuilder, ModelsOpts, NoApiKey, SystemOneOpts};
use crate::error::Error;
use crate::json::IntoState;
use crate::question::Question;

/// Blocking client. Owns a current-thread Tokio runtime around the async client.
pub struct Client {
    inner: crate::Client,
    runtime: tokio::runtime::Runtime,
}

impl Client {
    /// Returns a `Builder<NoApiKey>`. Call `api_key()` before `build()`.
    pub fn builder() -> Builder<NoApiKey> {
        Builder {
            inner: crate::Client::builder(),
        }
    }

    /// Builds a blocking `Client` from the environment. Errors when `TYPESAFE_API_KEY` is unset or blank.
    pub fn from_env() -> Result<Self, Error> {
        let inner = crate::Client::from_env()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| Error::Connection {
                message: error.to_string(),
            })?;
        Ok(Self { inner, runtime })
    }

    /// Builds a blocking `Client` with the given API key and default model, timeout, and retry policy.
    pub fn new(api_key: impl Into<String>) -> Result<Self, Error> {
        Self::builder().api_key(api_key).build()
    }

    /// Sends `state` and `questions` to `POST /v1/systemone` with client defaults. Returns decoded answers.
    pub fn system_one<S, I, K>(&self, state: S, questions: I) -> Result<SystemOneResponse, Error>
    where
        S: IntoState,
        I: IntoIterator<Item = (K, Question)>,
        K: Into<String>,
    {
        self.runtime
            .block_on(self.inner.system_one(state, questions))
    }

    /// Sends `state` and `questions` to `POST /v1/systemone` with per-call `SystemOneOpts`. Returns decoded answers.
    pub fn system_one_opts<S, I, K>(
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
        self.runtime
            .block_on(self.inner.system_one_opts(state, questions, opts))
    }

    /// Lists models from `GET /v1/models` with client defaults. Returns decoded model entries.
    pub fn models(&self) -> Result<ListModelsResponse, Error> {
        self.runtime.block_on(self.inner.models())
    }

    /// Lists models from `GET /v1/models` with per-call `ModelsOpts`. Returns decoded model entries.
    pub fn models_opts(&self, opts: ModelsOpts) -> Result<ListModelsResponse, Error> {
        self.runtime.block_on(self.inner.models_opts(opts))
    }
}

/// Typestate builder for the blocking `Client`. Starts as `Builder<NoApiKey>`, `api_key()` moves it to `Builder<ApiKeySet>`, and `build()` exists only on the keyed state.
pub struct Builder<S> {
    inner: ClientBuilder<S>,
}

impl Builder<NoApiKey> {
    /// Sets the API key and moves the builder to `Builder<ApiKeySet>`.
    pub fn api_key(self, api_key: impl Into<String>) -> Builder<ApiKeySet> {
        Builder {
            inner: self.inner.api_key(api_key),
        }
    }
}

impl Builder<ApiKeySet> {
    /// Replaces the API key on an already keyed builder.
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.inner = self.inner.api_key(api_key);
        self
    }

    /// Builds the runtime and returns a blocking `Client`. Reads defaults from the environment for unset fields.
    pub fn build(self) -> Result<Client, Error> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| Error::Connection {
                message: error.to_string(),
            })?;
        Ok(Client {
            inner: self.inner.build()?,
            runtime,
        })
    }
}

impl<S> Builder<S> {
    /// Sets the default model, overriding `TYPESAFE_DEFAULT_MODEL` and `jev-latest`.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.inner = self.inner.model(model);
        self
    }

    /// Sets the retry policy, overriding `RetryPolicy::default()`.
    pub fn retry(mut self, retry: crate::RetryPolicy) -> Self {
        self.inner = self.inner.retry(retry);
        self
    }

    /// Sets the per-attempt timeout, overriding the 10 second default.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    /// Adds one default header sent with every request. Errors on an invalid name or value.
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Result<Self, Error> {
        self.inner = self.inner.header(name, value)?;
        Ok(self)
    }

    /// Replaces the full set of default headers sent with every request.
    pub fn headers(mut self, headers: http::HeaderMap) -> Self {
        self.inner = self.inner.headers(headers);
        self
    }

    /// Sets the base URL, overriding `TYPESAFE_BASE_URL` and `https://api.typesafe.ai`.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.inner = self.inner.base_url(base_url);
        self
    }
}
