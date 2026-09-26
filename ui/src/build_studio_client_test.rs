use super::*;

#[test]
fn build_studio_client_is_tenant_scoped() {
    let _client = HttpBuildStudioClient::new(
        reqwest::Client::new(),
        "http://config-admin".to_string(),
        "secret".to_string(),
    );
}
