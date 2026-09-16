use std::collections::HashMap;

use serde_json::json;
use typesafe_sdk::{
    AsyncTypeSafeClient, Choice, ModelMetadata, Noul, Question, Questions, RetryPolicy, Score,
    TypeSafeBadRequestError, TypeSafeError,
};

fn sample_result() -> serde_json::Value {
    json!({
        "model": "jev-latest",
        "usage": { "input_tokens": 12, "output_tokens": 3 },
        "answers": {
            "spam": { "type": "noul", "noul": 0.98 },
            "tone": {
                "type": "choice",
                "choice": "friendly",
                "confidence": 0.9,
                "probabilities": { "friendly": 0.9, "hostile": 0.1 }
            },
            "quality": {
                "type": "score",
                "score": 1.7,
                "confidence": 0.8,
                "legend": { "0": "bad", "1": "ok", "2": "great" },
                "probabilities": { "0": 0.1, "1": 0.1, "2": 0.8 }
            }
        }
    })
}

fn questions_typed() -> Questions {
    let mut q = Questions::new();
    q.insert(
        "spam".into(),
        Question::Noul(Noul::new(Some(json!("Spam?")))),
    );
    q.insert(
        "tone".into(),
        Question::Choice(Choice::new(
            Some(json!("Tone?")),
            HashMap::from([("friendly".into(), None), ("hostile".into(), None)]),
        )),
    );
    q.insert(
        "quality".into(),
        Question::Score(Score::new(
            Some(json!("Quality?")),
            vec![json!("bad"), json!("ok"), json!("great")],
        )),
    );
    q
}

#[tokio::test]
async fn system_one_round_trip() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/systemone"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(sample_result()))
        .mount(&server)
        .await;

    let client = AsyncTypeSafeClient::new(
        Some("test-key".into()),
        None,
        Some(RetryPolicy {
            max_retries: 0,
            ..RetryPolicy::default()
        }),
        None,
        None,
        Some(server.uri()),
    )
    .unwrap();

    let response = client
        .system_one(
            json!({"document": "Hello 🌍"}),
            &questions_typed(),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(response.model, "jev-latest");
    assert_eq!(response.usage.input_tokens, Some(12));
    assert_eq!(response.nouls().len(), 1);
    assert_eq!(response.choices().len(), 1);
    assert_eq!(response.scores().len(), 1);
    assert!((response.nouls()["spam"].noul - 0.98).abs() < f64::EPSILON);
    assert_eq!(response.choices()["tone"].choice, "friendly");
    assert!((response.scores()["quality"].score - 1.7).abs() < f64::EPSILON);
    assert_eq!(response.scores()["quality"].legend.get(&0), Some(&json!("bad")));
}

#[tokio::test]
async fn models_list() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1/models"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "models": [{
                    "name": "jev-latest",
                    "description": "Fast model",
                    "release_date": "2026-08-01"
                }]
            })),
        )
        .mount(&server)
        .await;

    let client = AsyncTypeSafeClient::new(
        Some("test-key".into()),
        None,
        Some(RetryPolicy {
            max_retries: 0,
            ..RetryPolicy::default()
        }),
        None,
        None,
        Some(server.uri()),
    )
    .unwrap();

    let models = client.models().list(None, None, None).await.unwrap();
    assert_eq!(
        models.models,
        vec![ModelMetadata {
            name: "jev-latest".into(),
            description: "Fast model".into(),
            release_date: "2026-08-01".into(),
        }]
    );
}

#[tokio::test]
async fn missing_api_key() {
    let err = match AsyncTypeSafeClient::new(None, None, None, None, None, None) {
        Ok(_) => panic!("expected error"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("TYPESAFE_API_KEY"));
}

#[tokio::test]
async fn empty_questions_rejected() {
    let client = AsyncTypeSafeClient::new(
        Some("k".into()),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let err = client
        .system_one(json!("hi"), &Questions::new(), None, None, None, None, None)
        .await
        .unwrap_err();
    assert!(err.downcast_ref::<TypeSafeError>().is_some() || err.to_string().contains("At least one question"));
}

#[tokio::test]
async fn maps_http_400_to_bad_request() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1/models"))
        .respond_with(
            wiremock::ResponseTemplate::new(400).set_body_json(json!({"message": "Bad request"})),
        )
        .mount(&server)
        .await;

    let client = AsyncTypeSafeClient::new(
        Some("test-key".into()),
        None,
        Some(RetryPolicy {
            max_retries: 0,
            ..RetryPolicy::default()
        }),
        None,
        None,
        Some(server.uri()),
    )
    .unwrap();

    let err = client.models().list(None, None, None).await.unwrap_err();
    let bad = err
        .downcast_ref::<TypeSafeBadRequestError>()
        .expect("bad request type");
    assert_eq!(bad.0.status, 400);
}

#[test]
fn retry_policy_validates_timeout() {
    let err = RetryPolicy {
        timeout_budget_secs: Some(0.0),
        ..RetryPolicy::default()
    }
    .validate()
    .unwrap_err();
    assert!(err.to_string().contains("timeout"));
}
