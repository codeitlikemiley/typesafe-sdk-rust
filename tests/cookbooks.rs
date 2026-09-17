//! Cookbook scenarios against wiremock fixtures (same bodies as `examples/support/fixtures.rs`).

#[path = "../examples/support/fixtures.rs"]
mod fixtures;

mod support;

use support::mock_client::mock_client;
use typesafe_sdk::Question;

#[tokio::test]
async fn fan_out_mock() {
    let rt = mock_client(fixtures::fan_out_triage()).await;
    let response = rt
        .client
        .system_one(
            "fixture state",
            [
                (
                    "category",
                    Question::choice("?", [("bug_report", None), ("billing", None)]),
                ),
                ("has_reproducible_steps", Question::noul("?")),
            ],
        )
        .await
        .expect("system_one");
    assert_eq!(response.choice("category").unwrap().choice, "bug_report");
    assert_eq!(response.noul("has_reproducible_steps").unwrap().noul, 0.82);
}

#[tokio::test]
async fn confidence_routing_mock() {
    let rt = mock_client(fixtures::confidence_routing()).await;
    let response = rt
        .client
        .system_one(
            "fixture state",
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
async fn guardrails_mock() {
    let rt = mock_client(fixtures::guardrails()).await;
    let response = rt
        .client
        .system_one(
            "fixture state",
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
async fn composite_scoring_mock() {
    let rt = mock_client(fixtures::composite_scores()).await;
    let response = rt
        .client
        .system_one(
            "fixture state",
            [
                ("clarity", Question::score("?", ["poor", "ok", "excellent"])),
                (
                    "completeness",
                    Question::score("?", ["missing", "partial", "complete"]),
                ),
            ],
        )
        .await
        .expect("system_one");
    assert_eq!(response.score("clarity").unwrap().score, 2.0);
    assert_eq!(response.score("completeness").unwrap().score, 1.2);
}

#[tokio::test]
async fn noul_uncertainty_mock() {
    let rt = mock_client(fixtures::noul_uncertainty_band()).await;
    let response = rt
        .client
        .system_one("fixture state", [("fraud_likely", Question::noul("?"))])
        .await
        .expect("system_one");
    assert_eq!(response.noul("fraud_likely").unwrap().noul, 0.48);
}
