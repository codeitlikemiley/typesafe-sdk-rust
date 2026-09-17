//! Wiremock integration tests for each `examples/*.rs` cookbook program.

#[path = "../examples/support/fixtures.rs"]
mod fixtures;

mod support;

use support::mock_client::mock_client;
use typesafe_sdk::Question;

#[tokio::test]
async fn example_system_one() {
    let rt = mock_client(fixtures::mixed_primitives()).await;
    let response = rt
        .client
        .system_one(
            serde_json::json!({"document": "charged twice"}),
            [
                ("billing", Question::noul("billing?")),
                (
                    "tone",
                    Question::choice("tone?", [("calm", None), ("frustrated", None)]),
                ),
                (
                    "urgency",
                    Question::score("urgency?", ["wait", "week", "today"]),
                ),
            ],
        )
        .await
        .expect("system_one");
    assert_eq!(response.noul("billing").unwrap().noul, 0.98);
    assert_eq!(response.choice("tone").unwrap().choice, "frustrated");
    assert_eq!(response.score("urgency").unwrap().score, 2.2);
}

#[tokio::test]
async fn example_fan_out() {
    let rt = mock_client(fixtures::fan_out_triage()).await;
    let response = rt
        .client
        .system_one(
            "ticket",
            [
                (
                    "category",
                    Question::choice("?", [("bug_report", None), ("billing", None)]),
                ),
                ("has_reproducible_steps", Question::noul("?")),
                (
                    "frustration",
                    Question::score("?", ["calm", "frustrated", "angry"]),
                ),
            ],
        )
        .await
        .expect("system_one");
    assert_eq!(response.choice("category").unwrap().choice, "bug_report");
    assert_eq!(response.noul("has_reproducible_steps").unwrap().noul, 0.82);
    assert_eq!(response.score("frustration").unwrap().score, 1.6);
}

#[tokio::test]
async fn example_confidence_routing() {
    let rt = mock_client(fixtures::confidence_routing()).await;
    let response = rt
        .client
        .system_one(
            "msg",
            [(
                "action",
                Question::choice("?", [("approve_transfer", None), ("support", None)]),
            )],
        )
        .await
        .expect("system_one");
    let action = response.choice("action").unwrap();
    assert_eq!(action.choice, "approve_transfer");
    assert_eq!(action.confidence, 0.62);
}

#[tokio::test]
async fn example_guardrails() {
    let rt = mock_client(fixtures::guardrails()).await;
    let response = rt
        .client
        .system_one(
            "msg",
            [
                ("jailbreak", Question::noul("?")),
                (
                    "harm_severity",
                    Question::score("?", ["none", "moderate", "severe"]),
                ),
            ],
        )
        .await
        .expect("system_one");
    assert_eq!(response.noul("jailbreak").unwrap().noul, 0.91);
    assert_eq!(response.score("harm_severity").unwrap().score, 2.4);
}

#[tokio::test]
async fn example_citation_check() {
    let rt = mock_client(fixtures::citation_check()).await;
    let response = rt
        .client
        .system_one(
            serde_json::json!({"source": "policy", "claim": "same day refund"}),
            [(
                "supports_claim",
                Question::choice("?", [("yes", None), ("partially", None), ("no", None)]),
            )],
        )
        .await
        .expect("system_one");
    let answer = response.choice("supports_claim").unwrap();
    assert_eq!(answer.choice, "partially");
    assert_eq!(answer.confidence, 0.45);
}

#[tokio::test]
async fn example_structured_state() {
    let rt = mock_client(fixtures::structured_ticket()).await;
    let response = rt
        .client
        .system_one(
            serde_json::json!({"order": {"charges": [1, 2]}}),
            [
                ("duplicate_charge", Question::noul("?")),
                ("refund_eligible", Question::noul("?")),
            ],
        )
        .await
        .expect("system_one");
    assert_eq!(response.noul("duplicate_charge").unwrap().noul, 0.97);
    assert_eq!(response.noul("refund_eligible").unwrap().noul, 0.88);
}

#[tokio::test]
async fn example_composite_scoring() {
    let rt = mock_client(fixtures::composite_scores()).await;
    let response = rt
        .client
        .system_one(
            "draft",
            [
                ("clarity", Question::score("?", ["poor", "ok", "excellent"])),
                (
                    "completeness",
                    Question::score("?", ["missing", "partial", "complete"]),
                ),
                ("tone", Question::score("?", ["harsh", "neutral", "warm"])),
            ],
        )
        .await
        .expect("system_one");
    assert_eq!(response.score("clarity").unwrap().score, 2.0);
    assert_eq!(response.score("completeness").unwrap().score, 1.2);
    assert_eq!(response.score("tone").unwrap().score, 1.8);
}

#[tokio::test]
async fn example_intent_routing() {
    let rt = mock_client(fixtures::intent_routing()).await;
    let response = rt
        .client
        .system_one(
            "summarize revenue",
            [(
                "intent",
                Question::choice(
                    "?",
                    [
                        ("deterministic", None),
                        ("specialist_llm", None),
                        ("human", None),
                    ],
                ),
            )],
        )
        .await
        .expect("system_one");
    let intent = response.choice("intent").unwrap();
    assert_eq!(intent.choice, "specialist_llm");
    assert_eq!(intent.confidence, 0.79);
}

#[tokio::test]
async fn example_noul_uncertainty() {
    let rt = mock_client(fixtures::noul_uncertainty_band()).await;
    let response = rt
        .client
        .system_one("claim", [("fraud_likely", Question::noul("?"))])
        .await
        .expect("system_one");
    assert_eq!(response.noul("fraud_likely").unwrap().noul, 0.48);
}

#[cfg(feature = "blocking")]
#[test]
fn example_blocking_triage() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let rt = mock_client(fixtures::fan_out_triage()).await;
        let response = rt
            .client
            .system_one(
                "Billing double-charged my card.",
                [(
                    "category",
                    Question::choice(
                        "?",
                        [
                            ("bug_report", None),
                            ("billing", None),
                            ("feature_request", None),
                        ],
                    ),
                )],
            )
            .await
            .expect("system_one");
        assert_eq!(response.choice("category").unwrap().choice, "bug_report");
    });
}
