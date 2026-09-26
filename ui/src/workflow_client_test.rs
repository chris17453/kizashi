use super::*;
#[test]
fn workflow_client_has_a_dedicated_runtime_boundary() {
    let _client =
        HttpWorkflowClient::new(reqwest::Client::new(), "http://pipeline-runtime".to_string());
}
