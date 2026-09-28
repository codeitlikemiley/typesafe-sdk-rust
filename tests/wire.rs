//! Wire types in both directions: what a server deserializes and serializes must
//! be the bytes the client sends and the answers it decodes.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use typesafe_sdk::wire::{
    Answer, ChoiceAnswer, JsonContent, ModelMetadata, ModelsResponse, NoulAnswer, NoulCriteria,
    Question, ScoreAnswer, SystemOneRequest, SystemOneResponse, Usage,
};
use typesafe_sdk::{Client, RetryPolicy, SystemOneOpts};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Deserializes `json`, serializes the result, and requires the same bytes back.
fn round_trip<T: Serialize + DeserializeOwned>(json: &str) -> T {
    let value: T = serde_json::from_str(json).unwrap();
    assert_eq!(serde_json::to_string(&value).unwrap(), json);
    value
}

fn object(value: Value) -> serde_json::Map<String, Value> {
    value.as_object().unwrap().clone()
}

const REQUEST: &str = concat!(
    r#"{"state":{"document":"Hello 🌍","charges":[49,49.5]},"model":"jev-latest","questions":{"#,
    r#""spam":{"type":"noul","instructions":"Spam?"},"#,
    r#""bare":{"type":"noul"},"#,
    r#""duplicate":{"type":"noul","instructions":{"question":"Duplicate?"},"#,
    r#""criteria":{"true":"charged twice","false":["one charge"]}},"#,
    r#""tone":{"type":"choice","instructions":"Tone?","#,
    r#""criteria":{"friendly":null,"hostile":"Rude or \"angry\""}},"#,
    r#""quality":{"type":"score","instructions":"Quality?","#,
    r#""criteria":["bad",{"label":"great","min":2}]},"#,
    r#""ranked":{"type":"ranking","instructions":"Order","items":["b","a"]},"#,
    r#""weighted":{"type":"noul","instructions":"Spam?","weight":3}},"#,
    r#""beam_width":4,"nullable":null,"options":{"z":1,"a":[0.25,"x"]}}"#,
);

const RESPONSE: &str = concat!(
    r#"{"model":"jev-latest","usage":{"input_tokens":12,"output_tokens":3},"answers":{"#,
    r#""spam":{"type":"noul","noul":0.98},"#,
    r#""tone":{"type":"choice","choice":"friendly","confidence":0.9,"#,
    r#""probabilities":{"friendly":0.9,"hostile":0.1}},"#,
    r#""quality":{"type":"score","score":1.7,"confidence":0.8,"#,
    r#""legend":{"0":"bad","1":"ok","2":{"summary":"great","examples":["fast"]}},"#,
    r#""probabilities":{"0":0.1,"1":0.1,"2":0.8}}}}"#,
);

const MODELS: &str = concat!(
    r#"{"models":[{"name":"jev-latest","description":"Fast model","release_date":"2026-08-01"},"#,
    r#"{"name":"jev-large","description":"Careful model","release_date":"2026-09-01"}]}"#,
);

#[test]
fn request_round_trips_byte_for_byte() {
    let request: SystemOneRequest = round_trip(REQUEST);

    assert_eq!(request.model.as_deref(), Some("jev-latest"));
    assert_eq!(request.questions["spam"], Question::noul("Spam?"));
    assert_eq!(request.questions["bare"], Question::noul_bare());
    assert_eq!(
        request.questions["duplicate"],
        Question::noul(JsonContent::from_value(json!({"question": "Duplicate?"})).unwrap())
            .with_noul_criteria(
                NoulCriteria::new()
                    .yes("charged twice")
                    .no(JsonContent::from_value(json!(["one charge"])).unwrap()),
            )
    );
    assert_eq!(
        request.questions["tone"],
        Question::choice(
            "Tone?",
            [
                ("friendly", None),
                ("hostile", Some("Rude or \"angry\"".into())),
            ],
        )
    );
    assert_eq!(
        request.questions["quality"],
        Question::score(
            "Quality?",
            [
                JsonContent::from("bad"),
                JsonContent::from_value(json!({"label": "great", "min": 2})).unwrap(),
            ],
        )
    );
    // Unknown type, and a known type with a key the typed variant cannot hold.
    assert!(matches!(request.questions["ranked"], Question::Raw(_)));
    assert!(matches!(request.questions["weighted"], Question::Raw(_)));
    assert_eq!(
        request.extra.keys().collect::<Vec<_>>(),
        ["beam_width", "nullable", "options"]
    );
}

#[test]
fn request_without_model_round_trips() {
    let request: SystemOneRequest =
        round_trip(r#"{"state":"text","questions":{"q":{"type":"noul"}}}"#);
    assert_eq!(request.model, None);
    assert_eq!(request.state, JsonContent::from("text"));
    let request: SystemOneRequest =
        round_trip(r#"{"state":["a",1],"questions":{"q":{"type":"noul"}}}"#);
    assert_eq!(
        request.state,
        JsonContent::from_value(json!(["a", 1])).unwrap()
    );
}

#[test]
fn response_round_trips_byte_for_byte() {
    let response: SystemOneResponse = round_trip(RESPONSE);

    assert_eq!(response.model, "jev-latest");
    assert_eq!(response.usage, Usage::new(Some(12), Some(3)));
    assert_eq!(response.noul("spam").unwrap(), &NoulAnswer::new(0.98));
    assert_eq!(
        response.choice("tone").unwrap(),
        &ChoiceAnswer::new("friendly", 0.9, [("friendly", 0.9), ("hostile", 0.1)])
    );
    assert_eq!(
        response.score("quality").unwrap(),
        &ScoreAnswer::new(
            1.7,
            0.8,
            [
                (0, JsonContent::from("bad")),
                (1, JsonContent::from("ok")),
                (
                    2,
                    JsonContent::from_value(json!({"summary": "great", "examples": ["fast"]}))
                        .unwrap(),
                ),
            ],
            [(0, 0.1), (1, 0.1), (2, 0.8)],
        )
    );
    assert!(response.request_id().is_err());
    assert!(response.raw_body().is_empty());
}

#[test]
fn every_answer_type_round_trips_alone() {
    round_trip::<Answer>(r#"{"type":"noul","noul":0.98}"#);
    round_trip::<Answer>(
        r#"{"type":"choice","choice":"a","confidence":0.6,"probabilities":{"a":0.6,"b":0.4}}"#,
    );
    round_trip::<Answer>(
        r#"{"type":"score","score":0.0,"confidence":1.0,"legend":{"0":"only"},"probabilities":{"0":1.0}}"#,
    );
    round_trip::<NoulAnswer>(r#"{"noul":0.5}"#);
    round_trip::<ChoiceAnswer>(r#"{"choice":"a","confidence":1.0,"probabilities":{"a":1.0}}"#);
    round_trip::<ScoreAnswer>(
        r#"{"score":1.5,"confidence":0.5,"legend":{"0":"low","1":"high"},"probabilities":{"0":0.5,"1":0.5}}"#,
    );
}

#[test]
fn usage_counts_are_optional() {
    round_trip::<Usage>(r#"{"input_tokens":5,"output_tokens":7}"#);
    let usage: Usage = round_trip(r#"{"input_tokens":5}"#);
    assert_eq!(usage.output_tokens, None);
    assert_eq!(round_trip::<Usage>("{}"), Usage::default());
}

#[test]
fn models_listing_round_trips_byte_for_byte() {
    let models: ModelsResponse = round_trip(MODELS);
    assert_eq!(
        models,
        ModelsResponse::new([
            ModelMetadata::new("jev-latest", "Fast model", "2026-08-01"),
            ModelMetadata::new("jev-large", "Careful model", "2026-09-01"),
        ])
    );
    round_trip::<ModelsResponse>(r#"{"models":[]}"#);
}

#[test]
fn unknown_answer_type_is_skipped_like_the_client() {
    let response: SystemOneResponse = serde_json::from_value(json!({
        "model": "test",
        "usage": {"input_tokens": 1, "output_tokens": 1},
        "answers": {
            "spam": {"type": "noul", "noul": 0.9},
            "mystery": {"type": "aurora", "value": 3}
        }
    }))
    .unwrap();
    assert_eq!(response.answers.len(), 1);
    assert_eq!(response.noul("spam").unwrap().noul, 0.9);

    let error = serde_json::from_value::<Answer>(json!({"type": "aurora"})).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unrecognized answer type \"aurora\"")
    );
}

#[test]
fn malformed_response_names_the_field() {
    let error = serde_json::from_value::<SystemOneResponse>(json!({
        "model": "test",
        "answers": {"n": {"type": "noul"}}
    }))
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Invalid response data at 'answers.n.noul'."
    );
}

#[test]
fn deserialize_rejects_what_the_client_refuses_to_send() {
    let cases = [
        (json!({"type": ""}), "nonempty string \"type\""),
        (json!({"instructions": "?"}), "nonempty string \"type\""),
        (json!({"type": "choice"}), "requires \"criteria\""),
        (
            json!({"type": "score", "criteria": []}),
            "at least one score",
        ),
    ];
    for (question, message) in cases {
        let error = serde_json::from_value::<Question>(question.clone()).unwrap_err();
        assert!(error.to_string().contains(message), "{question}: {error}");
    }
    assert!(serde_json::from_value::<Question>(json!("noul")).is_err());

    let empty = serde_json::from_value::<SystemOneRequest>(json!({"state": "x", "questions": {}}));
    assert!(
        empty
            .unwrap_err()
            .to_string()
            .contains("At least one question")
    );
    let no_state =
        serde_json::from_value::<SystemOneRequest>(json!({"questions": {"q": {"type": "noul"}}}));
    assert!(no_state.is_err());
    let null_state = serde_json::from_value::<SystemOneRequest>(
        json!({"state": null, "questions": {"q": {"type": "noul"}}}),
    );
    assert!(null_state.is_err());
}

#[test]
fn extra_keys_replace_fields_in_place() {
    let request = SystemOneRequest {
        state: "text".into(),
        model: None,
        questions: typesafe_sdk::questions([("q", Question::noul("?"))]),
        extra: object(json!({"model": "override", "state": {"replaced": true}, "top_k": 3})),
    };
    assert_eq!(
        serde_json::to_string(&request).unwrap(),
        r#"{"state":{"replaced":true},"model":"override","questions":{"q":{"type":"noul","instructions":"?"}},"top_k":3}"#
    );
}

#[test]
fn noul_criteria_uses_the_wire_keys() {
    let criteria = NoulCriteria::new().yes("charged twice");
    assert_eq!(
        serde_json::to_string(&criteria).unwrap(),
        r#"{"true":"charged twice"}"#
    );
    assert_eq!(
        serde_json::from_str::<NoulCriteria>(r#"{"true":"charged twice"}"#).unwrap(),
        criteria
    );
}

async fn serve(route: &str, verb: &str, body: String) -> (MockServer, Client) {
    let server = MockServer::start().await;
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(&server)
        .await;
    let client = Client::builder()
        .api_key("test-key")
        .base_url(server.uri())
        .retry(RetryPolicy::disabled())
        .build()
        .unwrap();
    (server, client)
}

#[tokio::test]
async fn server_built_response_decodes_to_the_same_answers() {
    let built = SystemOneResponse::new(
        "local-model",
        Usage::new(Some(40), None),
        [
            ("spam", Answer::Noul(NoulAnswer::new(0.25))),
            (
                "tone",
                Answer::Choice(ChoiceAnswer::new(
                    "calm",
                    0.7,
                    [("calm", 0.7), ("angry", 0.3)],
                )),
            ),
            (
                "urgency",
                Answer::Score(ScoreAnswer::new(
                    2.2,
                    0.85,
                    [(0, "can wait"), (1, "this week"), (2, "today")],
                    [(0, 0.05), (1, 0.15), (2, 0.8)],
                )),
            ),
        ],
    );
    let body = serde_json::to_string(&built).unwrap();
    let (_server, client) = serve("/v1/systemone", "POST", body.clone()).await;

    let decoded = client
        .system_one("ticket", [("spam", Question::noul("Spam?"))])
        .await
        .unwrap();

    assert_eq!(decoded.model, built.model);
    assert_eq!(decoded.usage, built.usage);
    assert_eq!(decoded.answers, built.answers);
    assert_eq!(decoded.raw_body(), body.as_bytes());
    assert_eq!(decoded.score("urgency").unwrap().score, 2.2);
}

#[tokio::test]
async fn server_built_models_decode_to_the_same_entries() {
    let built = ModelsResponse::new([ModelMetadata::new("local-model", "fixture", "2026-01-01")]);
    let (_server, client) =
        serve("/v1/models", "GET", serde_json::to_string(&built).unwrap()).await;
    assert_eq!(client.models().await.unwrap().models, built.models);
}

#[tokio::test]
async fn server_reads_exactly_what_the_client_sent() {
    let (server, client) = serve(
        "/v1/systemone",
        "POST",
        serde_json::to_string(&SystemOneResponse::new(
            "m",
            Usage::default(),
            Vec::<(String, Answer)>::new(),
        ))
        .unwrap(),
    )
    .await;
    let questions = typesafe_sdk::questions([
        ("spam", Question::noul("Spam?")),
        (
            "tone",
            Question::choice("Tone?", [("calm", None), ("angry", Some("Loud".into()))]),
        ),
        ("urgency", Question::score("Urgency?", ["low", "high"])),
        (
            "ranked",
            Question::raw(object(json!({"type": "ranking", "items": [1, 2]}))),
        ),
    ]);
    client
        .system_one_opts(
            json!({"document": "charged twice"}),
            questions.clone(),
            SystemOneOpts {
                model: Some("jev-large".to_string()),
                extra_body: Some(object(json!({"top_k": 3}))),
                ..SystemOneOpts::default()
            },
        )
        .await
        .unwrap();

    let sent = server.received_requests().await.unwrap().remove(0).body;
    let received: SystemOneRequest = serde_json::from_slice(&sent).unwrap();
    assert_eq!(received.questions, questions);
    assert_eq!(received.model.as_deref(), Some("jev-large"));
    assert_eq!(received.extra, object(json!({"top_k": 3})));
    assert_eq!(serde_json::to_vec(&received).unwrap(), sent);
}

/// A gateway that deserializes a request and forwards it with the client sends
/// the bytes it received.
#[tokio::test]
async fn gateway_forwards_a_request_byte_for_byte() {
    let request: SystemOneRequest = serde_json::from_str(REQUEST).unwrap();
    let (server, client) = serve("/v1/systemone", "POST", RESPONSE.to_string()).await;
    client
        .system_one_opts(
            request.state,
            request.questions,
            SystemOneOpts {
                model: request.model,
                extra_body: Some(request.extra),
                ..SystemOneOpts::default()
            },
        )
        .await
        .unwrap();
    let sent = server.received_requests().await.unwrap().remove(0).body;
    assert_eq!(String::from_utf8(sent).unwrap(), REQUEST);
}
