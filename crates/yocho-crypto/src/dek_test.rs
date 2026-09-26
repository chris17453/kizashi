use super::*;

#[test]
fn generated_deks_are_random() {
    assert_ne!(Dek::generate().as_bytes(), Dek::generate().as_bytes());
}

#[test]
fn debug_output_never_contains_key_material() {
    let dek = Dek::from_bytes([0xAB; 32]);
    let rendered = format!("{dek:?}");
    assert!(!rendered.to_lowercase().contains("ab, ab"), "{rendered}");
    assert!(rendered.contains("redacted"));
    let index = IndexKey::from_bytes([0xCD; 32]);
    assert!(format!("{index:?}").contains("redacted"));
}

#[test]
fn from_bytes_round_trips() {
    let dek = Dek::from_bytes([7; 32]);
    assert_eq!(dek.as_bytes(), &[7; 32]);
    let index = IndexKey::from_bytes([9; 32]);
    assert_eq!(index.as_bytes(), &[9; 32]);
}

#[test]
fn from_slice_rejects_wrong_length() {
    assert!(Dek::from_slice(&[1; 31]).is_err());
    assert!(Dek::from_slice(&[1; 33]).is_err());
    assert_eq!(Dek::from_slice(&[1; 32]).unwrap().as_bytes(), &[1; 32]);
}
