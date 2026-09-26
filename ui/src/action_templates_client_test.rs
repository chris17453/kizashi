use super::*;

#[test]
fn client_error_keeps_http_status_visible() {
    let error = ActionTemplatesClientError::Rejected(409);
    assert_eq!(error.to_string(), "config admin service rejected the request: HTTP 409");
}
