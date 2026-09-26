#[test]
fn service_account_page_exposes_one_time_token_and_revoke_controls() {
    let source = include_str!("../templates/service_accounts.html");
    assert!(source.contains("Copy this token now"));
    assert!(source.contains("/service-accounts/{{ account.id }}/revoke"));
}
