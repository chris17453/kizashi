#[path = "mfa_test.rs"]
#[cfg(test)]
mod mfa_test;

use totp_rs::{Algorithm, Secret, TOTP};

const ISSUER: &str = "Kizashi";

fn totp_for_secret(secret_base32: &str, account_name: &str) -> Result<TOTP, String> {
    let secret = Secret::Encoded(secret_base32.to_string())
        .to_bytes()
        .map_err(|e| format!("invalid TOTP secret: {e:?}"))?;
    TOTP::new(Algorithm::SHA1, 6, 1, 30, secret, Some(ISSUER.to_string()), account_name.to_string())
        .map_err(|e| format!("failed to build TOTP: {e}"))
}

/// A freshly generated, unconfirmed TOTP secret plus everything an enrollment page needs to
/// render a QR code and a manual-entry fallback -- returned once at enroll time and never again
/// (`LocalUser.mfa_secret` is never serialized, see its doc comment).
pub struct MfaEnrollment {
    pub secret_base32: String,
    pub provisioning_uri: String,
    pub qr_code_base64_png: String,
}

/// Generates a new random TOTP secret for `account_name` (the username, shown inside
/// authenticator apps alongside the `Kizashi` issuer so a user with multiple tenants/workspaces
/// can tell enrollments apart) and everything needed to enroll it -- does not touch storage or
/// mark MFA enabled; the caller must persist `secret_base32` as pending and only flip
/// `mfa_enabled` after a `verify_code` round-trip confirms the user's authenticator app actually
/// has it (ADR-0051): an unconfirmed secret typo'd into an authenticator app must never be able
/// to lock a real login out.
pub fn generate_enrollment(account_name: &str) -> Result<MfaEnrollment, String> {
    let secret_base32 = Secret::generate_secret().to_encoded().to_string();
    let totp = totp_for_secret(&secret_base32, account_name)?;
    let provisioning_uri = totp.get_url();
    let qr_code_base64_png =
        totp.get_qr_base64().map_err(|e| format!("failed to render QR code: {e}"))?;
    Ok(MfaEnrollment { secret_base32, provisioning_uri, qr_code_base64_png })
}

/// Verifies a 6-digit code against a stored secret, allowing the current and immediately
/// adjacent 30-second windows (`TOTP::new`'s `skew = 1` above) so a slow typist or minor client
/// clock drift doesn't spuriously fail -- the standard tolerance recommended by RFC 6238 for
/// interactive use.
pub fn verify_code(secret_base32: &str, account_name: &str, code: &str) -> bool {
    match totp_for_secret(secret_base32, account_name) {
        Ok(totp) => totp.check_current(code).unwrap_or(false),
        Err(_) => false,
    }
}
