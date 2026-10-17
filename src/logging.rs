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

/// Debug view of a header map with secret header values shown as `***`.
pub(crate) struct RedactedHeaders<'a>(pub &'a HeaderMap);

impl std::fmt::Debug for RedactedHeaders<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(redact(self.0)).finish()
    }
}

/// Masks every credential in `headers` that occurs in `message` with `***`: the
/// values of secret headers, the token after the scheme of `Authorization` and
/// `Proxy-Authorization`, and their Python-repr and JSON-escaped forms. The
/// longest match wins at each position.
pub(crate) fn redact_text(message: &str, headers: &HeaderMap) -> String {
    let mut credentials: Vec<String> = Vec::new();
    for (name, value) in headers {
        let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
        if matches!(name.as_str(), "authorization" | "proxy-authorization") {
            let trimmed = value.trim_start();
            if let Some(at) = trimmed.find(char::is_whitespace) {
                let credential = trimmed[at..].trim_start();
                if !credential.is_empty() {
                    credentials.push(credential.to_string());
                }
            }
        }
        if is_secret(name.as_str()) && !value.is_empty() {
            credentials.push(value);
        }
    }
    let mut variants: Vec<String> = Vec::new();
    for credential in &credentials {
        let json = serde_json::to_string(credential).unwrap_or_default();
        let json = json[1..json.len() - 1].to_string();
        let repr = crate::asset::py_repr(credential);
        let repr = repr[1..repr.len() - 1].to_string();
        let bytes_repr = py_bytes_repr_inner(credential);
        variants.extend([credential.clone(), repr, bytes_repr, json]);
    }
    variants.retain(|variant| !variant.is_empty());
    variants.sort_by_key(|variant| std::cmp::Reverse(variant.len()));
    variants.dedup();
    if variants.is_empty() {
        return message.to_string();
    }
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    'scan: while let Some(c) = rest.chars().next() {
        for variant in &variants {
            if let Some(after) = rest.strip_prefix(variant.as_str()) {
                out.push_str("***");
                rest = after;
                continue 'scan;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Python's `repr(value.encode())[2:-1]`: the bytes literal without `b'` and `'`.
fn py_bytes_repr_inner(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        b'"'
    } else {
        b'\''
    };
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            byte if byte == quote => {
                out.push('\\');
                out.push(byte as char);
            }
            0x20..=0x7e => out.push(byte as char),
            byte => out.push_str(&format!("\\x{byte:02x}")),
        }
    }
    out
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

    /// reqwest does not put header values in its messages, but a custom client
    /// or proxy error may. The Python SDK masks the same forms.
    #[test]
    fn connection_messages_mask_credentials() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer sk-it's\\\"x"),
        );
        headers.insert("x-api-key", HeaderValue::from_static("key-1"));
        headers.insert("x-session-token", HeaderValue::from_static("tok-9"));
        headers.insert("accept", HeaderValue::from_static("application/json"));
        let cases = [
            ("sent Bearer sk-it's\\\"x!", "sent ***!"),
            ("token sk-it's\\\"x only", "token *** only"),
            // Python `repr` of the str and of the bytes, and JSON.
            ("repr sk-it\\'s\\\\\"x", "repr ***"),
            ("json sk-it's\\\\\\\"x", "json ***"),
            ("key-1 and tok-9", "*** and ***"),
            ("application/json stays", "application/json stays"),
        ];
        for (message, redacted) in cases {
            assert_eq!(redact_text(message, &headers), redacted, "{message}");
        }
    }
}
