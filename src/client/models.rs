use std::collections::HashMap;

use crate::constants::MODELS_PATH;
use crate::retry::RetryPolicy;
use crate::transport::prepare;
use crate::types::ListModelsResponse;

/// Models API for the blocking client.
pub struct Models<'a> {
    inner: &'a dyn ModelsSender,
}

pub(crate) trait ModelsSender {
    fn config(&self) -> &crate::config::Config;
    fn send_models(
        &self,
        req: crate::transport::PreparedRequest,
        retry: Option<&RetryPolicy>,
    ) -> Result<ListModelsResponse, Box<dyn std::error::Error + Send + Sync>>;
}

impl<'a> Models<'a> {
    pub(crate) fn new(inner: &'a dyn ModelsSender) -> Self {
        Self { inner }
    }

    pub fn list(
        &self,
        retry: Option<&RetryPolicy>,
        timeout_secs: Option<f64>,
        extra_headers: Option<&HashMap<String, String>>,
    ) -> Result<ListModelsResponse, Box<dyn std::error::Error + Send + Sync>> {
        let req = prepare(
            self.inner.config(),
            "GET",
            MODELS_PATH,
            None,
            timeout_secs,
            extra_headers,
        )?;
        self.inner.send_models(req, retry)
    }
}

/// Models API for the async client.
pub struct AsyncModels<'a> {
    inner: &'a dyn AsyncModelsSender,
}

pub(crate) trait AsyncModelsSender {
    fn config(&self) -> &crate::config::Config;
    fn send_models<'a>(
        &'a self,
        req: crate::transport::PreparedRequest,
        retry: Option<&'a RetryPolicy>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ListModelsResponse, Box<dyn std::error::Error + Send + Sync>>> + Send + 'a>,
    >;
}

impl<'a> AsyncModels<'a> {
    pub(crate) fn new(inner: &'a dyn AsyncModelsSender) -> Self {
        Self { inner }
    }

    pub async fn list(
        &self,
        retry: Option<&RetryPolicy>,
        timeout_secs: Option<f64>,
        extra_headers: Option<&HashMap<String, String>>,
    ) -> Result<ListModelsResponse, Box<dyn std::error::Error + Send + Sync>> {
        let req = prepare(
            self.inner.config(),
            "GET",
            MODELS_PATH,
            None,
            timeout_secs,
            extra_headers,
        )?;
        self.inner.send_models(req, retry).await
    }
}
