use super::*;

#[test]
fn generate_enrollment_produces_a_usable_secret_and_qr_code() {
    let enrollment = generate_enrollment("alice").unwrap();

    assert!(!enrollment.secret_base32.is_empty());
    assert!(enrollment.provisioning_uri.starts_with("otpauth://totp/"));
    assert!(enrollment.provisioning_uri.contains("Kizashi"));
    assert!(!enrollment.qr_code_base64_png.is_empty());
}

#[test]
fn a_freshly_generated_secret_verifies_its_own_current_code() {
    let enrollment = generate_enrollment("alice").unwrap();
    let totp = totp_for_secret(&enrollment.secret_base32, "alice").unwrap();
    let code = totp.generate_current().unwrap();

    assert!(verify_code(&enrollment.secret_base32, "alice", &code));
}

#[test]
fn a_wrong_code_is_rejected() {
    let enrollment = generate_enrollment("alice").unwrap();

    assert!(!verify_code(&enrollment.secret_base32, "alice", "000000"));
}

#[test]
fn an_invalid_secret_never_verifies() {
    assert!(!verify_code("not-a-valid-base32-secret!!!", "alice", "123456"));
}

#[test]
fn two_enrollments_generate_different_secrets() {
    let a = generate_enrollment("alice").unwrap();
    let b = generate_enrollment("alice").unwrap();

    assert_ne!(a.secret_base32, b.secret_base32);
}
