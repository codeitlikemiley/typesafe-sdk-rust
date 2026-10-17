use std::time::Duration;

use serde_json::{Value, json};
use typesafe_sdk::{
    ApiErrorKind, Client, Compression, Image, JsonContent, NoulCriteria, Question, RetryPolicy,
    SystemOneOpts,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request_header(request: &wiremock::Request, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn result_body() -> Value {
    json!({
        "model": "jev-latest",
        "usage": {"input_tokens": 12, "output_tokens": 3},
        "answers": {
            "spam": {"type": "noul", "noul": 0.98},
            "tone": {
                "type": "choice",
                "choice": "friendly",
                "confidence": 0.9,
                "probabilities": {"friendly": 0.9, "hostile": 0.1}
            },
            "quality": {
                "type": "score",
                "score": 1.7,
                "confidence": 0.8,
                "legend": {"0": "bad", "1": "ok", "2": "great"},
                "probabilities": {"0": 0.1, "1": 0.1, "2": 0.8}
            }
        }
    })
}

async fn client(server: &MockServer) -> Client {
    Client::builder()
        .api_key("test-key")
        .base_url(server.uri())
        .retry(RetryPolicy::disabled())
        .build()
        .unwrap()
}

#[tokio::test]
async fn system_one_round_trip() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer test-key"))
        .and(header("accept", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(result_body()))
        .mount(&server)
        .await;

    let client = client(&server).await;
    let response = client
        .system_one(
            json!({"document": "Hello 🌍"}),
            [
                ("spam", Question::noul("Spam?")),
                (
                    "tone",
                    Question::choice("Tone?", [("friendly", None), ("hostile", None)]),
                ),
                (
                    "quality",
                    Question::score("Quality?", ["bad", "ok", "great"]),
                ),
            ],
        )
        .await
        .unwrap();

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let body: Value = serde_json::from_slice(&received[0].body).unwrap();
    assert_eq!(
        body,
        json!({
            "state": {"document": "Hello 🌍"},
            "model": "jev-latest",
            "questions": {
                "spam": {"type": "noul", "instructions": "Spam?"},
                "tone": {"type": "choice", "instructions": "Tone?", "criteria": {"friendly": null, "hostile": null}},
                "quality": {"type": "score", "instructions": "Quality?", "criteria": ["bad", "ok", "great"]}
            }
        })
    );

    assert_eq!(response.model, "jev-latest");
    assert_eq!(response.usage.input_tokens, Some(12));
    assert_eq!(response.usage.output_tokens, Some(3));
    assert_eq!(response.noul("spam").unwrap().noul, 0.98);
    assert_eq!(response.choice("tone").unwrap().choice, "friendly");
    assert_eq!(response.choice("tone").unwrap().confidence, 0.9);
    assert_eq!(response.score("quality").unwrap().score, 1.7);
    assert_eq!(
        response.score("quality").unwrap().legend.get(&0).unwrap(),
        &JsonContent::from("bad")
    );
    assert_eq!(response.score("quality").unwrap().probabilities[&2], 0.8);
    assert!(request_header(&received[0], "x-typesafe-retry-count").is_none());
    assert!(
        request_header(&received[0], "user-agent")
            .unwrap()
            .starts_with("typesafe-sdk/")
    );
    assert!(
        request_header(&received[0], "x-typesafe-runtime")
            .unwrap()
            .starts_with("rust/")
    );
}

#[tokio::test]
async fn extra_body_overrides_model() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(result_body()))
        .mount(&server)
        .await;

    let client = client(&server).await;
    let mut extra = serde_json::Map::new();
    extra.insert("model".to_string(), json!("override-model"));
    extra.insert("beam_width".to_string(), json!(4));
    extra.insert("nullable".to_string(), Value::Null);
    client
        .system_one_opts(
            "hi",
            [("q", Question::noul("?"))],
            SystemOneOpts {
                model: Some("call-model".to_string()),
                extra_body: Some(extra),
                ..SystemOneOpts::default()
            },
        )
        .await
        .unwrap();

    let body: Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(
        body,
        json!({
            "state": "hi",
            "model": "override-model",
            "questions": {"q": {"type": "noul", "instructions": "?"}},
            "beam_width": 4,
            "nullable": null
        })
    );
}

/// Exact request bytes. Recorded from 0.1.2, whose client built the body as a
/// `serde_json::Map` and merged `extra_body` into it. The `SystemOneRequest`
/// client must send the same bytes, not only equal JSON.
#[tokio::test]
async fn request_body_bytes_are_stable() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(result_body()))
        .mount(&server)
        .await;
    let client = client(&server).await;
    let raw = |value: Value| Question::raw(value.as_object().unwrap().clone());

    client
        .system_one(
            json!({"document": "Hello 🌍", "score": 1.5, "tags": ["a", "b"]}),
            [
                ("spam", Question::noul("Spam?")),
                (
                    "bare",
                    Question::noul_bare().with_noul_criteria(NoulCriteria::new().no("ham")),
                ),
                (
                    "duplicate",
                    Question::noul(
                        JsonContent::from_value(json!({"question": "Duplicate?"})).unwrap(),
                    )
                    .with_noul_criteria(
                        NoulCriteria::new()
                            .yes("charged twice")
                            .no(JsonContent::from_value(json!(["one charge"])).unwrap()),
                    ),
                ),
                (
                    "tone",
                    Question::choice(
                        "Tone?",
                        [
                            ("friendly", None),
                            ("hostile", Some("Rude or \"angry\"".into())),
                        ],
                    ),
                ),
                (
                    "quality",
                    Question::score(
                        "Quality?",
                        [
                            JsonContent::from("bad"),
                            JsonContent::from_value(json!({"label": "great", "min": 2})).unwrap(),
                        ],
                    ),
                ),
            ],
        )
        .await
        .unwrap();

    let mut extra = serde_json::Map::new();
    extra.insert("beam_width".to_string(), json!(4));
    extra.insert("model".to_string(), json!("override-model"));
    extra.insert("state".to_string(), json!({"replaced": true}));
    extra.insert("nullable".to_string(), Value::Null);
    extra.insert("options".to_string(), json!({"z": 1, "a": [0.25, "x"]}));
    client
        .system_one_opts(
            "hi",
            [
                ("q", Question::noul("?")),
                (
                    "ranked",
                    raw(json!({"type": "ranking", "instructions": "Order", "items": ["b", "a"]})),
                ),
                (
                    "weighted",
                    raw(json!({"type": "noul", "instructions": "Spam?", "weight": 3})),
                ),
            ],
            SystemOneOpts {
                model: Some("call-model".to_string()),
                extra_body: Some(extra),
                ..SystemOneOpts::default()
            },
        )
        .await
        .unwrap();

    let mut extra = serde_json::Map::new();
    extra.insert(
        "questions".to_string(),
        json!({"override": {"type": "noul"}}),
    );
    client
        .system_one_opts(
            json!(["a", 1]),
            [("q", Question::noul("?"))],
            SystemOneOpts {
                extra_body: Some(extra),
                ..SystemOneOpts::default()
            },
        )
        .await
        .unwrap();

    let bodies: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .map(|request| String::from_utf8(request.body).unwrap())
        .collect();
    assert_eq!(
        bodies,
        [
            concat!(
                r#"{"state":{"document":"Hello 🌍","score":1.5,"tags":["a","b"]},"model":"jev-latest","#,
                r#""questions":{"spam":{"type":"noul","instructions":"Spam?"},"bare":{"type":"noul","criteria":{"false":"ham"}},"#,
                r#""duplicate":{"type":"noul","instructions":{"question":"Duplicate?"},"#,
                r#""criteria":{"true":"charged twice","false":["one charge"]}},"#,
                r#""tone":{"type":"choice","instructions":"Tone?","#,
                r#""criteria":{"friendly":null,"hostile":"Rude or \"angry\""}},"#,
                r#""quality":{"type":"score","instructions":"Quality?","#,
                r#""criteria":["bad",{"label":"great","min":2}]}}}"#,
            ),
            concat!(
                r#"{"state":{"replaced":true},"model":"override-model","#,
                r#""questions":{"q":{"type":"noul","instructions":"?"},"#,
                r#""ranked":{"type":"ranking","instructions":"Order","items":["b","a"]},"#,
                r#""weighted":{"type":"noul","instructions":"Spam?","weight":3}},"#,
                r#""beam_width":4,"nullable":null,"options":{"z":1,"a":[0.25,"x"]}}"#,
            ),
            r#"{"state":["a",1],"model":"jev-latest","questions":{"override":{"type":"noul"}}}"#,
        ]
    );
}

#[tokio::test]
async fn empty_questions_never_hit_the_network() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let client = client(&server).await;
    let error = client
        .system_one("x", Vec::<(String, Question)>::new())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("At least one question"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn empty_score_criteria_never_hit_the_network() {
    let server = MockServer::start().await;
    let client = client(&server).await;
    let error = client
        .system_one("x", [("rating", Question::score("?", Vec::<&str>::new()))])
        .await
        .unwrap_err();
    assert!(error.to_string().contains("\"rating\" has no criteria"));
}

#[tokio::test]
async fn raw_question_passthrough() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(result_body()))
        .mount(&server)
        .await;
    let client = client(&server).await;
    let raw = json!({"type": "noul", "instructions": "Spam?", "weight": 3})
        .as_object()
        .unwrap()
        .clone();
    client
        .system_one("hi", [("q", Question::raw(raw))])
        .await
        .unwrap();
    let body: Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["questions"]["q"]["weight"], 3);
}

#[tokio::test]
async fn unknown_answer_type_is_skipped() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "test",
            "usage": {"input_tokens": 1, "output_tokens": 1},
            "answers": {
                "spam": {"type": "noul", "noul": 0.9},
                "mystery": {"type": "aurora", "value": 3}
            }
        })))
        .mount(&server)
        .await;
    let response = client(&server)
        .await
        .system_one("text", [("q", Question::noul("?"))])
        .await
        .unwrap();
    assert_eq!(response.answers.len(), 1);
    assert_eq!(response.noul("spam").unwrap().noul, 0.9);
    let raw: Value = serde_json::from_slice(response.raw_body()).unwrap();
    assert_eq!(raw["answers"]["mystery"]["type"], "aurora");
}

#[tokio::test]
async fn malformed_response_names_the_field() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-typesafe-request-id", "req-123")
                .set_body_json(json!({
                    "model": "test",
                    "usage": {"input_tokens": 1, "output_tokens": 1},
                    "answers": {"n": {"type": "noul"}}
                })),
        )
        .mount(&server)
        .await;
    let error = client(&server)
        .await
        .system_one("x", [("q", Question::noul("?"))])
        .await
        .unwrap_err();
    let api = error.api().unwrap();
    assert_eq!(api.kind, ApiErrorKind::ResponseValidation);
    assert_eq!(api.field_path.as_deref(), Some("answers.n.noul"));
    assert_eq!(api.request_id(), Some("req-123"));
    assert_eq!(
        error.to_string(),
        "POST http://127.0.0.1/placeholder: 200 Invalid response data at 'answers.n.noul'. (request_id=req-123)"
            .replace("http://127.0.0.1/placeholder", &format!("{}/v1/systemone", server.uri()))
    );
}

#[tokio::test]
async fn error_status_mapping() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("x-typesafe-request-id", "req_123")
                .insert_header("retry-after-ms", "125")
                .set_body_json(json!({"detail": {"message": "Server explanation"}})),
        )
        .mount(&server)
        .await;
    let error = client(&server).await.models().await.unwrap_err();
    let api = error.api().unwrap();
    assert_eq!(api.kind, ApiErrorKind::RateLimited);
    assert_eq!(api.status, 429);
    assert_eq!(api.retry_after_ms().map(|ms| ms.round()), Some(125.0));
    assert!(error.to_string().contains("429 Server explanation"));
    assert!(error.to_string().contains("request_id=req_123"));
}

#[tokio::test]
async fn models_list() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{
                "name": "jev-latest",
                "description": "Fast model",
                "release_date": "2026-08-01",
                "context_window": 128000
            }]
        })))
        .mount(&server)
        .await;
    let response = client(&server).await.models().await.unwrap();
    assert_eq!(response.models[0].name, "jev-latest");
    assert_eq!(response.models[0].description, "Fast model");
    let raw: Value = serde_json::from_slice(response.raw_body()).unwrap();
    assert_eq!(raw["models"][0]["context_window"], 128000);
}

#[tokio::test]
async fn retries_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("retry-after-ms", "0")
                .set_body_json(json!({"message": "temporarily unavailable"})),
        )
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"models": []})))
        .mount(&server)
        .await;

    let client = Client::builder()
        .api_key("test-key")
        .base_url(server.uri())
        .retry(RetryPolicy {
            backoff_initial: Duration::ZERO,
            backoff_max: Duration::ZERO,
            timeout: None,
            ..RetryPolicy::default()
        })
        .build()
        .unwrap();
    let response = client.models().await.unwrap();
    assert!(response.models.is_empty());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(request_header(&requests[0], "x-typesafe-retry-count").is_none());
    assert_eq!(
        request_header(&requests[1], "x-typesafe-retry-count").as_deref(),
        Some("1")
    );
    assert_eq!(
        request_header(&requests[2], "x-typesafe-retry-count").as_deref(),
        Some("2")
    );
}

#[tokio::test]
async fn noul_criteria_and_rich_json() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "custom",
            "usage": {"input_tokens": 1, "output_tokens": 1},
            "answers": {
                "risk": {
                    "type": "score",
                    "score": 0.0,
                    "confidence": 1.0,
                    "legend": {"0": {"summary": "duplicated", "examples": ["charged twice"]}},
                    "probabilities": {"0": 1.0}
                }
            }
        })))
        .mount(&server)
        .await;
    let criteria = json!({"summary": "duplicated", "examples": ["charged twice"]});
    let client = client(&server).await;
    let response = client
        .system_one_opts(
            "a ticket",
            [
                (
                    "duplicate",
                    Question::noul(
                        JsonContent::from_value(json!({"question": "Duplicate?"})).unwrap(),
                    )
                    .with_noul_criteria(
                        NoulCriteria::new().yes(JsonContent::from_value(criteria.clone()).unwrap()),
                    ),
                ),
                (
                    "risk",
                    Question::score(
                        "Risk?",
                        [JsonContent::from_value(criteria.clone()).unwrap()],
                    ),
                ),
            ],
            SystemOneOpts {
                model: Some("custom".to_string()),
                ..SystemOneOpts::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        Value::from(response.score("risk").unwrap().legend[&0].clone()),
        criteria
    );
}

#[test]
fn missing_api_key() {
    unsafe { std::env::remove_var("TYPESAFE_API_KEY") };
    let error = Client::from_env().unwrap_err();
    assert!(error.to_string().contains("TYPESAFE_API_KEY"));
}

#[test]
fn zero_timeout_is_rejected() {
    let error = Client::builder()
        .api_key("k")
        .timeout(Duration::ZERO)
        .build()
        .unwrap_err();
    assert!(error.to_string().contains("timeout"));
}

#[tokio::test]
async fn custom_base_url_serves_system_one_and_models() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/gateway/v1/systemone"))
        .and(header("authorization", "Bearer local-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "local-model",
            "usage": {"input_tokens": 1, "output_tokens": 1},
            "answers": {"billing": {"type": "noul", "noul": 0.5}}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/gateway/v1/models"))
        .and(header("authorization", "Bearer local-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{
                "name": "local-model",
                "description": "not TypeSafe",
                "release_date": "2026-01-01"
            }]
        })))
        .mount(&server)
        .await;

    let client = Client::builder()
        .api_key("local-key")
        .base_url(format!("{}/gateway/", server.uri()))
        .model("local-model")
        .retry(RetryPolicy::disabled())
        .build()
        .unwrap();
    let response = client
        .system_one("ticket", [("billing", Question::noul("Billing?"))])
        .await
        .unwrap();
    let models = client.models().await.unwrap();

    assert_eq!(response.noul("billing").unwrap().noul, 0.5);
    assert_eq!(models.models[0].name, "local-model");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url.path(), "/gateway/v1/systemone");
    assert_eq!(requests[1].url.path(), "/gateway/v1/models");
    assert!(
        requests
            .iter()
            .all(|request| request.url.host_str() != Some("api.typesafe.ai"))
    );
}

#[tokio::test]
async fn same_origin_307_is_followed() {
    let server = MockServer::start().await;
    let landed = format!("{}/v1/systemone-landed", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(307).insert_header("location", landed.as_str()))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone-landed"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "local-model",
            "usage": {"input_tokens": 1, "output_tokens": 1},
            "answers": {"q": {"type": "noul", "noul": 0.2}}
        })))
        .mount(&server)
        .await;

    let response = client(&server)
        .await
        .system_one("x", [("q", Question::noul("?"))])
        .await
        .unwrap();
    assert_eq!(response.noul("q").unwrap().noul, 0.2);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].url.path(), "/v1/systemone-landed");
    let body: Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(body["state"], "x");
    assert_eq!(
        request_header(&requests[1], "authorization").as_deref(),
        Some("Bearer test-key")
    );
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\npng-data";
const JPEG: &[u8] = b"\xff\xd8\xffjpeg-data";
const GIF: &[u8] = b"GIF89agif-data";

/// A server that answers every `POST /v1/systemone` with `result_body()`.
async fn system_one_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(result_body()))
        .mount(&server)
        .await;
    server
}

fn uncompressed_client(server: &MockServer) -> Client {
    Client::builder()
        .api_key("test-key")
        .base_url(server.uri())
        .retry(RetryPolicy::disabled())
        .compression(Compression::None)
        .build()
        .unwrap()
}

/// The boundary of a `multipart/form-data` request, checked to be 32 hex digits.
fn boundary(request: &wiremock::Request) -> String {
    let content_type = request_header(request, "content-type").unwrap();
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .unwrap_or_else(|| panic!("not multipart: {content_type}"));
    assert_eq!(boundary.len(), 32, "{boundary}");
    assert!(
        boundary
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    boundary.to_string()
}

/// The exact multipart body the Python SDK writes for `json` and `(part content type, bytes)`.
fn multipart(boundary: &str, json: &str, parts: &[(&str, &[u8])]) -> Vec<u8> {
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"request\"\r\nContent-Type: application/json\r\n\r\n{json}\r\n"
    )
    .into_bytes();
    for (index, (content_type, bytes)) in parts.iter().enumerate() {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"asset-{index}\"; filename=\"asset-{index}\"\r\nContent-Type: {content_type}\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

#[test]
fn images_are_assets_by_identity() {
    let image = Image::from_bytes(PNG);
    let same_bytes = Image::from_bytes(PNG);
    let reference = image.to_string();
    assert_eq!(reference, image.reference());
    let hex = reference
        .strip_prefix("<image:")
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap();
    assert_eq!(hex.len(), 32);
    assert!(hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
    assert_eq!(image.clone(), image);
    assert_eq!(image.clone().reference(), reference);
    assert_ne!(same_bytes, image);
    assert_ne!(same_bytes.reference(), reference);
    // The lint sees the image's read-once cache. Hashing uses identity only.
    #[allow(clippy::mutable_key_type)]
    let set: std::collections::HashSet<Image> = [image.clone(), image.clone(), same_bytes].into();
    assert_eq!(set.len(), 2);
}

#[test]
fn base64_images_follow_the_python_rules() {
    let cases = [
        ("", "Image data must contain non-empty ASCII base64 text."),
        (
            " \r\n\t",
            "Image data must contain non-empty ASCII base64 text.",
        ),
        (
            "iVBORw0é",
            "Image data must contain non-empty ASCII base64 text.",
        ),
        ("iVBORw0", "Image data must contain valid base64 text."),
        ("iVBO-w0K", "Image data must contain valid base64 text."),
        (
            "data:image/png;base64,iVBORw0KGgo=",
            "Image data must contain valid base64 text.",
        ),
    ];
    for (text, message) in cases {
        let error = Image::from_base64(text).unwrap_err();
        assert_eq!(error.to_string(), message, "{text:?}");
    }
}

/// Every alias is sent, in order, then each known reference in order of first
/// appearance in the encoded body. An image under two keys is sent twice, an
/// unreferenced passed image is not sent, and unknown reference text stays text.
#[tokio::test]
async fn images_go_as_multipart_parts_named_in_assets() {
    let server = system_one_server().await;
    let client = uncompressed_client(&server);
    let listing = Image::from_bytes(PNG);
    // "\xff\xd8\xffjpeg-data", wrapped with a CRLF.
    let damage = Image::from_base64("/9j/anBl\r\nZy1kYXRh\r\n").unwrap();
    let unused = Image::from_bytes(GIF);
    let unreferenced = Image::from_bytes(JPEG);
    let garbage = format!("<image:{}>", "0".repeat(32));
    client
        .system_one_opts(
            json!(["Compare <listing>.", listing.to_string(), garbage, {"nested": [damage.to_string()]}]),
            [("q", Question::noul(format!("Is {damage} damaged?")))],
            SystemOneOpts {
                assets: [
                    ("<listing>".to_string(), listing.clone()),
                    ("<unused>".to_string(), unused),
                ]
                .into_iter()
                .collect(),
                images: vec![damage.clone(), unreferenced.clone()],
                ..SystemOneOpts::default()
            },
        )
        .await
        .unwrap();
    client
        .system_one_opts(
            "no image here",
            [("q", Question::noul("?"))],
            SystemOneOpts {
                images: vec![unreferenced],
                ..SystemOneOpts::default()
            },
        )
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let boundary = boundary(&requests[0]);
    let json = format!(
        concat!(
            r#"{{"state":["Compare <listing>.","{a}","{g}",{{"nested":["{d}"]}}],"model":"jev-latest","#,
            r#""questions":{{"q":{{"type":"noul","instructions":"Is {d} damaged?"}}}},"#,
            r#""assets":{{"<listing>":{{"type":"image","part":"asset-0"}},"#,
            r#""<unused>":{{"type":"image","part":"asset-1"}},"#,
            r#""{a}":{{"type":"image","part":"asset-2"}},"#,
            r#""{d}":{{"type":"image","part":"asset-3"}}}}}}"#,
        ),
        a = listing,
        d = damage,
        g = garbage,
    );
    let expected = multipart(
        &boundary,
        &json,
        &[
            ("image/png", PNG),
            ("application/octet-stream", GIF),
            ("image/png", PNG),
            ("image/jpeg", JPEG),
        ],
    );
    assert_eq!(
        String::from_utf8_lossy(&requests[0].body),
        String::from_utf8_lossy(&expected)
    );
    assert_eq!(requests[0].body, expected);
    assert_eq!(
        request_header(&requests[0], "accept").as_deref(),
        Some("application/json")
    );

    // Nothing referenced and no alias: plain JSON without `assets`.
    assert_eq!(
        request_header(&requests[1], "content-type").as_deref(),
        Some("application/json")
    );
    assert_eq!(
        String::from_utf8(requests[1].body.clone()).unwrap(),
        r#"{"state":"no image here","model":"jev-latest","questions":{"q":{"type":"noul","instructions":"?"}}}"#
    );
}

/// An `assets` key in `extra_body` sends the body as JSON, unchanged, and
/// neither checks nor reads any image.
#[tokio::test]
async fn extra_body_assets_are_sent_raw() {
    let server = system_one_server().await;
    let client = uncompressed_client(&server);
    let missing = Image::from_path("/nonexistent/typesafe-sdk/missing.png");
    let mut extra = serde_json::Map::new();
    extra.insert(
        "assets".to_string(),
        json!({"<x>": {"type": "image", "format": "png", "data_base64": "iVBORw0KGgo="}}),
    );
    client
        .system_one_opts(
            format!("<x> {missing}"),
            [("q", Question::noul("?"))],
            SystemOneOpts {
                extra_body: Some(extra),
                assets: [(" ".to_string(), missing.clone())].into_iter().collect(),
                images: vec![missing.clone()],
                ..SystemOneOpts::default()
            },
        )
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        request_header(&requests[0], "content-type").as_deref(),
        Some("application/json")
    );
    assert_eq!(
        String::from_utf8(requests[0].body.clone()).unwrap(),
        format!(
            r#"{{"state":"<x> {missing}","model":"jev-latest","questions":{{"q":{{"type":"noul","instructions":"?"}}}},"assets":{{"<x>":{{"type":"image","format":"png","data_base64":"iVBORw0KGgo="}}}}}}"#
        )
    );
}

#[tokio::test]
async fn asset_errors_name_the_alias_before_any_request() {
    let server = system_one_server().await;
    let client = uncompressed_client(&server);
    let empty = Image::from_bytes(Vec::new());
    let missing = Image::from_path("missing.png");
    let good = Image::from_bytes(PNG);
    // Aliased images, referenced images, state, and the error message.
    type Case = (Vec<(&'static str, Image)>, Vec<Image>, String, String);
    let cases: Vec<Case> = vec![
        (
            vec![("<photo>", empty.clone())],
            vec![],
            "x".to_string(),
            "Asset '<photo>': Image data is empty.".to_string(),
        ),
        (
            vec![("<ok>", good.clone()), ("<photo>", missing.clone())],
            vec![],
            "x".to_string(),
            "Asset '<photo>': Image file 'missing.png' could not be read: No such file or directory."
                .to_string(),
        ),
        (
            vec![("<ok>", good.clone()), ("  ", good.clone())],
            vec![],
            "x".to_string(),
            "Asset aliases must be non-empty strings.".to_string(),
        ),
        (
            vec![],
            vec![empty.clone()],
            format!("see {empty}"),
            format!("Asset '{empty}': Image data is empty."),
        ),
    ];
    for (assets, images, state, message) in cases {
        let error = client
            .system_one_opts(
                state,
                [("q", Question::noul("?"))],
                SystemOneOpts {
                    assets: assets
                        .into_iter()
                        .map(|(alias, image)| (alias.to_string(), image))
                        .collect(),
                    images,
                    ..SystemOneOpts::default()
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(error, typesafe_sdk::Error::Sdk(_)), "{error:?}");
        assert_eq!(error.to_string(), message);
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A file is read once, on first send, and kept: a retry sends the same body,
/// and a later call sends the first bytes without reading the file again.
#[tokio::test]
async fn file_images_are_read_once_and_reused_on_retries() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).insert_header("retry-after-ms", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(result_body()))
        .mount(&server)
        .await;
    let file = std::env::temp_dir().join(format!("typesafe-sdk-{:x}.png", fastrand::u64(..)));
    std::fs::write(&file, PNG).unwrap();
    let client = Client::builder()
        .api_key("test-key")
        .base_url(server.uri())
        .compression(Compression::None)
        .retry(RetryPolicy {
            backoff_initial: Duration::ZERO,
            backoff_max: Duration::ZERO,
            ..RetryPolicy::default()
        })
        .build()
        .unwrap();
    let photo = Image::from_path(&file);
    let opts = || SystemOneOpts {
        assets: [("<photo>".to_string(), photo.clone())]
            .into_iter()
            .collect(),
        ..SystemOneOpts::default()
    };
    client
        .system_one_opts("<photo>", [("q", Question::noul("?"))], opts())
        .await
        .unwrap();
    // A second read would fail now.
    std::fs::remove_file(&file).unwrap();
    client
        .system_one_opts("<photo>", [("q", Question::noul("?"))], opts())
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].body, requests[1].body);
    assert_eq!(
        request_header(&requests[1], "x-typesafe-retry-count").as_deref(),
        Some("1")
    );
    let json = r#"{"state":"<photo>","model":"jev-latest","questions":{"q":{"type":"noul","instructions":"?"}},"assets":{"<photo>":{"type":"image","part":"asset-0"}}}"#;
    assert_eq!(
        requests[2].body,
        multipart(&boundary(&requests[2]), json, &[("image/png", PNG)])
    );
}

/// Attached images are sent when referenced, by every clone of the client,
/// until the last guard holding them is dropped.
#[tokio::test]
async fn attached_images_are_held_until_the_last_guard_drops() {
    fn assert_send<T: Send + 'static>(value: T) -> T {
        value
    }
    let server = system_one_server().await;
    let client = uncompressed_client(&server);
    let clone = client.clone();
    let other = uncompressed_client(&server);
    let photo = Image::from_bytes(PNG);
    let idle = Image::from_bytes(JPEG);
    let state = format!("Look at {photo}");
    let send = |client: Client| {
        let state = state.clone();
        async move {
            client
                .system_one(state, [("q", Question::noul("?"))])
                .await
                .unwrap();
        }
    };

    let first = assert_send(client.attach_assets([photo.clone(), idle]));
    let second = clone.attach_assets([photo.clone()]);
    send(clone.clone()).await;
    send(other.clone()).await;
    drop(first);
    send(client.clone()).await;
    drop(second);
    send(client.clone()).await;

    let requests = server.received_requests().await.unwrap();
    let json = format!(
        r#"{{"state":"Look at {photo}","model":"jev-latest","questions":{{"q":{{"type":"noul","instructions":"?"}}}},"assets":{{"{photo}":{{"type":"image","part":"asset-0"}}}}}}"#
    );
    let plain = format!(
        r#"{{"state":"Look at {photo}","model":"jev-latest","questions":{{"q":{{"type":"noul","instructions":"?"}}}}}}"#
    );
    for (index, attached) in [(0, true), (1, false), (2, true), (3, false)] {
        let request = &requests[index];
        if attached {
            assert_eq!(
                request.body,
                multipart(&boundary(request), &json, &[("image/png", PNG)]),
                "request {index}"
            );
        } else {
            assert_eq!(
                String::from_utf8(request.body.clone()).unwrap(),
                plain,
                "request {index}"
            );
        }
    }
}

#[tokio::test]
async fn box_answers_decode_from_the_client() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "jev-latest",
            "answers": {
                "dog": {"type": "box", "box": {"top": 0.2, "bottom": 0.66, "left": 0.19, "right": 0.68}, "confidence": 0.9},
                "spam": {"type": "noul", "noul": 0.1},
                "cat": {"type": "box", "box": {"top": 0, "bottom": 1, "left": 0, "right": 1}, "confidence": null}
            }
        })))
        .mount(&server)
        .await;
    let response = client(&server)
        .await
        .system_one(
            "<image>",
            [
                ("dog", Question::box_("The dog")),
                ("cat", Question::box_bare()),
            ],
        )
        .await
        .unwrap();
    let dog = response.box_("dog").unwrap();
    assert_eq!(
        dog.box_,
        typesafe_sdk::BoxCoordinates::new(0.2, 0.66, 0.19, 0.68)
    );
    assert_eq!(dog.confidence, Some(0.9));
    assert_eq!(response.box_("cat").unwrap().confidence, None);
    assert!(response.box_("spam").is_err());
    assert_eq!(
        response.boxes().map(|(name, _)| name).collect::<Vec<_>>(),
        ["dog", "cat"]
    );
    let body =
        String::from_utf8(server.received_requests().await.unwrap()[0].body.clone()).unwrap();
    assert_eq!(
        body,
        r#"{"state":"<image>","model":"jev-latest","questions":{"dog":{"type":"box","instructions":"The dog"},"cat":{"type":"box"}}}"#
    );
}

#[tokio::test]
async fn invalid_questions_never_hit_the_network() {
    let server = system_one_server().await;
    let client = client(&server).await;
    let raw = |value: Value| Question::raw(value.as_object().unwrap().clone());
    let cases = [
        (
            Question::choice("?", Vec::<(String, Option<JsonContent>)>::new()),
            r#"Choice question "q" has no criteria; at least one choice is required."#,
        ),
        (
            raw(json!({"type": "choice", "criteria": {}})),
            r#"Choice question "q" has no criteria; at least one choice is required."#,
        ),
        (
            raw(json!({"type": "choice"})),
            r#"Question "q" requires "criteria"."#,
        ),
        (
            raw(json!({"type": "score", "criteria": null})),
            r#"Score question "q" has no criteria; at least one score is required."#,
        ),
        (
            Question::noul_bare(),
            r#"Noul question "q" has neither instructions nor criteria; at least one is required."#,
        ),
        (
            Question::noul_bare().with_noul_criteria(NoulCriteria::new()),
            r#"Noul question "q" has neither instructions nor criteria; at least one is required."#,
        ),
        (
            raw(json!({"type": "noul", "criteria": {"true": null}})),
            r#"Noul question "q" has neither instructions nor criteria; at least one is required."#,
        ),
        (
            raw(json!({"type": "noul", "instructions": null})),
            r#"Noul question "q" has neither instructions nor criteria; at least one is required."#,
        ),
        (
            raw(json!({"type": "noul", "instructions": "?", "criteria": ["yes"]})),
            r#"Noul question "q" criteria must be an object."#,
        ),
    ];
    for (question, message) in cases {
        let error = client.system_one("x", [("q", question)]).await.unwrap_err();
        assert_eq!(error.to_string(), message);
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // A described outcome, or empty instructions, is enough.
    for question in [
        raw(json!({"type": "noul", "criteria": {"false": "x"}})),
        raw(json!({"type": "noul", "instructions": ""})),
        raw(json!({"type": "box"})),
        Question::noul_bare().with_noul_criteria(NoulCriteria::new().no("x")),
    ] {
        client.system_one("x", [("q", question)]).await.unwrap();
    }
}

#[test]
fn api_keys_are_trimmed_and_checked_at_build() {
    let empty =
        "No API key was provided. Pass api_key or set the TYPESAFE_API_KEY environment variable.";
    let invalid = "API key must contain only printable ASCII characters without whitespace.";
    let cases = [
        ("", empty),
        (" \r\n", empty),
        ("sk secret", invalid),
        ("sk\tsecret", invalid),
        ("sk\nsecret", invalid),
        ("sk\u{7f}secret", invalid),
        ("sk\u{a0}secret", invalid),
        ("sk\u{200b}secret", invalid),
        ("sk-sécret", invalid),
    ];
    for (key, message) in cases {
        let error = Client::builder().api_key(key).build().unwrap_err();
        assert_eq!(error.to_string(), message, "{key:?}");
        assert!(!format!("{error:?}").contains("secret"), "{key:?}");
        #[cfg(feature = "blocking")]
        {
            let error = typesafe_sdk::blocking::Client::builder()
                .api_key(key)
                .build()
                .err()
                .unwrap();
            assert_eq!(error.to_string(), message, "{key:?}");
        }
    }
}

#[tokio::test]
async fn api_key_is_sent_trimmed() {
    let server = system_one_server().await;
    Client::builder()
        .api_key("  sk-trimmed\r\n")
        .base_url(server.uri())
        .build()
        .unwrap()
        .system_one("x", [("q", Question::noul("?"))])
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        request_header(&requests[0], "authorization").as_deref(),
        Some("Bearer sk-trimmed")
    );
}

#[test]
fn debug_output_hides_the_key_and_secret_headers() {
    let mut headers = http::HeaderMap::new();
    headers.insert("x-api-key", "header-secret".parse().unwrap());
    headers.insert("x-trace", "visible".parse().unwrap());
    let builder = Client::builder()
        .api_key("sk-hidden")
        .headers(headers.clone());
    let client = builder.clone().build().unwrap();
    let opts = SystemOneOpts {
        extra_headers: headers.clone(),
        ..SystemOneOpts::default()
    };
    let models = typesafe_sdk::ModelsOpts {
        extra_headers: headers,
        ..typesafe_sdk::ModelsOpts::default()
    };
    for shown in [
        format!("{builder:?}"),
        format!("{client:?}"),
        format!("{opts:?}"),
        format!("{models:?}"),
    ] {
        assert!(!shown.contains("sk-hidden"), "{shown}");
        assert!(!shown.contains("header-secret"), "{shown}");
        assert!(shown.contains("***"), "{shown}");
        assert!(shown.contains("visible"), "{shown}");
    }
}

#[tokio::test]
async fn decode_as_reads_the_raw_body_and_names_the_failing_field() {
    #[derive(serde::Deserialize)]
    struct Answers {
        spam: Spam,
    }
    #[derive(serde::Deserialize)]
    struct Spam {
        noul: f64,
    }
    #[derive(serde::Deserialize)]
    struct Mine {
        answers: Answers,
    }
    #[derive(Debug, serde::Deserialize)]
    struct Wrong {
        #[allow(dead_code)]
        answers: std::collections::HashMap<String, WrongAnswer>,
    }
    #[derive(Debug, serde::Deserialize)]
    struct WrongAnswer {
        #[allow(dead_code)]
        noul: String,
    }

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-typesafe-request-id", "req-9")
                .set_body_json(result_body()),
        )
        .mount(&server)
        .await;
    let response = client(&server)
        .await
        .system_one("x", [("spam", Question::noul("?"))])
        .await
        .unwrap();
    assert_eq!(
        response.decode_as::<Mine>().unwrap().answers.spam.noul,
        0.98
    );

    let error = response.decode_as::<Wrong>().unwrap_err();
    let api = error.api().unwrap();
    assert_eq!(api.kind, ApiErrorKind::ResponseValidation);
    assert_eq!(api.field_path.as_deref(), Some("answers.spam.noul"));
    assert_eq!(api.request_id(), Some("req-9"));
    assert_eq!(api.status, 200);

    let built = typesafe_sdk::SystemOneResponse::new(
        "m",
        typesafe_sdk::Usage::default(),
        Vec::<(String, typesafe_sdk::Answer)>::new(),
    );
    assert!(matches!(
        built.decode_as::<Value>(),
        Err(typesafe_sdk::Error::Sdk(_))
    ));
}

/// The JSON body for state `"x" * n` and one noul question.
#[cfg(feature = "zstd")]
fn sized_body(n: usize) -> String {
    format!(
        r#"{{"state":"{}","model":"jev-latest","questions":{{"q":{{"type":"noul","instructions":"?"}}}}}}"#,
        "x".repeat(n)
    )
}

/// Bodies of 1024 bytes or more go zstd-compressed with `Content-Encoding: zstd`,
/// compressed once for every retry. Smaller bodies, `GET /v1/models`, and a
/// client built with `Compression::None` send plain bytes.
#[cfg(feature = "zstd")]
#[tokio::test]
async fn large_bodies_are_zstd_compressed_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).insert_header("retry-after-ms", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(result_body()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"models": []})))
        .mount(&server)
        .await;
    let builder = Client::builder()
        .api_key("test-key")
        .base_url(server.uri())
        .retry(RetryPolicy {
            backoff_initial: Duration::ZERO,
            backoff_max: Duration::ZERO,
            ..RetryPolicy::default()
        });
    let client = builder.clone().build().unwrap();
    let plain = builder.compression(Compression::None).build().unwrap();
    let overhead = sized_body(0).len();
    let send = |client: &Client, n: usize| {
        let client = client.clone();
        async move {
            client
                .system_one("x".repeat(n), [("q", Question::noul("?"))])
                .await
                .unwrap();
        }
    };
    send(&client, 1024 - overhead).await; // 503, then retried
    send(&client, 1023 - overhead).await;
    send(&plain, 4096).await;
    client.models().await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 5);
    for request in &requests[..2] {
        assert_eq!(
            request_header(request, "content-encoding").as_deref(),
            Some("zstd")
        );
        assert_eq!(
            request_header(request, "content-type").as_deref(),
            Some("application/json")
        );
        let decoded = zstd::decode_all(&request.body[..]).unwrap();
        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            sized_body(1024 - overhead)
        );
    }
    assert_eq!(requests[0].body, requests[1].body);
    assert!(request_header(&requests[2], "content-encoding").is_none());
    assert_eq!(requests[2].body, sized_body(1023 - overhead).into_bytes());
    assert!(request_header(&requests[3], "content-encoding").is_none());
    assert_eq!(requests[3].body, sized_body(4096).into_bytes());
    assert!(request_header(&requests[4], "content-encoding").is_none());
}

/// A multipart body is compressed as a whole and keeps its multipart content type.
#[cfg(feature = "zstd")]
#[tokio::test]
async fn multipart_bodies_are_compressed_whole() {
    let server = system_one_server().await;
    let client = client(&server).await;
    let mut data = PNG.to_vec();
    data.resize(4096, 7);
    let photo = Image::from_bytes(data.clone());
    client
        .system_one_opts(
            "<photo>",
            [("q", Question::noul("?"))],
            SystemOneOpts {
                assets: [("<photo>".to_string(), photo)].into_iter().collect(),
                ..SystemOneOpts::default()
            },
        )
        .await
        .unwrap();
    let request = &server.received_requests().await.unwrap()[0];
    assert_eq!(
        request_header(request, "content-encoding").as_deref(),
        Some("zstd")
    );
    let json = r#"{"state":"<photo>","model":"jev-latest","questions":{"q":{"type":"noul","instructions":"?"}},"assets":{"<photo>":{"type":"image","part":"asset-0"}}}"#;
    assert_eq!(
        zstd::decode_all(&request.body[..]).unwrap(),
        multipart(&boundary(request), json, &[("image/png", &data)])
    );
}

#[cfg(not(feature = "zstd"))]
#[tokio::test]
async fn without_the_zstd_feature_bodies_go_uncompressed() {
    let error = Client::builder()
        .api_key("k")
        .compression(Compression::Zstd)
        .build()
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "zstd compression requires the `zstd` feature of typesafe-sdk."
    );
    let server = system_one_server().await;
    client(&server)
        .await
        .system_one("x".repeat(4096), [("q", Question::noul("?"))])
        .await
        .unwrap();
    let request = &server.received_requests().await.unwrap()[0];
    assert!(request_header(request, "content-encoding").is_none());
}
