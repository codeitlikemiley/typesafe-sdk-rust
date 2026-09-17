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
    pub fn builder() -> Builder<NoApiKey> {
        Builder {
            inner: crate::Client::builder(),
        }
    }

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

    pub fn new(api_key: impl Into<String>) -> Result<Self, Error> {
        Self::builder().api_key(api_key).build()
    }

    pub fn system_one<S, I, K>(&self, state: S, questions: I) -> Result<SystemOneResponse, Error>
    where
        S: IntoState,
        I: IntoIterator<Item = (K, Question)>,
        K: Into<String>,
    {
        self.runtime
            .block_on(self.inner.system_one(state, questions))
    }

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

    pub fn models(&self) -> Result<ListModelsResponse, Error> {
        self.runtime.block_on(self.inner.models())
    }

    pub fn models_opts(&self, opts: ModelsOpts) -> Result<ListModelsResponse, Error> {
        self.runtime.block_on(self.inner.models_opts(opts))
    }
}

pub struct Builder<S> {
    inner: ClientBuilder<S>,
}

impl Builder<NoApiKey> {
    pub fn api_key(self, api_key: impl Into<String>) -> Builder<ApiKeySet> {
        Builder {
            inner: self.inner.api_key(api_key),
        }
    }
}

impl Builder<ApiKeySet> {
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.inner = self.inner.api_key(api_key);
        self
    }

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
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.inner = self.inner.model(model);
        self
    }

    pub fn retry(mut self, retry: crate::RetryPolicy) -> Self {
        self.inner = self.inner.retry(retry);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Result<Self, Error> {
        self.inner = self.inner.header(name, value)?;
        Ok(self)
    }

    pub fn headers(mut self, headers: http::HeaderMap) -> Self {
        self.inner = self.inner.headers(headers);
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.inner = self.inner.base_url(base_url);
        self
    }
}
