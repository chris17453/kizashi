use super::*;

#[test]
fn sqlx_errors_map_to_store_errors() {
    let err: CryptoError = sqlx::Error::RowNotFound.into();
    assert!(matches!(err, CryptoError::Store(ref m) if m.contains("no rows")), "{err:?}");
}

#[test]
fn subject_shredded_display_names_the_time_but_no_key_material() {
    let at = chrono::DateTime::parse_from_rfc3339("2026-09-26T12:00:00Z").unwrap().to_utc();
    let msg = CryptoError::SubjectShredded { shredded_at: at }.to_string();
    assert!(msg.contains("2026-09-26"), "{msg}");
}
