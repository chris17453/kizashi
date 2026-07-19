use super::*;

#[test]
fn parses_a_host_and_port() {
    let target = parse_connect_target("api.zendesk.com:443").unwrap();
    assert_eq!(target.host, "api.zendesk.com");
    assert_eq!(target.port, 443);
}

#[test]
fn rejects_a_target_with_no_port() {
    assert!(parse_connect_target("api.zendesk.com").is_none());
}

#[test]
fn rejects_a_target_with_a_non_numeric_port() {
    assert!(parse_connect_target("api.zendesk.com:https").is_none());
}

#[test]
fn rejects_an_empty_host() {
    assert!(parse_connect_target(":443").is_none());
}
