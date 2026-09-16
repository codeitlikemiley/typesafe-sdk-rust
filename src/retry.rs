//! Retry policy configuration and execution helpers.

use std::thread;
use std::time::{Duration, Instant};

use rand::Rng;

use crate::config::resolve_timeout;
use crate::error::{
    parse_retry_after, ApiFailure, TypeSafeApiConnectionError,
    TypeSafeApiTimeoutError, TypeSafeError,
};

/// Configuration for SDK retry behavior.
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff_initial: f64,
    pub backoff_max: f64,
    pub backoff_jitter: f64,
    pub http_statuses: Vec<u16>,
    pub respect_retry_after: bool,
    pub api_connection_error: bool,
    pub api_timeout_error: bool,
    pub timeout_budget_secs: Option<f64>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            backoff_initial: 0.5,
            backoff_max: 5.0,
            backoff_jitter: 0.25,
            http_statuses: {
                let mut s = vec![408, 429];
                s.extend(500..600);
                s
            },
            respect_retry_after: true,
            api_connection_error: true,
            api_timeout_error: true,
            timeout_budget_secs: Some(30.0),
        }
    }
}

impl RetryPolicy {
    pub fn validate(&self) -> Result<(), TypeSafeError> {
        for (name, value) in [
            ("backoff_initial", self.backoff_initial),
            ("backoff_max", self.backoff_max),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(TypeSafeError::new(format!(
                    "{} must be a non-negative, finite number of seconds.",
                    name
                )));
            }
        }
        if self.backoff_jitter < 0.0 || self.backoff_jitter > 1.0 || !self.backoff_jitter.is_finite() {
            return Err(TypeSafeError::new("backoff_jitter must be between zero and one."));
        }
        if let Some(t) = self.timeout_budget_secs {
            resolve_timeout(t)?;
        }
        Ok(())
    }

    fn retryable(&self, err: &ApiFailure) -> bool {
        match err {
            ApiFailure::Timeout(_) => self.api_timeout_error,
            ApiFailure::Connection(_) => self.api_connection_error,
            ApiFailure::Api(e) => self.http_statuses.contains(&e.status),
            ApiFailure::Validation(_) | ApiFailure::Sdk(_) => false,
        }
    }

    fn wait_secs(&self, attempt: u32, err: &ApiFailure) -> f64 {
        if self.respect_retry_after {
            if let ApiFailure::Api(e) = err {
                if let Some(ms) = parse_retry_after(&e.headers) {
                    return ms / 1000.0;
                }
            }
        }
        backoff(attempt, self.backoff_initial, self.backoff_max, self.backoff_jitter)
    }
}

fn backoff(attempt: u32, initial: f64, maximum: f64, jitter: f64) -> f64 {
    if initial == 0.0 || maximum == 0.0 {
        return 0.0;
    }
    let exponent = attempt.saturating_sub(1) as i32;
    let max_exp = (maximum / initial).log2();
    let exponential = if exponent as f64 >= max_exp {
        maximum
    } else {
        initial * 2f64.powi(exponent)
    };
    let mut rng = rand::thread_rng();
    let delay = exponential * (1.0 - rng.gen::<f64>() * jitter);
    exponential.min((delay * 1000.0).round() / 1000.0)
}

pub fn with_retry<T, F>(policy: &RetryPolicy, mut f: F) -> Result<T, ApiFailure>
where
    F: FnMut(u32) -> Result<T, ApiFailure>,
{
    policy.validate()?;
    let budget_start = Instant::now();
    let max_attempts = policy.max_retries + 1;
    let mut last_err: Option<ApiFailure> = None;

    for attempt in 0..max_attempts {
        match f(attempt) {
            Ok(v) => return Ok(v),
            Err(err) => {
                if !policy.retryable(&err) || attempt + 1 >= max_attempts {
                    return Err(err);
                }
                let wait = policy.wait_secs(attempt + 1, &err);
                if let Some(budget) = policy.timeout_budget_secs {
                    let elapsed = budget_start.elapsed().as_secs_f64();
                    if elapsed + wait >= budget {
                        return Err(err);
                    }
                }
                last_err = Some(err);
                if wait > 0.0 {
                    thread::sleep(Duration::from_secs_f64(wait));
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        ApiFailure::Sdk(TypeSafeError::new("retry loop ended without result"))
    }))
}

pub async fn with_retry_async<T, F, Fut>(policy: &RetryPolicy, mut f: F) -> Result<T, ApiFailure>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Result<T, ApiFailure>>,
{
    policy.validate()?;
    let budget_start = Instant::now();
    let max_attempts = policy.max_retries + 1;
    let mut last_err: Option<ApiFailure> = None;

    for attempt in 0..max_attempts {
        match f(attempt).await {
            Ok(v) => return Ok(v),
            Err(err) => {
                if !policy.retryable(&err) || attempt + 1 >= max_attempts {
                    return Err(err);
                }
                let wait = policy.wait_secs(attempt + 1, &err);
                if let Some(budget) = policy.timeout_budget_secs {
                    let elapsed = budget_start.elapsed().as_secs_f64();
                    if elapsed + wait >= budget {
                        return Err(err);
                    }
                }
                last_err = Some(err);
                if wait > 0.0 {
                    tokio::time::sleep(Duration::from_secs_f64(wait)).await;
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        ApiFailure::Sdk(TypeSafeError::new("retry loop ended without result"))
    }))
}

pub fn connection_error(msg: impl Into<String>) -> ApiFailure {
    ApiFailure::Connection(TypeSafeApiConnectionError(msg.into()))
}

pub fn timeout_error(timeout_secs: f64) -> ApiFailure {
    ApiFailure::Timeout(TypeSafeApiTimeoutError { timeout_secs })
}
