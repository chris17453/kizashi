use super::*;

#[test]
fn parses_multiple_valid_entries() {
    let parsed = parse_registry(
        "ingestion-gateway=http://localhost:8081,auth-service=http://localhost:8090",
    );
    assert_eq!(
        parsed,
        vec![
            ServiceEndpoint {
                name: "ingestion-gateway".to_string(),
                url: "http://localhost:8081".to_string()
            },
            ServiceEndpoint {
                name: "auth-service".to_string(),
                url: "http://localhost:8090".to_string()
            },
        ]
    );
}

#[test]
fn trims_whitespace_around_entries() {
    let parsed = parse_registry(" a=http://x , b=http://y ");
    assert_eq!(parsed[0], ServiceEndpoint { name: "a".to_string(), url: "http://x".to_string() });
    assert_eq!(parsed[1], ServiceEndpoint { name: "b".to_string(), url: "http://y".to_string() });
}

#[test]
fn skips_malformed_entries_without_failing_the_whole_parse() {
    let parsed = parse_registry("a=http://x,not-a-valid-entry,b=http://y");
    assert_eq!(parsed.len(), 2);
}

#[test]
fn empty_input_returns_an_empty_registry() {
    assert!(parse_registry("").is_empty());
}
