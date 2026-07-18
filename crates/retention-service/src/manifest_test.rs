use super::*;

#[test]
fn manifest_round_trips_through_json() {
    let manifest = ArchiveManifest::new(
        Uuid::new_v4(),
        "raw",
        3,
        "2026-01-01T00:00:00Z".parse().unwrap(),
        "2026-01-02T00:00:00Z".parse().unwrap(),
    );

    let json = serde_json::to_string(&manifest).unwrap();
    let parsed: ArchiveManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed, manifest);
    assert_eq!(parsed.format_version, CURRENT_FORMAT_VERSION);
}

#[test]
fn archive_key_is_partitioned_by_tenant_data_class_and_date() {
    let tenant_id = Uuid::new_v4();
    let batch_id = Uuid::new_v4();
    let window_end: chrono::DateTime<chrono::Utc> = "2026-03-05T00:00:00Z".parse().unwrap();

    let key = archive_key(tenant_id, "raw", window_end, batch_id);

    assert_eq!(key, format!("archive/{tenant_id}/raw/2026/03/05/{batch_id}.ndjson.gz"));
}
