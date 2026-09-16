use std::collections::HashSet;
use std::time::Duration;

use crate::error::Error;

/// Statuses that trigger a retry.
///
/// `Default` is 408, 429, and 500-599. That set is a predicate, not a stored table.
/// `Custom` is an exact set, including the empty set (retry no HTTP status).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetryStatuses {
    Default,
    Custom(HashSet<u16>),
}

impl RetryStatuses {
    pub fn contains(&self, status: u16) -> bool {
        match self {
            Self::Default => matches!(status, 408 | 429 | 500..=599),
            Self::Custom(set) => set.contains(&status),
        }
    }
}

/// Retry behavior for one client or one call. Numbers match the Python SDK defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
    pub backoff_jitter: f64,
    pub http_statuses: RetryStatuses,
    pub respect_retry_after: bool,
    pub api_connection_error: bool,
    pub api_timeout_error: bool,
    pub timeout: Option<Duration>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            backoff_initial: Duration::from_millis(500),
            backoff_max: Duration::from_secs(5),
            backoff_jitter: 0.25,
            http_statuses: RetryStatuses::Default,
            respect_retry_after: true,
            api_connection_error: true,
            api_timeout_error: true,
            timeout: Some(Duration::from_secs(30)),
        }
    }
}

impl RetryPolicy {
    pub fn disabled() -> Self {
        Self {
            max_retries: 0,
            timeout: None,
            ..Self::default()
        }
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.backoff_jitter.is_nan() || !(0.0..=1.0).contains(&self.backoff_jitter) {
            return Err(Error::sdk("backoff_jitter must be between zero and one."));
        }
        if let Some(timeout) = self.timeout {
            crate::config::resolve_timeout(timeout)?;
        }
        Ok(())
    }

    pub(crate) fn retryable(&self, error: &Error) -> bool {
        match error {
            Error::Timeout { .. } => self.api_timeout_error,
            Error::Connection { .. } => self.api_connection_error,
            Error::Api(api) => self.http_statuses.contains(api.status),
            Error::Sdk(_) => false,
        }
    }

    pub(crate) fn wait(&self, attempt_number: u32, error: &Error, jitter: f64) -> Duration {
        if self.respect_retry_after {
            if let Error::Api(api) = error {
                if let Some(delay) = api.retry_after {
                    return delay;
                }
            }
        }
        backoff(
            attempt_number,
            self.backoff_initial.as_secs_f64(),
            self.backoff_max.as_secs_f64(),
            self.backoff_jitter,
            jitter,
        )
    }
}

/// `attempt` is 1-based, matching Python Tenacity's `attempt_number` on wait.
pub(crate) fn backoff(
    attempt: u32,
    initial: f64,
    maximum: f64,
    jitter: f64,
    random: f64,
) -> Duration {
    if initial == 0.0 || maximum == 0.0 {
        return Duration::ZERO;
    }
    let exponent = f64::from(attempt.saturating_sub(1));
    let cap = maximum.log2() - initial.log2();
    let exponential = if exponent >= cap {
        maximum
    } else {
        initial * 2f64.powf(exponent)
    };
    let delay = exponential * (1.0 - random * jitter);
    let delay = exponential.min((delay * 1000.0).round() / 1000.0);
    Duration::from_secs_f64(delay.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_matches_python_table() {
        let expected = [(1, 0.5), (2, 1.0), (3, 2.0), (4, 4.0), (5, 5.0), (20, 5.0)];
        for (attempt, seconds) in expected {
            assert_eq!(
                backoff(attempt, 0.5, 5.0, 0.25, 0.0),
                Duration::from_secs_f64(seconds)
            );
        }
        assert_eq!(
            backoff(1, 0.5, 5.0, 0.25, 1.0),
            Duration::from_secs_f64(0.375)
        );
    }

    #[test]
    fn zero_backoff_disables_delay() {
        assert_eq!(backoff(3, 0.0, 5.0, 0.25, 0.0), Duration::ZERO);
        assert_eq!(backoff(3, 0.5, 0.0, 0.25, 0.0), Duration::ZERO);
    }

    #[test]
    fn default_statuses_match_python_set() {
        let statuses = RetryStatuses::Default;
        for status in [408, 429, 500, 503, 599] {
            assert!(statuses.contains(status), "{status}");
        }
        for status in [400, 404, 409, 499] {
            assert!(!statuses.contains(status), "{status}");
        }
    }

    #[test]
    fn custom_statuses_are_exact() {
        let statuses = RetryStatuses::Custom(HashSet::from([409]));
        assert!(statuses.contains(409));
        assert!(!statuses.contains(429));
        assert!(!RetryStatuses::Custom(HashSet::new()).contains(503));
    }
}
