use super::*;

#[test]
fn projection_client_is_a_separate_reconciliation_boundary() {
    let _client = HttpProjectionClient::new(
        reqwest::Client::new(),
        "http://ontology".to_string(),
        "secret".to_string(),
    );
}
