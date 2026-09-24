//! Client overhead against an in-process wiremock server.
//!
//! Retries are off. The mock body is tiny, so the sample is serialize, HTTP, and
//! deserialize, not model time. The `reqwest` client is built once and reused.
//!
//! ```bash
//! cargo bench --bench client_overhead
//! ```

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use serde_json::json;
use typesafe_sdk::{Client, Question, RetryPolicy};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_overhead(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");

    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": "local-model",
                "usage": {"input_tokens": 12, "output_tokens": 3},
                "answers": {"billing": {"type": "noul", "noul": 0.98}}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "models": [{
                    "name": "local-model",
                    "description": "local fixture",
                    "release_date": "2026-01-01"
                }]
            })))
            .mount(&server)
            .await;
    });

    let base = server.uri();
    let client = Box::leak(Box::new(
        Client::builder()
            .api_key("bench-key")
            .base_url(&base)
            .retry(RetryPolicy::disabled())
            .timeout(Duration::from_secs(5))
            .build()
            .expect("client"),
    ));

    let mut group = c.benchmark_group("mock_http");
    group.sample_size(40);
    group.measurement_time(Duration::from_secs(5));
    group.bench_function("system_one", |b| {
        b.to_async(&rt).iter(|| async {
            let response = client
                .system_one(
                    json!({"document": "I was charged twice."}),
                    [("billing", Question::noul("Is this ticket about billing?"))],
                )
                .await
                .expect("system_one");
            black_box(response.noul("billing").expect("noul").noul);
        });
    });
    group.bench_function("models", |b| {
        b.to_async(&rt).iter(|| async {
            let response = client.models().await.expect("models");
            black_box(response.models.len());
        });
    });
    group.finish();

    c.bench_function("client_build", |b| {
        b.iter(|| {
            let built = Client::builder()
                .api_key("bench-key")
                .base_url(&base)
                .retry(RetryPolicy::disabled())
                .build()
                .expect("client");
            black_box(built);
        });
    });
}

criterion_group!(benches, client_overhead);
criterion_main!(benches);
