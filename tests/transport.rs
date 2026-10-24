//! A custom `Transport` carries the whole request: what wasm hosts such as
//! Spin rely on when the `reqwest` feature is off.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;
use serde_json::{Value, json};
use typesafe_sdk::transport::BoxFuture;
use typesafe_sdk::{
    Client, Compression, Error, Question, RetryPolicy, Transport, TransportRequest,
    TransportResponse,
};

#[derive(Clone, Default)]
struct Scripted {
    replies: Arc<Mutex<Vec<Result<TransportResponse, Error>>>>,
    sent: Arc<Mutex<Vec<TransportRequest>>>,
    slept: Arc<Mutex<Vec<Duration>>>,
}

impl Scripted {
    fn new(replies: Vec<Result<TransportResponse, Error>>) -> Self {
        Self {
            replies: Arc::new(Mutex::new(replies.into_iter().rev().collect())),
            ..Self::default()
        }
    }
}

impl Transport for Scripted {
    fn send(&self, request: TransportRequest) -> BoxFuture<'_, Result<TransportResponse, Error>> {
        self.sent.lock().unwrap().push(request);
        let reply = self.replies.lock().unwrap().pop().expect("no reply left");
        Box::pin(async move { reply })
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'_, ()> {
        self.slept.lock().unwrap().push(duration);
        Box::pin(async {})
    }
}

fn reply(status: u16, body: Value) -> Result<TransportResponse, Error> {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    Ok(TransportResponse {
        status,
        headers,
        body: Bytes::from(serde_json::to_vec(&body).unwrap()),
    })
}

fn answers() -> Value {
    json!({
        "model": "jev-latest",
        "usage": {"input_tokens": 5, "output_tokens": 1},
        "answers": {"cor": {"type": "noul", "noul": 0.97}}
    })
}

fn client(transport: Scripted, retry: RetryPolicy) -> Client {
    Client::builder()
        .api_key("sk-test-key")
        .base_url("https://gateway.example")
        .compression(Compression::None)
        .retry(retry)
        .transport(transport)
        .build()
        .unwrap()
}

#[tokio::test]
async fn the_transport_carries_the_request_and_the_answer() {
    let transport = Scripted::new(vec![reply(200, answers())]);
    let response = client(transport.clone(), RetryPolicy::disabled())
        .system_one("BIR Form 2303", [("cor", Question::noul("Is this a COR?"))])
        .await
        .unwrap();
    assert_eq!(response.noul("cor").unwrap().noul, 0.97);

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].method, http::Method::POST);
    assert_eq!(sent[0].url, "https://gateway.example/v1/systemone");
    assert_eq!(sent[0].headers["authorization"], "Bearer sk-test-key");
    let body: Value = serde_json::from_slice(sent[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["state"], "BIR Form 2303");
    assert_eq!(body["questions"]["cor"]["type"], "noul");
}

#[tokio::test]
async fn retries_wait_through_the_transport() {
    let transport = Scripted::new(vec![
        reply(503, json!({"error": {"message": "busy"}})),
        reply(200, answers()),
    ]);
    let retry = RetryPolicy {
        max_retries: 1,
        backoff_jitter: 0.0,
        ..RetryPolicy::default()
    };
    client(transport.clone(), retry)
        .system_one("x", [("cor", Question::noul("COR?"))])
        .await
        .unwrap();
    assert_eq!(transport.sent.lock().unwrap().len(), 2);
    assert_eq!(transport.slept.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn connection_messages_from_a_transport_are_masked() {
    let transport = Scripted::new(vec![Err(Error::Connection {
        message: "proxy refused Bearer sk-test-key".into(),
    })]);
    let error = client(transport, RetryPolicy::disabled())
        .system_one("x", [("cor", Question::noul("COR?"))])
        .await
        .unwrap_err();
    match error {
        Error::Connection { message } => {
            assert!(!message.contains("sk-test-key"), "{message}");
        }
        other => panic!("expected a connection error, got {other:?}"),
    }
}
