//! Cloudflare Workers AI Clef: the System One body, posted to
//! `{base}/accounts/{account_id}/ai/run/@cf/cloudflare/{model}`, answered in
//! the Workers AI envelope `{"success", "errors", "result"}`.

use serde_json::Value;

use crate::answer::{SystemOneResponse, decode_fields, finish_response};
use crate::error::{Error, ErrorBody, api_error_with_message, deserialize_body};
use crate::question::Question;
use crate::wire::SystemOneRequest;

const PROVIDER: &str = "Clef";
/// Workers AI accepts 1 to 64 questions per request.
const MAX_QUESTIONS: usize = 64;
const MAX_NAME_LENGTH: usize = 100;

pub(crate) fn validate_account_id(account_id: &str) -> Result<String, Error> {
    let account_id = account_id.trim();
    if account_id.is_empty() {
        return Err(Error::sdk(
            "The Clef provider needs a Cloudflare account ID. Pass Provider::Clef { account_id } or set CLOUDFLARE_ACCOUNT_ID.",
        ));
    }
    if !account_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(Error::sdk(
            "The Cloudflare account ID must contain only ASCII letters, digits, '-', and '_'.",
        ));
    }
    Ok(account_id.to_string())
}

/// The path appended to the base URL for `model`.
pub(crate) fn path(account_id: &str, model: &str) -> String {
    format!("/accounts/{account_id}/ai/run/@cf/cloudflare/{model}")
}

/// Refuses what Workers AI cannot take, before any request.
pub(crate) fn check(request: &SystemOneRequest) -> Result<(), Error> {
    let count = request.questions.len();
    if count > MAX_QUESTIONS {
        return Err(Error::unsupported(
            PROVIDER,
            format!("more than {MAX_QUESTIONS} questions in one request (got {count})"),
        ));
    }
    for (name, question) in &request.questions {
        if !valid_name(name) {
            return Err(Error::unsupported(
                PROVIDER,
                format!(
                    "the question name \"{name}\": names must be 1 to {MAX_NAME_LENGTH} characters from [A-Za-z0-9_.-]"
                ),
            ));
        }
        let (kind, labels) = match question {
            Question::Box { .. } => ("box", None),
            Question::Choice { criteria, .. } => ("choice", Some(criteria.len())),
            Question::Raw(map) => (
                map.get("type").and_then(Value::as_str).unwrap_or_default(),
                map.get("criteria")
                    .and_then(Value::as_object)
                    .map(|c| c.len()),
            ),
            _ => ("", None),
        };
        match (kind, labels) {
            ("box", _) => {
                return Err(Error::unsupported(
                    PROVIDER,
                    format!("box questions (question \"{name}\")"),
                ));
            }
            ("choice", Some(1)) => {
                return Err(Error::unsupported(
                    PROVIDER,
                    format!(
                        "a choice with one label (question \"{name}\"): Workers AI needs at least two"
                    ),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn valid_name(name: &str) -> bool {
    (1..=MAX_NAME_LENGTH).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

/// Unwraps the envelope. `success: false` or any entry in `errors` is an
/// `Error::Api` carrying Cloudflare's messages. `result` decodes as a System
/// One response, with error paths under `result.`. Workers AI does not
/// document `result.model`, so a `result` without it reports `model`, the
/// model the request named.
pub(crate) fn decode(
    status: u16,
    headers: http::HeaderMap,
    endpoint: Option<String>,
    body: bytes::Bytes,
    model: &str,
) -> Result<SystemOneResponse, Error> {
    let parsed = serde_json::from_slice::<Value>(&body).ok();
    let envelope = parsed.as_ref().and_then(Value::as_object);
    if let Some(envelope) = envelope {
        let failed = envelope.get("success") == Some(&Value::Bool(false));
        let errors = envelope
            .get("errors")
            .and_then(Value::as_array)
            .is_some_and(|errors| !errors.is_empty());
        if failed || errors {
            let message = parsed
                .as_ref()
                .and_then(cloudflare_message)
                .unwrap_or_else(|| "Cloudflare reported success: false with no errors.".into());
            return Err(api_error_with_message(
                status,
                deserialize_body(&body),
                headers,
                endpoint,
                message,
            ));
        }
    }
    let fields = match envelope.and_then(|envelope| envelope.get("result")) {
        Some(Value::Object(result)) if !result.contains_key("model") => {
            let mut result = result.clone();
            result.insert("model".to_string(), Value::String(model.to_string()));
            decode_fields(&Value::Object(result)).map_err(|path| format!("result.{path}"))
        }
        Some(result) => decode_fields(result).map_err(|path| format!("result.{path}")),
        None => Err("result".to_string()),
    };
    finish_response(status, headers, endpoint, body, fields)
}

/// Gives an HTTP error with a Cloudflare envelope the envelope's messages.
pub(crate) fn with_cloudflare_message(mut error: Error) -> Error {
    if let Error::Api(api) = &mut error {
        if let Some(message) = match &api.body {
            Some(ErrorBody::Json(body)) => cloudflare_message(body),
            _ => None,
        } {
            api.set_message(message);
        }
    }
    error
}

/// `errors[].message`, each with its code, joined with `; `.
fn cloudflare_message(body: &Value) -> Option<String> {
    let parts: Vec<String> = body
        .get("errors")?
        .as_array()?
        .iter()
        .filter_map(|entry| {
            let message = entry.get("message")?.as_str()?;
            Some(match entry.get("code") {
                Some(Value::Number(code)) => format!("{message} (code {code})"),
                _ => message.to_string(),
            })
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("; "))
}
