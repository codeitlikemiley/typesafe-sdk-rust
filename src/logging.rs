use std::sync::OnceLock;

use http::HeaderMap;

use crate::constants::{LOG_LEVEL_ENV, SECRET_HEADERS};

static SETUP: OnceLock<()> = OnceLock::new();

pub(crate) fn setup() {
    SETUP.get_or_init(|| {
        let raw = std::env::var(LOG_LEVEL_ENV).unwrap_or_default();
        let filter = match raw.trim().to_ascii_lowercase().as_str() {
            "debug" => log::LevelFilter::Debug,
            "info" => log::LevelFilter::Info,
            "warn" | "warning" => log::LevelFilter::Warn,
            "error" => log::LevelFilter::Error,
            "off" => log::LevelFilter::Off,
            _ => return,
        };
        log::set_max_level(filter);
    });
}

pub(crate) fn redact(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            let key = name.as_str().to_string();
            let shown = if is_secret(&key) {
                "***".to_string()
            } else {
                value.to_str().unwrap_or("").to_string()
            };
            (key, shown)
        })
        .collect()
}

fn is_secret(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    SECRET_HEADERS.contains(&lowered.as_str())
        || lowered.contains("token")
        || lowered.contains("secret")
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;

    #[test]
    fn redacts_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer secret"));
        headers.insert("accept", HeaderValue::from_static("application/json"));
        let redacted = redact(&headers);
        assert!(
            redacted
                .iter()
                .any(|(name, value)| name == "authorization" && value == "***")
        );
        assert!(
            redacted
                .iter()
                .any(|(name, value)| name == "accept" && value == "application/json")
        );
    }
}
