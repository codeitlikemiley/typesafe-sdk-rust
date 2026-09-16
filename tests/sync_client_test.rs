use typesafe_sdk::TypeSafeClient;

#[test]
fn blocking_client_constructs() {
    let client = TypeSafeClient::new(
        Some("test-key".into()),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    drop(client);
}
