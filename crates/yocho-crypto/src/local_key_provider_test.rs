use super::*;
use crate::{CryptoError, Dek, KeyProvider};
use base64::Engine;
use uuid::Uuid;

fn provider() -> LocalKeyProvider {
    LocalKeyProvider::new([42; 32])
}

#[tokio::test]
async fn wrap_then_unwrap_round_trips() {
    let p = provider();
    let tenant = Uuid::new_v4();
    let kek = p.create_tenant_kek(tenant).await.unwrap();
    let dek = Dek::generate();
    let wrapped = p.wrap_dek(tenant, &kek, &dek).await.unwrap();
    assert_ne!(&wrapped[..], &dek.as_bytes()[..]);
    let unwrapped = p.unwrap_dek(tenant, &kek, &wrapped).await.unwrap();
    assert_eq!(unwrapped.as_bytes(), dek.as_bytes());
}

#[tokio::test]
async fn create_tenant_kek_is_idempotent_and_tenant_scoped() {
    let p = provider();
    let t1 = Uuid::new_v4();
    let t2 = Uuid::new_v4();
    assert_eq!(p.create_tenant_kek(t1).await.unwrap(), p.create_tenant_kek(t1).await.unwrap());
    assert_ne!(p.create_tenant_kek(t1).await.unwrap(), p.create_tenant_kek(t2).await.unwrap());
}

#[tokio::test]
async fn wrapped_dek_of_one_tenant_cannot_be_unwrapped_as_another_tenant() {
    let p = provider();
    let t1 = Uuid::new_v4();
    let t2 = Uuid::new_v4();
    let kek1 = p.create_tenant_kek(t1).await.unwrap();
    let kek2 = p.create_tenant_kek(t2).await.unwrap();
    let wrapped = p.wrap_dek(t1, &kek1, &Dek::generate()).await.unwrap();
    assert!(matches!(p.unwrap_dek(t2, &kek1, &wrapped).await, Err(CryptoError::KekMismatch)));
    assert!(matches!(p.unwrap_dek(t2, &kek2, &wrapped).await, Err(CryptoError::UnwrapFailed)));
}

#[tokio::test]
async fn wrap_rejects_a_kek_id_belonging_to_another_tenant() {
    let p = provider();
    let other = p.create_tenant_kek(Uuid::new_v4()).await.unwrap();
    let err = p.wrap_dek(Uuid::new_v4(), &other, &Dek::generate()).await.unwrap_err();
    assert!(matches!(err, CryptoError::KekMismatch));
}

#[tokio::test]
async fn different_master_keys_cannot_unwrap_each_other() {
    let tenant = Uuid::new_v4();
    let a = LocalKeyProvider::new([1; 32]);
    let b = LocalKeyProvider::new([2; 32]);
    let kek = a.create_tenant_kek(tenant).await.unwrap();
    let wrapped = a.wrap_dek(tenant, &kek, &Dek::generate()).await.unwrap();
    assert!(matches!(b.unwrap_dek(tenant, &kek, &wrapped).await, Err(CryptoError::UnwrapFailed)));
}

#[tokio::test]
async fn malformed_wrapped_bytes_are_rejected_without_panicking() {
    let p = provider();
    let tenant = Uuid::new_v4();
    let kek = p.create_tenant_kek(tenant).await.unwrap();
    for bad in [vec![], vec![1u8], vec![9u8; 5], vec![WRAP_VERSION_V1; 20]] {
        assert!(p.unwrap_dek(tenant, &kek, &bad).await.is_err());
    }
}

#[tokio::test]
async fn index_keys_are_per_tenant_stable_and_distinct_from_kek_material() {
    let p = provider();
    let t1 = Uuid::new_v4();
    let t2 = Uuid::new_v4();
    let k1 = p.tenant_index_key(t1).await.unwrap();
    assert_eq!(k1.as_bytes(), p.tenant_index_key(t1).await.unwrap().as_bytes());
    assert_ne!(k1.as_bytes(), p.tenant_index_key(t2).await.unwrap().as_bytes());
    assert_ne!(k1.as_bytes(), &p.derive_kek(t1).as_bytes()[..]);
}

#[test]
fn from_base64_accepts_32_bytes_and_rejects_others() {
    let engine = base64::engine::general_purpose::STANDARD;
    assert!(LocalKeyProvider::from_base64(&engine.encode([5u8; 32])).is_ok());
    assert!(matches!(
        LocalKeyProvider::from_base64(&engine.encode([5u8; 16])),
        Err(CryptoError::InvalidMasterKey)
    ));
    assert!(matches!(
        LocalKeyProvider::from_base64("not base64!!"),
        Err(CryptoError::InvalidMasterKey)
    ));
}

#[test]
fn from_env_value_distinguishes_missing_from_invalid() {
    assert!(matches!(LocalKeyProvider::from_env_value(None), Err(CryptoError::MissingMasterKey)));
    assert!(matches!(
        LocalKeyProvider::from_env_value(Some("  ".to_string())),
        Err(CryptoError::MissingMasterKey)
    ));
    let engine = base64::engine::general_purpose::STANDARD;
    assert!(LocalKeyProvider::from_env_value(Some(engine.encode([5u8; 32]))).is_ok());
}

#[test]
fn master_key_env_var_name_is_stable() {
    assert_eq!(MASTER_KEY_ENV, "YOCHO_MASTER_KEY");
}
