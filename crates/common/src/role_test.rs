use super::*;
use std::str::FromStr;

#[test]
fn roles_are_ordered_viewer_operator_admin() {
    assert!(Role::Viewer < Role::Operator);
    assert!(Role::Operator < Role::Admin);
}

#[test]
fn at_least_operator_excludes_viewer_and_includes_operator_and_admin() {
    assert!(!Role::Viewer.at_least(Role::Operator));
    assert!(Role::Operator.at_least(Role::Operator));
    assert!(Role::Admin.at_least(Role::Operator));
}

#[test]
fn round_trips_through_display_and_from_str() {
    for role in [Role::Viewer, Role::Operator, Role::Admin] {
        assert_eq!(Role::from_str(&role.to_string()).unwrap(), role);
    }
}

#[test]
fn from_str_rejects_unknown_values() {
    assert!(Role::from_str("superuser").is_err());
}

#[test]
fn serializes_as_snake_case() {
    assert_eq!(serde_json::to_string(&Role::Operator).unwrap(), "\"operator\"");
}
