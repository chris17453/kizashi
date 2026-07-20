use super::*;

#[test]
fn defaults_to_insecure_when_unset() {
    assert!(!secure_from_env_value(None));
}

#[test]
fn is_secure_when_set_to_true() {
    assert!(secure_from_env_value(Some("true")));
}

#[test]
fn is_insecure_for_any_other_value() {
    assert!(!secure_from_env_value(Some("false")));
    assert!(!secure_from_env_value(Some("1")));
    assert!(!secure_from_env_value(Some("")));
}

#[test]
fn suffix_is_empty_when_insecure() {
    assert_eq!(cookie_secure_suffix(false), "");
}

#[test]
fn suffix_adds_the_secure_attribute_when_secure() {
    assert_eq!(cookie_secure_suffix(true), "; Secure");
}
