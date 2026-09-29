//! Wire types in both directions: what a server deserializes and serializes must
//! be the bytes the client sends and the answers it decodes.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use typesafe_sdk::wire::{
    Answer, ChoiceAnswer, JsonContent, ListModelsResponse, ModelMetadata, NoulAnswer, NoulCriteria,
    Question, ScoreAnswer, SystemOneRequest, SystemOneResponse, Usage,
};
use typesafe_sdk::{ApiErrorKind, Client, RetryPolicy, SystemOneOpts};
use wiremock::matchers::{any, method, path};
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
    let models: ListModelsResponse = round_trip(MODELS);
    assert_eq!(
        models,
        ListModelsResponse::new([
            ModelMetadata::new("jev-latest", "Fast model", "2026-08-01"),
            ModelMetadata::new("jev-large", "Careful model", "2026-09-01"),
        ])
    );
    assert!(models.request_id().is_err());
    assert!(models.raw_body().is_empty());
    round_trip::<ListModelsResponse>(r#"{"models":[]}"#);
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
            json!({"type": "score", "instructions": "?"}),
            "requires \"criteria\"",
        ),
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
    let number_state = serde_json::from_value::<SystemOneRequest>(
        json!({"state": 5, "questions": {"q": {"type": "noul"}}}),
    );
    assert!(number_state.is_err());
    let number_model = serde_json::from_value::<SystemOneRequest>(
        json!({"state": "x", "model": 5, "questions": {"q": {"type": "noul"}}}),
    );
    assert!(number_model.is_err());
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

/// A question is typed only when the typed variant writes the same bytes back.
/// The same keys and values in another order stay `Raw`, which keeps the order.
#[test]
fn reordered_questions_stay_raw() {
    for json in [
        r#"{"instructions":"x","type":"noul"}"#,
        r#"{"type":"noul","criteria":{"false":"no","true":"yes"}}"#,
        r#"{"type":"choice","criteria":{"a":null},"instructions":"x"}"#,
        r#"{"criteria":["low","high"],"type":"score"}"#,
    ] {
        let question: Question = round_trip(json);
        assert!(matches!(question, Question::Raw(_)), "{json}");
    }

    assert_eq!(
        round_trip::<Question>(r#"{"type":"noul","instructions":"x"}"#),
        Question::noul("x")
    );
    assert_eq!(
        round_trip::<Question>(r#"{"type":"noul","criteria":{"true":"yes","false":"no"}}"#),
        Question::noul_bare().with_noul_criteria(NoulCriteria::new().yes("yes").no("no"))
    );
}

/// Score maps keep the order they arrive in, rather than sorting by index.
#[test]
fn score_maps_keep_wire_order() {
    let score: ScoreAnswer = round_trip(concat!(
        r#"{"score":2.0,"confidence":0.7,"legend":{"0":"low","10":"high","2":"mid"},"#,
        r#""probabilities":{"10":0.2,"0":0.1,"2":0.7}}"#,
    ));
    assert_eq!(score.legend.keys().copied().collect::<Vec<_>>(), [0, 10, 2]);
    assert_eq!(
        score.probabilities.keys().copied().collect::<Vec<_>>(),
        [10, 0, 2]
    );
    assert_eq!(score.legend[&10], JsonContent::from("high"));
    assert_eq!(score.probabilities[&2], 0.7);

    // Keys are `u32`: a non-canonical key reads as its index and writes canonically.
    let score: ScoreAnswer = serde_json::from_str(
        r#"{"score":1.0,"confidence":1.0,"legend":{"01":"one"},"probabilities":{"01":1.0}}"#,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_string(&score).unwrap(),
        r#"{"score":1.0,"confidence":1.0,"legend":{"1":"one"},"probabilities":{"1":1.0}}"#
    );
}

/// Response types hold what the client decodes and nothing else. Each pair is
/// an input and what serializing its decoded value writes.
#[test]
fn response_round_trips_keep_only_what_the_client_decodes() {
    let cases = [
        // Unknown keys go at every level, and so does a `null` count.
        (
            concat!(
                r#"{"model":"m","trace":"t","usage":{"input_tokens":null,"output_tokens":3,"#,
                r#""cached_tokens":1},"answers":{"n":{"type":"noul","noul":0.5,"why":"?"}}}"#,
            ),
            r#"{"model":"m","usage":{"output_tokens":3},"answers":{"n":{"type":"noul","noul":0.5}}}"#,
        ),
        // A missing `usage` or `answers` is written as `{}`.
        (
            r#"{"model":"m"}"#,
            r#"{"model":"m","usage":{},"answers":{}}"#,
        ),
        // Fields are written in declaration order.
        (
            r#"{"answers":{},"usage":{"output_tokens":1,"input_tokens":2},"model":"m"}"#,
            r#"{"model":"m","usage":{"input_tokens":2,"output_tokens":1},"answers":{}}"#,
        ),
        // An unknown answer type goes, `f64` fields write `1` as `1.0`, and score
        // keys are `u32`.
        (
            concat!(
                r#"{"model":"m","answers":{"x":{"type":"aurora"},"s":{"confidence":1,"#,
                r#""type":"score","score":2,"legend":{"02":"hi"},"probabilities":{"02":1}}}}"#,
            ),
            concat!(
                r#"{"model":"m","usage":{},"answers":{"s":{"type":"score","score":2.0,"#,
                r#""confidence":1.0,"legend":{"2":"hi"},"probabilities":{"2":1.0}}}}"#,
            ),
        ),
        // A count above `i64::MAX` wraps, as it always has in the client.
        (
            r#"{"model":"m","usage":{"input_tokens":18446744073709551615}}"#,
            r#"{"model":"m","usage":{"input_tokens":-1},"answers":{}}"#,
        ),
    ];
    for (input, output) in cases {
        let response: SystemOneResponse = serde_json::from_str(input).unwrap();
        assert_eq!(serde_json::to_string(&response).unwrap(), output, "{input}");
    }

    let listing: ListModelsResponse = serde_json::from_str(concat!(
        r#"{"models":[{"release_date":"r","name":"a","description":"d","context_window":8}],"#,
        r#""next":null}"#,
    ))
    .unwrap();
    assert_eq!(
        serde_json::to_string(&listing).unwrap(),
        r#"{"models":[{"name":"a","description":"d","release_date":"r"}]}"#
    );
}

/// A request writes `state`, `model`, and `questions` first and leaves out a
/// `null` model. Other keys keep their bytes and relative order.
#[test]
fn request_round_trips_put_the_known_keys_first() {
    let request: SystemOneRequest = serde_json::from_str(
        r#"{"top_k":3,"questions":{"q":{"type":"noul"}},"model":null,"state":"x","seed":[1]}"#,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_string(&request).unwrap(),
        r#"{"state":"x","questions":{"q":{"type":"noul"}},"top_k":3,"seed":[1]}"#
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
                    [(2, "today"), (0, "can wait"), (1, "this week")],
                    [(2, 0.8), (0, 0.05), (1, 0.15)],
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
    let urgency = decoded.score("urgency").unwrap();
    assert_eq!(urgency.score, 2.2);
    assert_eq!(
        urgency.legend.keys().copied().collect::<Vec<_>>(),
        [2, 0, 1]
    );
    assert_eq!(
        urgency.probabilities.keys().copied().collect::<Vec<_>>(),
        [2, 0, 1]
    );
}

#[tokio::test]
async fn server_built_models_decode_to_the_same_entries() {
    let built =
        ListModelsResponse::new([ModelMetadata::new("local-model", "fixture", "2026-01-01")]);
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

/// Response bodies that probe the client's leniency: absent and `null` fields,
/// repeated and unknown keys, unknown answer types, and a bad value at each level.
const RESPONSE_EDGES: &[&str] = &[
    r#"{"model":"m"}"#,
    r#"{"model":"a","model":"b","extra":{"kept":false}}"#,
    r#"{"model":"m","usage":null}"#,
    r#"{"model":"m","usage":[1,2]}"#,
    r#"{"model":7}"#,
    r#"["m"]"#,
    r#"{"model":"m","answers":[]}"#,
    r#"{"model":"m","answers":{"x":{"type":"aurora"},"n":{"type":"noul","noul":0.5}}}"#,
    r#"{"model":"m","answers":{"n":"noul"}}"#,
    r#"{"model":"m","answers":{"n":{"noul":0.5}}}"#,
];

/// `usage` objects, each read alone and inside a response.
const USAGE_EDGES: &[&str] = &[
    r#"{}"#,
    r#"{"input_tokens":null,"output_tokens":3,"cached_tokens":9}"#,
    r#"{"input_tokens":18446744073709551615}"#,
    r#"{"input_tokens":9223372036854775807,"output_tokens":-4}"#,
    r#"{"input_tokens":1,"input_tokens":2}"#,
    r#"{"input_tokens":1.5}"#,
    r#"{"output_tokens":"3"}"#,
];

/// Answer objects, each read alone and inside a response.
const ANSWER_EDGES: &[&str] = &[
    r#"{"type":"noul","noul":1,"why":"?"}"#,
    r#"{"type":"noul","noul":0.25,"noul":0.75}"#,
    r#"{"type":"noul","noul":null}"#,
    r#"{"type":"choice","choice":"a","confidence":1,"probabilities":{"a":1,"b":0}}"#,
    r#"{"type":"choice","choice":"a","confidence":1,"probabilities":{"a":"high"}}"#,
    r#"{"type":"choice","choice":1,"confidence":1,"probabilities":{}}"#,
    r#"{"type":"score","score":1,"confidence":1,"legend":{"01":"low","1":"high"},"probabilities":{"1":1}}"#,
    r#"{"type":"score","score":1,"confidence":1,"legend":{"x":"low"},"probabilities":{}}"#,
    r#"{"type":"score","score":1,"confidence":1,"legend":{"0":null},"probabilities":{}}"#,
    r#"{"type":"score","score":1,"confidence":1,"legend":{},"probabilities":{"0":"1"}}"#,
];

/// Models listings, and entries each read alone and inside a listing.
const MODELS_EDGES: &[&str] = &[
    r#"{"models":[]}"#,
    r#"{"models":[{"name":"a","description":"d","release_date":"r"},7]}"#,
    r#"{"models":{}}"#,
    r#"{"models":null}"#,
    r#"{}"#,
    r#"[]"#,
];
const MODEL_EDGES: &[&str] = &[
    r#"{"name":"a","description":"d","release_date":"r","context_window":128000}"#,
    r#"{"name":"a","name":"b","description":"d","release_date":"r"}"#,
    r#"{"name":"a","description":"d"}"#,
    r#"{"name":null,"description":"d","release_date":"r"}"#,
    r#"{"name":"a","description":["d"],"release_date":"r"}"#,
];

/// A mock upstream whose one response body is swapped per case, and a client for it.
struct Upstream {
    server: MockServer,
    client: Client,
}

impl Upstream {
    async fn start() -> Self {
        let server = MockServer::start().await;
        let client = Client::builder()
            .api_key("test-key")
            .base_url(server.uri())
            .retry(RetryPolicy::disabled())
            .build()
            .unwrap();
        Self { server, client }
    }

    async fn serve(&self, body: &str) {
        self.server.reset().await;
        Mock::given(any())
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(body.to_string(), "application/json"),
            )
            .mount(&self.server)
            .await;
    }

    /// The client's decode of `body` as a System One response, or the path it rejects.
    async fn system_one(&self, body: &str) -> Result<SystemOneResponse, String> {
        self.serve(body).await;
        self.client
            .system_one("x", [("q", Question::noul("?"))])
            .await
            .map_err(|error| rejected_path(&error))
    }

    /// The client's decode of `body` as a models listing, or the path it rejects.
    async fn models(&self, body: &str) -> Result<ListModelsResponse, String> {
        self.serve(body).await;
        self.client
            .models()
            .await
            .map_err(|error| rejected_path(&error))
    }
}

fn rejected_path(error: &typesafe_sdk::Error) -> String {
    let api = error.api().unwrap();
    assert_eq!(api.kind, ApiErrorKind::ResponseValidation, "{error}");
    api.field_path.clone().unwrap()
}

/// The path in a serde error that reads `Invalid {what} data at '{path}'.`, or
/// the whole message when it reads otherwise.
fn serde_path(error: serde_json::Error, what: &str) -> String {
    let message = error.to_string();
    let path = message
        .strip_prefix(&format!("Invalid {what} data at '"))
        .and_then(|rest| rest.split_once("'."))
        .map(|(path, _)| path.to_string());
    path.unwrap_or(message)
}

fn without_prefix(path: String, prefix: &str) -> String {
    path.strip_prefix(prefix)
        .map(str::to_string)
        .unwrap_or(path)
}

/// Reads an answer object with the struct for its `type`, not with `Answer`.
fn inner_answer(json: &str) -> Result<Answer, String> {
    let value: Value = serde_json::from_str(json).unwrap();
    match value["type"].as_str() {
        Some("noul") => serde_json::from_str(json).map(Answer::Noul),
        Some("choice") => serde_json::from_str(json).map(Answer::Choice),
        Some("score") => serde_json::from_str(json).map(Answer::Score),
        _ => panic!("{json} has no known type"),
    }
    .map_err(|error| serde_path(error, "answer"))
}

/// Every serde impl a server decodes a response with runs the client's decoder:
/// the same input gives the same value, or an error naming the same field.
#[tokio::test]
async fn serde_reads_responses_exactly_as_the_client_does() {
    let upstream = Upstream::start().await;
    let mut bodies: Vec<String> = RESPONSE_EDGES.iter().map(|body| body.to_string()).collect();
    bodies.extend(
        USAGE_EDGES
            .iter()
            .map(|usage| format!(r#"{{"model":"m","usage":{usage}}}"#)),
    );
    bodies.extend(
        ANSWER_EDGES
            .iter()
            .map(|answer| format!(r#"{{"model":"m","answers":{{"a":{answer}}}}}"#)),
    );
    for body in &bodies {
        let by_client = upstream
            .system_one(body)
            .await
            .map(|response| (response.model, response.usage, response.answers));
        let by_serde = serde_json::from_str::<SystemOneResponse>(body)
            .map(|response| (response.model, response.usage, response.answers))
            .map_err(|error| serde_path(error, "response"));
        assert_eq!(by_serde, by_client, "{body}");
    }

    for usage in USAGE_EDGES {
        let by_client = upstream
            .system_one(&format!(r#"{{"model":"m","usage":{usage}}}"#))
            .await
            .map(|response| response.usage)
            .map_err(|path| without_prefix(path, "usage."));
        let by_serde =
            serde_json::from_str::<Usage>(usage).map_err(|error| serde_path(error, "usage"));
        assert_eq!(by_serde, by_client, "{usage}");
    }

    for answer in ANSWER_EDGES {
        let by_client = upstream
            .system_one(&format!(r#"{{"model":"m","answers":{{"a":{answer}}}}}"#))
            .await
            .map(|mut response| response.answers.swap_remove("a").unwrap())
            .map_err(|path| without_prefix(path, "answers.a."));
        let by_serde =
            serde_json::from_str::<Answer>(answer).map_err(|error| serde_path(error, "answer"));
        assert_eq!(by_serde, by_client, "{answer}");
        assert_eq!(inner_answer(answer), by_client, "{answer}");
    }

    // The leniency the comparisons above hold serde to.
    let usage = |json: &str| serde_json::from_str::<Usage>(json).unwrap();
    assert_eq!(
        usage(r#"{"input_tokens":18446744073709551615}"#),
        Usage::new(Some(-1), None)
    );
    assert_eq!(
        usage(r#"{"input_tokens":1,"input_tokens":2}"#),
        Usage::new(Some(2), None)
    );
}

#[tokio::test]
async fn serde_reads_models_exactly_as_the_client_does() {
    let upstream = Upstream::start().await;
    let mut bodies: Vec<String> = MODELS_EDGES.iter().map(|body| body.to_string()).collect();
    bodies.extend(
        MODEL_EDGES
            .iter()
            .map(|entry| format!(r#"{{"models":[{entry}]}}"#)),
    );
    for body in &bodies {
        let by_client = upstream.models(body).await.map(|listing| listing.models);
        let by_serde = serde_json::from_str::<ListModelsResponse>(body)
            .map(|listing| listing.models)
            .map_err(|error| serde_path(error, "response"));
        assert_eq!(by_serde, by_client, "{body}");
    }

    for entry in MODEL_EDGES {
        let by_client = upstream
            .models(&format!(r#"{{"models":[{entry}]}}"#))
            .await
            .map(|mut listing| listing.models.remove(0))
            .map_err(|path| without_prefix(path, "models[0]."));
        let by_serde = serde_json::from_str::<ModelMetadata>(entry)
            .map_err(|error| serde_path(error, "model"));
        assert_eq!(by_serde, by_client, "{entry}");
    }
}
