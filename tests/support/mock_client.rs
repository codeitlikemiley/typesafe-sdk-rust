use serde_json::Value;
use typesafe_sdk::{Client, RetryPolicy};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub struct MockRuntime {
    pub client: Client,
    _server: MockServer,
}

pub async fn mock_client(body: Value) -> MockRuntime {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer mock-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    let client = Client::builder()
        .api_key("mock-key")
        .base_url(server.uri())
        .retry(RetryPolicy::disabled())
        .build()
        .expect("client");
    MockRuntime {
        client,
        _server: server,
    }
}
