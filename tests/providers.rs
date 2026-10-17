//! Provider contract tests: the same `Question`s and typed answers over
//! TypeSafe System One, Cloudflare Clef, and OpenAI Decisions.
//!
//! Clef fixtures are seeded from the production Workers AI transport in
//! ultra-instinct (`crates/aui-cli/src/clef.rs`). OpenAI fixtures are from the
//! public guide (developers.openai.com/api/docs/guides/decisions), unverified
//! against the live API.
#![cfg(all(feature = "clef", feature = "openai-decisions"))]

use indexmap::IndexMap;
use serde_json::{Value, json};
use typesafe_sdk::{
    Answer, ApiErrorKind, ChoiceAnswer, Client, Error, Image, JsonContent, NoulAnswer,
    NoulCriteria, Provider, Question, RetryPolicy, ScoreAnswer, SystemOneOpts,
};
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCOUNT: &str = "acc123";
const CLEF_PATH: &str = "/accounts/acc123/ai/run/@cf/cloudflare/clef";

fn clef() -> Provider {
    Provider::Clef {
        account_id: ACCOUNT.to_string(),
    }
}

fn client(server: &MockServer, provider: Provider) -> Client {
    Client::builder()
        .api_key("test-key")
        .provider(provider)
        .base_url(server.uri())
        .retry(RetryPolicy::disabled())
        .build()
        .unwrap()
}

fn header(request: &wiremock::Request, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn logical_questions() -> [(&'static str, Question); 3] {
    [
        ("billing", Question::noul("Is this about billing?")),
        (
            "tone",
            Question::choice(
                "What is the tone?",
                [("calm", Some("Even and polite".into())), ("angry", None)],
            ),
        ),
        (
            "urgency",
            Question::score("How urgent?", ["can wait", "today"]),
        ),
    ]
}

/// The System One answers object, also Clef's `result.answers`.
fn system_one_answers() -> Value {
    json!({
        "billing": {"type": "noul", "noul": 0.97},
        "tone": {
            "type": "choice",
            "choice": "angry",
            "confidence": 0.8,
            "probabilities": {"calm": 0.2, "angry": 0.8}
        },
        "urgency": {
            "type": "score",
            "score": 0.9,
            "confidence": 0.9,
            "legend": {"0": "can wait", "1": "today"},
            "probabilities": {"0": 0.1, "1": 0.9}
        }
    })
}

fn expected_answers() -> IndexMap<String, Answer> {
    [
        ("billing", Answer::Noul(NoulAnswer::new(0.97))),
        (
            "tone",
            Answer::Choice(ChoiceAnswer::new(
                "angry",
                0.8,
                [("calm", 0.2), ("angry", 0.8)],
            )),
        ),
        (
            "urgency",
            Answer::Score(ScoreAnswer::new(
                0.9,
                0.9,
                [(0, "can wait"), (1, "today")],
                [(0, 0.1), (1, 0.9)],
            )),
        ),
    ]
    .into_iter()
    .map(|(name, answer)| (name.to_string(), answer))
    .collect()
}

#[tokio::test]
async fn providers_send_golden_requests_and_decode_the_same_answers() {
    let usage = json!({"input_tokens": 21, "output_tokens": 3});
    let cases = [
        (
            Provider::TypeSafe,
            "/v1/systemone",
            r#"{"state":"I was charged twice.","model":"jev-latest","questions":{"billing":{"type":"noul","instructions":"Is this about billing?"},"tone":{"type":"choice","instructions":"What is the tone?","criteria":{"calm":"Even and polite","angry":null}},"urgency":{"type":"score","instructions":"How urgent?","criteria":["can wait","today"]}}}"#,
            json!({"model": "jev-latest", "usage": usage, "answers": system_one_answers()}),
            "jev-latest",
            Some(21),
        ),
        (
            clef(),
            CLEF_PATH,
            r#"{"state":"I was charged twice.","model":"clef","questions":{"billing":{"type":"noul","instructions":"Is this about billing?"},"tone":{"type":"choice","instructions":"What is the tone?","criteria":{"calm":"Even and polite","angry":null}},"urgency":{"type":"score","instructions":"How urgent?","criteria":["can wait","today"]}}}"#,
            json!({
                "success": true,
                "errors": [],
                "messages": [],
                "result": {"model": "clef", "usage": usage, "answers": system_one_answers()}
            }),
            "clef",
            Some(21),
        ),
        (
            Provider::OpenAiDecisions,
            "/v1/decisions",
            r#"{"model":"gpt-6-luna","input":"I was charged twice.","questions":[{"type":"predicate","name":"billing","instructions":"Is this about billing?"},{"type":"choice","name":"tone","instructions":"What is the tone?","choices":[{"value":"calm","description":"Even and polite"},{"value":"angry"}]},{"type":"score","name":"urgency","instructions":"How urgent?","levels":[{"label":"0","description":"can wait"},{"label":"1","description":"today"}]}]}"#,
            // Answers out of question order: the client restores it.
            json!({"answers": [
                {
                    "type": "score",
                    "name": "urgency",
                    "score": 0.9,
                    "probabilities": [
                        {"value": 0, "label": "0", "probability": 0.1},
                        {"value": 1, "label": "1", "probability": 0.9}
                    ],
                    "confidence": 0.9
                },
                {"type": "predicate", "name": "billing", "probability": 0.97},
                {
                    "type": "choice",
                    "name": "tone",
                    "choice": "angry",
                    "probabilities": [
                        {"value": "calm", "probability": 0.2},
                        {"value": "angry", "probability": 0.8}
                    ],
                    "confidence": 0.8
                }
            ]}),
            "gpt-6-luna",
            None,
        ),
    ];
    for (provider, route, golden, reply, model, input_tokens) in cases {
        let name = format!("{provider:?}");
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(reply))
            .expect(1)
            .mount(&server)
            .await;
        let response = client(&server, provider)
            .system_one("I was charged twice.", logical_questions())
            .await
            .unwrap_or_else(|error| panic!("{name}: {error}"));

        let received = server.received_requests().await.unwrap();
        let request = &received[0];
        assert_eq!(String::from_utf8_lossy(&request.body), golden, "{name}");
        assert_eq!(
            header(request, "authorization").as_deref(),
            Some("Bearer test-key"),
            "{name}"
        );
        assert_eq!(
            header(request, "content-type").as_deref(),
            Some("application/json"),
            "{name}"
        );
        assert_eq!(response.answers, expected_answers(), "{name}");
        // `IndexMap` equality ignores order, so check it apart.
        assert_eq!(
            response.answers.keys().collect::<Vec<_>>(),
            ["billing", "tone", "urgency"],
            "{name}"
        );
        assert_eq!(response.model, model, "{name}");
        assert_eq!(response.usage.input_tokens, input_tokens, "{name}");
    }
}

/// clef.rs sends structured `state` and `instructions` and decodes
/// `result.answers`; the Clef provider passes both through unchanged.
#[tokio::test]
async fn clef_sends_structured_content_and_the_model_in_path_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/accounts/acc123/ai/run/@cf/cloudflare/clef-flash"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "errors": [],
            "result": {
                "model": "clef-flash",
                "answers": {
                    "operation": {
                        "type": "choice",
                        "choice": "CLICK",
                        "confidence": 0.9,
                        "probabilities": {"CLICK": 0.9, "TYPE_TEXT": 0.05, "SCROLL_DOWN": 0.03, "DONE": 0.02}
                    }
                }
            }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let instructions = JsonContent::from_value(json!({"goal": "Send the message"})).unwrap();
    let response = client(&server, clef())
        .system_one_opts(
            json!({"goal": "Send the message", "actions": [{"id": "CLICK:n12"}]}),
            [(
                "operation",
                Question::choice(
                    instructions,
                    [
                        ("CLICK", None),
                        ("TYPE_TEXT", None),
                        ("SCROLL_DOWN", None),
                        ("DONE", None),
                    ],
                ),
            )],
            SystemOneOpts {
                model: Some("clef-flash".to_string()),
                ..SystemOneOpts::default()
            },
        )
        .await
        .unwrap();
    let received = server.received_requests().await.unwrap();
    assert_eq!(
        String::from_utf8_lossy(&received[0].body),
        r#"{"state":{"goal":"Send the message","actions":[{"id":"CLICK:n12"}]},"model":"clef-flash","questions":{"operation":{"type":"choice","instructions":{"goal":"Send the message"},"criteria":{"CLICK":null,"TYPE_TEXT":null,"SCROLL_DOWN":null,"DONE":null}}}}"#
    );
    let operation = response.choice("operation").unwrap();
    assert_eq!(operation.choice, "CLICK");
    assert_eq!(operation.probabilities["DONE"], 0.02);
    assert_eq!(response.usage.input_tokens, None);
}

/// Neither Clef nor OpenAI gets a compressed body, however large.
#[tokio::test]
async fn only_typesafe_compresses_request_bodies() {
    let state = "x".repeat(4096);
    for (provider, compressed) in [
        (Provider::TypeSafe, cfg!(feature = "zstd")),
        (clef(), false),
        (Provider::OpenAiDecisions, false),
    ] {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let name = format!("{provider:?}");
        client(&server, provider)
            .system_one(state.as_str(), [("q", Question::noul("?"))])
            .await
            .unwrap_err();
        let received = server.received_requests().await.unwrap();
        assert_eq!(
            header(&received[0], "content-encoding").is_some(),
            compressed,
            "{name}"
        );
        assert_eq!(received[0].body.len() < 4096, compressed, "{name}");
    }
}

/// Provider, state, questions, options, and the expected unsupported feature.
type UnsupportedCase<'a> = (
    Provider,
    JsonContent,
    Vec<(String, Question)>,
    SystemOneOpts,
    &'a str,
);

/// Environment variables to set, and the parts the client or error must show.
type EnvCase = (
    &'static [(&'static str, &'static str)],
    &'static [&'static str],
);

fn raw(value: Value) -> Question {
    Question::raw(value.as_object().unwrap().clone())
}

#[tokio::test]
async fn unsupported_requests_never_reach_the_server() {
    let photo = Image::from_bytes(&b"\x89PNG\r\n\x1a\npixel"[..]);
    let with_images = SystemOneOpts {
        images: vec![photo.clone()],
        ..SystemOneOpts::default()
    };
    let with_assets = SystemOneOpts {
        assets: [("<photo>".to_string(), photo.clone())]
            .into_iter()
            .collect(),
        ..SystemOneOpts::default()
    };
    let with_extra_assets = SystemOneOpts {
        extra_body: Some(json!({"assets": {}}).as_object().unwrap().clone()),
        ..SystemOneOpts::default()
    };
    let noul = || vec![("q".to_string(), Question::noul("?"))];
    let many: Vec<(String, Question)> = (0..65)
        .map(|i| (format!("q{i}"), Question::noul("?")))
        .collect();
    let long_name = "n".repeat(101);
    let long_name_error = format!(
        r#"the question name "{long_name}": names must be 1 to 100 characters from [A-Za-z0-9_.-]"#
    );
    let structured = |value: Value| JsonContent::from_value(value).unwrap();
    let cases: Vec<UnsupportedCase> = vec![
        (
            clef(),
            "x".into(),
            vec![("dog".into(), Question::box_("The dog"))],
            SystemOneOpts::default(),
            r#"box questions (question "dog")"#,
        ),
        (
            clef(),
            "x".into(),
            vec![("dog".into(), raw(json!({"type": "box"})))],
            SystemOneOpts::default(),
            r#"box questions (question "dog")"#,
        ),
        (
            clef(),
            "x".into(),
            many,
            SystemOneOpts::default(),
            "more than 64 questions in one request (got 65)",
        ),
        (
            clef(),
            "x".into(),
            vec![("has space".into(), Question::noul("?"))],
            SystemOneOpts::default(),
            r#"the question name "has space": names must be 1 to 100 characters from [A-Za-z0-9_.-]"#,
        ),
        (
            clef(),
            "x".into(),
            vec![(long_name.clone(), Question::noul("?"))],
            SystemOneOpts::default(),
            &long_name_error,
        ),
        (
            clef(),
            "x".into(),
            vec![("c".into(), Question::choice("?", [("only", None)]))],
            SystemOneOpts::default(),
            r#"a choice with one label (question "c"): Workers AI needs at least two"#,
        ),
        (
            clef(),
            "x".into(),
            vec![(
                "c".into(),
                raw(json!({"type": "choice", "criteria": {"only": null}})),
            )],
            SystemOneOpts::default(),
            r#"a choice with one label (question "c"): Workers AI needs at least two"#,
        ),
        (clef(), "x".into(), noul(), with_images.clone(), "images"),
        (clef(), "x".into(), noul(), with_assets.clone(), "images"),
        (
            clef(),
            "x".into(),
            noul(),
            with_extra_assets.clone(),
            "images",
        ),
        (
            Provider::OpenAiDecisions,
            "x".into(),
            vec![("dog".into(), Question::box_("The dog"))],
            SystemOneOpts::default(),
            r#"box questions (question "dog")"#,
        ),
        (
            Provider::OpenAiDecisions,
            "x".into(),
            noul(),
            with_images,
            "images",
        ),
        (
            Provider::OpenAiDecisions,
            "x".into(),
            noul(),
            with_assets,
            "images",
        ),
        (
            Provider::OpenAiDecisions,
            "x".into(),
            noul(),
            with_extra_assets,
            "images",
        ),
        (
            Provider::OpenAiDecisions,
            structured(json!({"doc": "x"})),
            noul(),
            SystemOneOpts::default(),
            "structured state: state must be a string",
        ),
        (
            Provider::OpenAiDecisions,
            structured(json!(["x"])),
            noul(),
            SystemOneOpts::default(),
            "structured state: state must be a string",
        ),
        (
            Provider::OpenAiDecisions,
            "x".into(),
            vec![("q".into(), Question::noul(structured(json!({"ask": "?"}))))],
            SystemOneOpts::default(),
            r#"structured instructions (question "q"): instructions must be a string"#,
        ),
        (
            Provider::OpenAiDecisions,
            "x".into(),
            vec![(
                "q".into(),
                Question::choice("?", [("a", Some(structured(json!({"d": 1})))), ("b", None)]),
            )],
            SystemOneOpts::default(),
            r#"structured criteria descriptions (question "q"): descriptions must be strings"#,
        ),
        (
            Provider::OpenAiDecisions,
            "x".into(),
            vec![(
                "q".into(),
                Question::score("?", [structured(json!(["low"])), "high".into()]),
            )],
            SystemOneOpts::default(),
            r#"structured criteria descriptions (question "q"): descriptions must be strings"#,
        ),
        (
            Provider::OpenAiDecisions,
            "x".into(),
            vec![(
                "q".into(),
                Question::noul("?").with_noul_criteria(NoulCriteria::new().yes("billing")),
            )],
            SystemOneOpts::default(),
            r#"noul criteria (question "q")"#,
        ),
        (
            Provider::OpenAiDecisions,
            "x".into(),
            vec![(
                "q".into(),
                raw(json!({"type": "noul", "criteria": {"false": "no"}})),
            )],
            SystemOneOpts::default(),
            r#"noul criteria (question "q")"#,
        ),
        (
            Provider::OpenAiDecisions,
            "x".into(),
            vec![(
                "q".into(),
                Question::Score {
                    instructions: None,
                    criteria: vec!["a".into()],
                },
            )],
            SystemOneOpts::default(),
            r#"questions without instructions (question "q")"#,
        ),
        (
            Provider::OpenAiDecisions,
            "x".into(),
            vec![(
                "q".into(),
                raw(json!({"type": "noul", "instructions": "?", "weight": 2})),
            )],
            SystemOneOpts::default(),
            r#"raw questions that are not a plain noul, choice, or score (question "q")"#,
        ),
    ];
    let server = MockServer::start().await;
    for (provider, state, questions, opts, feature) in cases {
        let expected = match &provider {
            Provider::OpenAiDecisions => "OpenAI Decisions",
            _ => "Clef",
        };
        let error = client(&server, provider)
            .system_one_opts(state, questions, opts)
            .await
            .unwrap_err();
        match &error {
            Error::Unsupported {
                provider,
                feature: got,
            } => {
                assert_eq!((*provider, got.as_str()), (expected, feature));
            }
            other => panic!("{feature}: expected Unsupported, got {other:?}"),
        }
        assert_eq!(
            error.to_string(),
            format!("The {expected} provider does not support {feature}.")
        );
    }

    // An attached image counts once the request references it.
    for provider in [clef(), Provider::OpenAiDecisions] {
        let client = client(&server, provider);
        let _attached = client.attach_assets([photo.clone()]);
        let error = client
            .system_one(format!("Photo: {photo}"), noul())
            .await
            .unwrap_err();
        assert!(
            matches!(&error, Error::Unsupported { feature, .. } if feature == "images"),
            "{error:?}"
        );
        let error = client.models().await.unwrap_err();
        assert!(
            matches!(&error, Error::Unsupported { feature, .. } if feature == "models()"),
            "{error:?}"
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // The limits themselves pass: 64 questions, a 100-character name, the
    // whole name alphabet, two labels, and an attached image not referenced.
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": {"model": "clef", "answers": {}}
        })))
        .mount(&server)
        .await;
    let mut questions: Vec<(String, Question)> = (0..62)
        .map(|i| (format!("q{i}"), Question::noul("?")))
        .collect();
    questions.push(("n".repeat(100), Question::noul("?")));
    questions.push((
        "Az09_.-".to_string(),
        Question::choice("?", [("a", None), ("b", None)]),
    ));
    let client = client(&server, clef());
    let _attached = client.attach_assets([photo.clone()]);
    client.system_one("x", questions).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn clef_errors_carry_the_cloudflare_messages() {
    let cases = [
        (
            200,
            json!({"success": false, "errors": [{"code": 7000, "message": "No route for that URI"}], "result": null}),
            ApiErrorKind::Other,
            "No route for that URI (code 7000)",
        ),
        (
            200,
            json!({"success": true, "errors": [{"message": "Model is busy"}, {"code": 3040, "message": "Capacity"}], "result": null}),
            ApiErrorKind::Other,
            "Model is busy; Capacity (code 3040)",
        ),
        (
            200,
            json!({"success": false, "errors": []}),
            ApiErrorKind::Other,
            "Cloudflare reported success: false with no errors.",
        ),
        (
            401,
            json!({"success": false, "errors": [{"code": 10000, "message": "Authentication error"}]}),
            ApiErrorKind::Authentication,
            "Authentication error (code 10000)",
        ),
    ];
    for (status, body, kind, message) in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(CLEF_PATH))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&server)
            .await;
        let error = client(&server, clef())
            .system_one("x", [("q", Question::noul("?"))])
            .await
            .unwrap_err();
        let api = error.api().unwrap_or_else(|| panic!("{error:?}"));
        assert_eq!((api.status, api.kind), (status, kind), "{message}");
        assert_eq!(
            error.to_string(),
            format!("POST {}{CLEF_PATH}: {status} {message}", server.uri())
        );
    }
}

#[tokio::test]
async fn malformed_provider_responses_name_the_field() {
    let cases = [
        (
            clef(),
            CLEF_PATH,
            json!({"success": true, "result": {"answers": {}}}),
            "result.model",
        ),
        (
            clef(),
            CLEF_PATH,
            json!({"success": true, "result": {"model": "clef", "answers": {"q": {"type": "noul"}}}}),
            "result.answers.q.noul",
        ),
        (clef(), CLEF_PATH, json!({"success": true}), "result"),
        (
            Provider::OpenAiDecisions,
            "/v1/decisions",
            json!({"answers": {"q": {"type": "predicate"}}}),
            "answers",
        ),
        (
            Provider::OpenAiDecisions,
            "/v1/decisions",
            json!({"answers": [{"type": "predicate", "name": "q"}]}),
            "answers[0].probability",
        ),
        (
            Provider::OpenAiDecisions,
            "/v1/decisions",
            json!({"answers": [{"type": "choice", "name": "q", "choice": "a", "confidence": 1.0, "probabilities": [{"probability": 1.0}]}]}),
            "answers[0].probabilities[0].value",
        ),
        (
            Provider::OpenAiDecisions,
            "/v1/decisions",
            json!({"answers": [{"type": "predicate", "probability": 0.5}]}),
            "answers[0].name",
        ),
    ];
    for (provider, route, body, field) in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let error = client(&server, provider)
            .system_one("x", [("q", Question::noul("?"))])
            .await
            .unwrap_err();
        let api = error.api().unwrap_or_else(|| panic!("{error:?}"));
        assert_eq!(api.kind, ApiErrorKind::ResponseValidation, "{field}");
        assert_eq!(api.field_path.as_deref(), Some(field));
    }
}

#[tokio::test]
async fn openai_refusals_are_answers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/decisions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"answers": [
            {"type": "predicate", "name": "damage", "probability": 0.92},
            {"type": "refusal", "name": "identity"},
            // No name: the guide's Java sample treats it as optional, so the
            // answer goes to the question at the same position.
            {"type": "refusal"},
            {"type": "verdict", "name": "extra"}
        ]})))
        .mount(&server)
        .await;
    let response = client(&server, Provider::OpenAiDecisions)
        .system_one(
            "Inspect the product in this photo.",
            [
                ("damage", Question::noul("Visible damage?")),
                ("identity", Question::noul("Who is this?")),
                ("age", Question::noul("Is this person over 30?")),
            ],
        )
        .await
        .unwrap();
    assert_eq!(response.noul("damage").unwrap().noul, 0.92);
    let refused: Vec<&str> = response.refusals().map(|(name, _)| name).collect();
    assert_eq!(refused, ["identity", "age"]);
    assert_eq!(response.answers.len(), 3);
    let identity = response.answers["identity"].as_refusal().unwrap();
    assert_eq!(identity.raw["name"], "identity");
    assert_eq!(
        serde_json::to_string(&response.answers["identity"]).unwrap(),
        r#"{"type":"refusal"}"#
    );
    assert_eq!(
        response.noul("identity").unwrap_err().to_string(),
        r#"The model refused to answer "identity", so there is no noul answer."#
    );
    assert_eq!(
        response.noul("missing").unwrap_err().to_string(),
        r#"No noul answer named "missing"."#
    );
}

/// The only test that touches the environment, so no other test races it.
#[test]
fn from_env_selects_the_provider() {
    let vars = [
        "TYPESAFE_PROVIDER",
        "TYPESAFE_API_KEY",
        "TYPESAFE_BASE_URL",
        "TYPESAFE_DEFAULT_MODEL",
        "CLOUDFLARE_API_TOKEN",
        "CLOUDFLARE_ACCOUNT_ID",
        "OPENAI_API_KEY",
    ];
    let set = |pairs: &[(&str, &str)]| {
        for var in vars {
            unsafe { std::env::remove_var(var) };
        }
        for (name, value) in pairs {
            unsafe { std::env::set_var(name, value) };
        }
    };
    let debug = |result: Result<Client, Error>| match result {
        Ok(client) => format!("{client:?}"),
        Err(error) => format!("error: {error}"),
    };
    let cases: [EnvCase; 8] = [
        (
            &[("TYPESAFE_API_KEY", "ts-key")],
            &[
                "provider: TypeSafe",
                r#"base_url: "https://api.typesafe.ai""#,
                r#"default_model: "jev-latest""#,
            ],
        ),
        (
            &[
                ("TYPESAFE_PROVIDER", " clef "),
                ("CLOUDFLARE_API_TOKEN", "cf-token"),
                ("CLOUDFLARE_ACCOUNT_ID", "acc123"),
                ("TYPESAFE_API_KEY", "ignored"),
                ("TYPESAFE_BASE_URL", "http://ignored.example"),
                ("TYPESAFE_DEFAULT_MODEL", "ignored-model"),
            ],
            &[
                r#"provider: Clef { account_id: "acc123" }"#,
                r#"base_url: "https://api.cloudflare.com/client/v4""#,
                r#"default_model: "clef""#,
                "compression: None",
            ],
        ),
        (
            &[
                ("TYPESAFE_PROVIDER", "OpenAI"),
                ("OPENAI_API_KEY", "sk-test"),
            ],
            &[
                "provider: OpenAiDecisions",
                r#"base_url: "https://api.openai.com""#,
                r#"default_model: "gpt-6-luna""#,
            ],
        ),
        (
            &[
                ("TYPESAFE_PROVIDER", "clef"),
                ("CLOUDFLARE_API_TOKEN", "cf-token"),
            ],
            &[
                "error: TYPESAFE_PROVIDER=clef needs the Cloudflare account ID. Set the CLOUDFLARE_ACCOUNT_ID environment variable.",
            ],
        ),
        (
            &[
                ("TYPESAFE_PROVIDER", "clef"),
                ("CLOUDFLARE_ACCOUNT_ID", "acc123"),
                ("TYPESAFE_API_KEY", "ts-key"),
            ],
            &[
                "error: No API key was provided. Pass api_key or set the CLOUDFLARE_API_TOKEN environment variable.",
            ],
        ),
        (
            &[
                ("TYPESAFE_PROVIDER", "clef"),
                ("CLOUDFLARE_ACCOUNT_ID", "acc/123"),
                ("CLOUDFLARE_API_TOKEN", "cf-token"),
            ],
            &[
                "error: The Cloudflare account ID must contain only ASCII letters, digits, '-', and '_'.",
            ],
        ),
        (
            &[
                ("TYPESAFE_PROVIDER", "openai"),
                ("OPENAI_API_KEY", "sk test"),
            ],
            &["error: API key must contain only printable ASCII characters without whitespace."],
        ),
        (
            &[
                ("TYPESAFE_PROVIDER", "anthropic"),
                ("TYPESAFE_API_KEY", "ts-key"),
            ],
            &[
                r#"error: Unknown TYPESAFE_PROVIDER value "anthropic". Use typesafe, clef, or openai."#,
            ],
        ),
    ];
    for (pairs, expected) in cases {
        set(pairs);
        let shown = debug(Client::from_env());
        for part in expected {
            assert!(shown.contains(part), "{pairs:?}: {shown} lacks {part}");
        }
        assert!(
            !shown.contains("-key") && !shown.contains("cf-token") && !shown.contains("sk-test"),
            "{shown}"
        );
    }
    set(&[]);
}

#[cfg(feature = "blocking")]
#[test]
fn blocking_client_takes_a_provider() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(CLEF_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "errors": [],
                "result": {"model": "clef", "answers": {"q": {"type": "noul", "noul": 0.4}}}
            })))
            .expect(1)
            .mount(&server)
            .await;
        server
    });
    let client = typesafe_sdk::blocking::Client::builder()
        .api_key("test-key")
        .provider(clef())
        .base_url(server.uri())
        .build()
        .unwrap();
    let response = client
        .system_one("x", [("q", Question::noul("?"))])
        .unwrap();
    assert_eq!(response.noul("q").unwrap().noul, 0.4);
    runtime.block_on(server.verify());
}
