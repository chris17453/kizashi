use super::*;
use uuid::Uuid;

#[test]
fn returns_a_plain_client_when_no_proxy_url_is_given() {
    let client = build_outbound_client(None, Uuid::new_v4(), "zendesk-connector");
    assert!(client.is_ok());
}

#[test]
fn returns_a_proxied_client_for_a_valid_proxy_url() {
    let client =
        build_outbound_client(Some("http://localhost:3128"), Uuid::new_v4(), "zendesk-connector");
    assert!(client.is_ok());
}

#[test]
fn returns_an_error_for_a_malformed_proxy_url() {
    let client =
        build_outbound_client(Some("not a valid url at all"), Uuid::new_v4(), "zendesk-connector");
    assert!(client.is_err());
}
