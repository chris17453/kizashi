use super::*;

fn basic_header(tenant_id: &str, connector_id: &str) -> String {
    use base64::Engine;
    let raw = format!("{tenant_id}:{connector_id}");
    format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(raw))
}

#[test]
fn parses_a_valid_basic_auth_header() {
    let header = basic_header("tenant-a", "zendesk-connector");
    let identity = parse_proxy_authorization(&header).unwrap();
    assert_eq!(identity.tenant_id, "tenant-a");
    assert_eq!(identity.connector_id, "zendesk-connector");
}

#[test]
fn rejects_a_non_basic_scheme() {
    assert!(parse_proxy_authorization("Bearer sometoken").is_none());
}

#[test]
fn rejects_invalid_base64() {
    assert!(parse_proxy_authorization("Basic not-valid-base64!!!").is_none());
}

#[test]
fn rejects_a_header_missing_the_colon_separator() {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode("no-colon-here");
    assert!(parse_proxy_authorization(&format!("Basic {encoded}")).is_none());
}

#[test]
fn tenant_or_connector_id_may_itself_contain_a_colon() {
    // connector_id is everything after the first colon, so it may contain colons itself.
    let header = basic_header("tenant-a", "conn:with:colons");
    let identity = parse_proxy_authorization(&header).unwrap();
    assert_eq!(identity.tenant_id, "tenant-a");
    assert_eq!(identity.connector_id, "conn:with:colons");
}
