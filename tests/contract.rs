use std::time::Duration;

use serde_json::{json, Value};
use typesafe_sdk::{
    ApiErrorKind, Client, JsonContent, NoulCriteria, Question, RetryPolicy, SystemOneOpts,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request_header(request: &wiremock::Request, name: &str) -> Option<String> {
    request.headers.iter().find_map(|(key, values)| {
        if key.as_str().eq_ignore_ascii_case(name) {
            values.get(0).map(ToString::to_string)
        } else {
            None
        }
    })
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
    assert!(request_header(&received[0], "user-agent")
        .unwrap()
        .starts_with("typesafe-sdk/"));
    assert!(request_header(&received[0], "x-typesafe-runtime")
        .unwrap()
        .starts_with("rust/"));
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
    std::env::remove_var("TYPESAFE_API_KEY");
    let error = Client::builder().build().unwrap_err();
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
