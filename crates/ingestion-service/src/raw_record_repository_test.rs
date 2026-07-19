use super::*;
use std::sync::Mutex;

/// In-memory test double shared by this module's tests and by ingest_handler's unit tests, so
/// handler logic can be verified without a live Postgres instance (CLAUDE.md §2).
#[derive(Default)]
pub struct InMemoryRawRecordRepository {
    pub records: Mutex<Vec<RawRecord>>,
}

#[async_trait]
impl RawRecordRepository for InMemoryRawRecordRepository {
    async fn insert(&self, record: &RawRecord) -> Result<bool, RepositoryError> {
        let mut records = self.records.lock().unwrap();
        if let Some(external_id) = &record.external_id {
            let already_present = records.iter().any(|r| {
                r.tenant_id == record.tenant_id
                    && r.connector_id == record.connector_id
                    && r.external_id.as_deref() == Some(external_id.as_str())
            });
            if already_present {
                return Ok(false);
            }
        }
        records.push(record.clone());
        Ok(true)
    }

    async fn update_normalized_payload(
        &self,
        record_id: uuid::Uuid,
        normalized_payload: &serde_json::Value,
    ) -> Result<bool, RepositoryError> {
        let mut records = self.records.lock().unwrap();
        match records.iter_mut().find(|r| r.id == record_id) {
            Some(record) => {
                record.normalized_payload = Some(normalized_payload.clone());
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn list_older_than(
        &self,
        tenant_id: uuid::Uuid,
        cutoff: chrono::DateTime<chrono::Utc>,
        limit: i64,
    ) -> Result<Vec<RawRecord>, RepositoryError> {
        let records = self.records.lock().unwrap();
        let mut matching: Vec<RawRecord> = records
            .iter()
            .filter(|r| r.tenant_id == tenant_id && r.ingested_at < cutoff)
            .cloned()
            .collect();
        matching.sort_by_key(|r| r.ingested_at);
        matching.truncate(limit as usize);
        Ok(matching)
    }

    async fn delete(
        &self,
        tenant_id: uuid::Uuid,
        record_id: uuid::Uuid,
    ) -> Result<bool, RepositoryError> {
        let mut records = self.records.lock().unwrap();
        let before = records.len();
        records.retain(|r| !(r.id == record_id && r.tenant_id == tenant_id));
        Ok(records.len() < before)
    }

    async fn stats_by_connector(
        &self,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<ConnectorStats>, RepositoryError> {
        let records = self.records.lock().unwrap();
        let mut by_connector: std::collections::BTreeMap<
            String,
            (i64, chrono::DateTime<chrono::Utc>),
        > = std::collections::BTreeMap::new();
        for r in records.iter().filter(|r| r.tenant_id == tenant_id) {
            let entry = by_connector.entry(r.connector_id.clone()).or_insert((0, r.ingested_at));
            entry.0 += 1;
            if r.ingested_at > entry.1 {
                entry.1 = r.ingested_at;
            }
        }
        Ok(by_connector
            .into_iter()
            .map(|(connector_id, (record_count, last_ingested_at))| ConnectorStats {
                connector_id,
                record_count,
                last_ingested_at,
            })
            .collect())
    }

    async fn list_by_connector(
        &self,
        tenant_id: uuid::Uuid,
        connector_id: &str,
        limit: i64,
    ) -> Result<Vec<RawRecord>, RepositoryError> {
        let records = self.records.lock().unwrap();
        let mut matching: Vec<RawRecord> = records
            .iter()
            .filter(|r| r.tenant_id == tenant_id && r.connector_id == connector_id)
            .cloned()
            .collect();
        matching.sort_by_key(|r| std::cmp::Reverse(r.ingested_at));
        matching.truncate(limit as usize);
        Ok(matching)
    }

    async fn get_by_id(
        &self,
        tenant_id: uuid::Uuid,
        record_id: uuid::Uuid,
    ) -> Result<Option<RawRecord>, RepositoryError> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == record_id && r.tenant_id == tenant_id)
            .cloned())
    }

    async fn search(
        &self,
        tenant_id: uuid::Uuid,
        filter: &RecordSearchFilter,
    ) -> Result<Vec<RawRecord>, RepositoryError> {
        let records = self.records.lock().unwrap();
        let mut matching: Vec<RawRecord> = records
            .iter()
            .filter(|r| r.tenant_id == tenant_id)
            .filter(|r| filter.connector_id.as_deref().is_none_or(|c| r.connector_id == c))
            .filter(|r| filter.source_type.is_none_or(|s| r.source_type == s))
            .filter(|r| filter.from.is_none_or(|from| r.ingested_at >= from))
            .filter(|r| filter.to.is_none_or(|to| r.ingested_at <= to))
            .filter(|r| {
                filter.query.as_deref().is_none_or(|q| {
                    r.raw_payload.to_string().to_lowercase().contains(&q.to_lowercase())
                })
            })
            .filter(|r| {
                filter.subject.as_deref().is_none_or(|s| {
                    r.raw_payload
                        .get("subject")
                        .and_then(|v| v.as_str())
                        .is_some_and(|subject| subject.to_lowercase().contains(&s.to_lowercase()))
                })
            })
            .filter(|r| {
                filter.email_from.as_deref().is_none_or(|f| {
                    r.raw_payload
                        .get("from")
                        .and_then(|v| v.as_str())
                        .is_some_and(|from| from.to_lowercase().contains(&f.to_lowercase()))
                })
            })
            .filter(|r| {
                filter.attachment_filename.as_deref().is_none_or(|f| {
                    r.raw_payload.get("attachments").and_then(|v| v.as_array()).is_some_and(
                        |attachments| {
                            attachments.iter().any(|a| {
                                a.get("filename").and_then(|v| v.as_str()).is_some_and(|filename| {
                                    filename.to_lowercase().contains(&f.to_lowercase())
                                })
                            })
                        },
                    )
                })
            })
            .filter(|r| filter.normalized.is_none_or(|n| r.normalized_payload.is_some() == n))
            .cloned()
            .collect();
        matching.sort_by_key(|r| std::cmp::Reverse(r.ingested_at));
        let matching =
            matching.into_iter().skip(filter.offset as usize).take(filter.limit as usize).collect();
        Ok(matching)
    }
}

pub struct FailingRawRecordRepository;

#[async_trait]
impl RawRecordRepository for FailingRawRecordRepository {
    async fn insert(&self, _record: &RawRecord) -> Result<bool, RepositoryError> {
        Err(RepositoryError::Backend("simulated failure".to_string()))
    }

    async fn update_normalized_payload(
        &self,
        _record_id: uuid::Uuid,
        _normalized_payload: &serde_json::Value,
    ) -> Result<bool, RepositoryError> {
        Err(RepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list_older_than(
        &self,
        _tenant_id: uuid::Uuid,
        _cutoff: chrono::DateTime<chrono::Utc>,
        _limit: i64,
    ) -> Result<Vec<RawRecord>, RepositoryError> {
        Err(RepositoryError::Backend("simulated failure".to_string()))
    }

    async fn delete(
        &self,
        _tenant_id: uuid::Uuid,
        _record_id: uuid::Uuid,
    ) -> Result<bool, RepositoryError> {
        Err(RepositoryError::Backend("simulated failure".to_string()))
    }

    async fn stats_by_connector(
        &self,
        _tenant_id: uuid::Uuid,
    ) -> Result<Vec<ConnectorStats>, RepositoryError> {
        Err(RepositoryError::Backend("simulated failure".to_string()))
    }

    async fn list_by_connector(
        &self,
        _tenant_id: uuid::Uuid,
        _connector_id: &str,
        _limit: i64,
    ) -> Result<Vec<RawRecord>, RepositoryError> {
        Err(RepositoryError::Backend("simulated failure".to_string()))
    }

    async fn get_by_id(
        &self,
        _tenant_id: uuid::Uuid,
        _record_id: uuid::Uuid,
    ) -> Result<Option<RawRecord>, RepositoryError> {
        Err(RepositoryError::Backend("simulated failure".to_string()))
    }

    async fn search(
        &self,
        _tenant_id: uuid::Uuid,
        _filter: &RecordSearchFilter,
    ) -> Result<Vec<RawRecord>, RepositoryError> {
        Err(RepositoryError::Backend("simulated failure".to_string()))
    }
}

#[tokio::test]
async fn in_memory_repository_stores_inserted_records() {
    let repo = InMemoryRawRecordRepository::default();
    let record = RawRecord::new(
        "zendesk",
        common::SourceType::Ticket,
        uuid::Uuid::new_v4(),
        serde_json::json!({}),
    );

    let inserted = repo.insert(&record).await.unwrap();

    assert!(inserted);
    let stored = repo.records.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0], record);
}

#[tokio::test]
async fn inserting_a_record_with_no_external_id_is_never_deduped() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let record_a =
        RawRecord::new("zendesk", common::SourceType::Ticket, tenant_id, serde_json::json!({}));
    let record_b =
        RawRecord::new("zendesk", common::SourceType::Ticket, tenant_id, serde_json::json!({}));

    assert!(repo.insert(&record_a).await.unwrap());
    assert!(repo.insert(&record_b).await.unwrap());
    assert_eq!(repo.records.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn reinserting_the_same_external_id_for_the_same_tenant_and_connector_is_a_no_op() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let record =
        RawRecord::new("imap", common::SourceType::Message, tenant_id, serde_json::json!({}))
            .with_external_id("message-id-123");

    let first = repo.insert(&record).await.unwrap();
    let second = repo.insert(&record).await.unwrap();

    assert!(first, "first insert of a new external_id must succeed");
    assert!(!second, "re-inserting the same external_id must be a no-op, not a duplicate row");
    assert_eq!(repo.records.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn the_same_external_id_is_not_deduped_across_different_tenants() {
    let repo = InMemoryRawRecordRepository::default();
    let record_a = RawRecord::new(
        "imap",
        common::SourceType::Message,
        uuid::Uuid::new_v4(),
        serde_json::json!({}),
    )
    .with_external_id("message-id-123");
    let record_b = RawRecord::new(
        "imap",
        common::SourceType::Message,
        uuid::Uuid::new_v4(),
        serde_json::json!({}),
    )
    .with_external_id("message-id-123");

    assert!(repo.insert(&record_a).await.unwrap());
    assert!(repo.insert(&record_b).await.unwrap());
    assert_eq!(repo.records.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn failing_repository_returns_backend_error() {
    let repo = FailingRawRecordRepository;
    let record = RawRecord::new(
        "zendesk",
        common::SourceType::Ticket,
        uuid::Uuid::new_v4(),
        serde_json::json!({}),
    );

    let err = repo.insert(&record).await.unwrap_err();
    assert!(matches!(err, RepositoryError::Backend(_)));
}

#[tokio::test]
async fn update_normalized_payload_sets_it_on_a_known_record_and_returns_true() {
    let repo = InMemoryRawRecordRepository::default();
    let record = RawRecord::new(
        "zendesk",
        common::SourceType::Ticket,
        uuid::Uuid::new_v4(),
        serde_json::json!({}),
    );
    repo.insert(&record).await.unwrap();

    let normalized = serde_json::json!({"text": "hi"});
    let updated = repo.update_normalized_payload(record.id, &normalized).await.unwrap();

    assert!(updated);
    assert_eq!(repo.records.lock().unwrap()[0].normalized_payload, Some(normalized));
}

#[tokio::test]
async fn update_normalized_payload_returns_false_for_unknown_record() {
    let repo = InMemoryRawRecordRepository::default();
    let updated =
        repo.update_normalized_payload(uuid::Uuid::new_v4(), &serde_json::json!({})).await.unwrap();
    assert!(!updated);
}

fn record_for_tenant_ingested_at(
    tenant_id: uuid::Uuid,
    ingested_at: chrono::DateTime<chrono::Utc>,
) -> RawRecord {
    let mut record =
        RawRecord::new("zendesk", common::SourceType::Ticket, tenant_id, serde_json::json!({}));
    record.ingested_at = ingested_at;
    record
}

fn record_ingested_at(ingested_at: chrono::DateTime<chrono::Utc>) -> RawRecord {
    record_for_tenant_ingested_at(uuid::Uuid::new_v4(), ingested_at)
}

#[tokio::test]
async fn list_older_than_returns_only_records_before_the_cutoff_oldest_first() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    let old = record_for_tenant_ingested_at(tenant_id, now - chrono::Duration::days(10));
    let older = record_for_tenant_ingested_at(tenant_id, now - chrono::Duration::days(20));
    let recent = record_for_tenant_ingested_at(tenant_id, now);
    repo.insert(&old).await.unwrap();
    repo.insert(&older).await.unwrap();
    repo.insert(&recent).await.unwrap();

    let cutoff = now - chrono::Duration::days(5);
    let found = repo.list_older_than(tenant_id, cutoff, 10).await.unwrap();

    assert_eq!(found, vec![older, old]);
}

#[tokio::test]
async fn list_older_than_is_scoped_to_tenant() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    let mine = record_for_tenant_ingested_at(tenant_id, now - chrono::Duration::days(10));
    let someone_elses =
        record_for_tenant_ingested_at(uuid::Uuid::new_v4(), now - chrono::Duration::days(10));
    repo.insert(&mine).await.unwrap();
    repo.insert(&someone_elses).await.unwrap();

    let found = repo.list_older_than(tenant_id, now, 10).await.unwrap();
    assert_eq!(found, vec![mine]);
}

#[tokio::test]
async fn list_older_than_respects_the_limit() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    for days_ago in 1..=5 {
        repo.insert(&record_for_tenant_ingested_at(
            tenant_id,
            now - chrono::Duration::days(days_ago),
        ))
        .await
        .unwrap();
    }

    let found = repo.list_older_than(tenant_id, now, 2).await.unwrap();
    assert_eq!(found.len(), 2);
}

#[tokio::test]
async fn delete_removes_a_known_record_and_returns_true() {
    let repo = InMemoryRawRecordRepository::default();
    let record = record_ingested_at(chrono::Utc::now());
    repo.insert(&record).await.unwrap();

    let deleted = repo.delete(record.tenant_id, record.id).await.unwrap();

    assert!(deleted);
    assert!(repo.records.lock().unwrap().is_empty());
}

#[tokio::test]
async fn delete_returns_false_for_unknown_record() {
    let repo = InMemoryRawRecordRepository::default();
    let deleted = repo.delete(uuid::Uuid::new_v4(), uuid::Uuid::new_v4()).await.unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn delete_returns_false_when_tenant_does_not_match() {
    let repo = InMemoryRawRecordRepository::default();
    let record = record_ingested_at(chrono::Utc::now());
    repo.insert(&record).await.unwrap();

    let deleted = repo.delete(uuid::Uuid::new_v4(), record.id).await.unwrap();

    assert!(!deleted);
    assert_eq!(repo.records.lock().unwrap().len(), 1);
}

fn record_for_connector(
    tenant_id: uuid::Uuid,
    connector_id: &str,
    ingested_at: chrono::DateTime<chrono::Utc>,
) -> RawRecord {
    let mut record =
        RawRecord::new(connector_id, common::SourceType::Ticket, tenant_id, serde_json::json!({}));
    record.ingested_at = ingested_at;
    record
}

#[tokio::test]
async fn stats_by_connector_aggregates_count_and_latest_ingested_at() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    repo.insert(&record_for_connector(tenant_id, "zendesk", now - chrono::Duration::days(1)))
        .await
        .unwrap();
    repo.insert(&record_for_connector(tenant_id, "zendesk", now)).await.unwrap();
    repo.insert(&record_for_connector(tenant_id, "sql", now)).await.unwrap();

    let mut stats = repo.stats_by_connector(tenant_id).await.unwrap();
    stats.sort_by(|a, b| a.connector_id.cmp(&b.connector_id));

    assert_eq!(stats.len(), 2);
    assert_eq!(stats[0].connector_id, "sql");
    assert_eq!(stats[0].record_count, 1);
    assert_eq!(stats[1].connector_id, "zendesk");
    assert_eq!(stats[1].record_count, 2);
    assert_eq!(stats[1].last_ingested_at, now);
}

#[tokio::test]
async fn stats_by_connector_is_scoped_to_tenant() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    repo.insert(&record_for_connector(tenant_id, "zendesk", now)).await.unwrap();
    repo.insert(&record_for_connector(uuid::Uuid::new_v4(), "zendesk", now)).await.unwrap();

    let stats = repo.stats_by_connector(tenant_id).await.unwrap();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].record_count, 1);
}

#[tokio::test]
async fn list_by_connector_returns_only_matching_connector_newest_first() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    let older = record_for_connector(tenant_id, "zendesk", now - chrono::Duration::days(1));
    let newer = record_for_connector(tenant_id, "zendesk", now);
    let other_connector = record_for_connector(tenant_id, "sql", now);
    repo.insert(&older).await.unwrap();
    repo.insert(&newer).await.unwrap();
    repo.insert(&other_connector).await.unwrap();

    let found = repo.list_by_connector(tenant_id, "zendesk", 10).await.unwrap();
    assert_eq!(found, vec![newer, older]);
}

#[tokio::test]
async fn list_by_connector_respects_the_limit() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    for days_ago in 1..=5 {
        repo.insert(&record_for_connector(
            tenant_id,
            "zendesk",
            now - chrono::Duration::days(days_ago),
        ))
        .await
        .unwrap();
    }

    let found = repo.list_by_connector(tenant_id, "zendesk", 2).await.unwrap();
    assert_eq!(found.len(), 2);
}

#[tokio::test]
async fn get_by_id_returns_the_record_when_tenant_matches() {
    let repo = InMemoryRawRecordRepository::default();
    let record = record_ingested_at(chrono::Utc::now());
    repo.insert(&record).await.unwrap();

    let found = repo.get_by_id(record.tenant_id, record.id).await.unwrap();
    assert_eq!(found, Some(record));
}

#[tokio::test]
async fn get_by_id_returns_none_when_tenant_does_not_match() {
    let repo = InMemoryRawRecordRepository::default();
    let record = record_ingested_at(chrono::Utc::now());
    repo.insert(&record).await.unwrap();

    let found = repo.get_by_id(uuid::Uuid::new_v4(), record.id).await.unwrap();
    assert_eq!(found, None);
}

fn record_with_payload(
    tenant_id: uuid::Uuid,
    connector_id: &str,
    ingested_at: chrono::DateTime<chrono::Utc>,
    payload: serde_json::Value,
) -> RawRecord {
    let mut record = RawRecord::new(connector_id, common::SourceType::Ticket, tenant_id, payload);
    record.ingested_at = ingested_at;
    record
}

#[tokio::test]
async fn search_with_no_filters_returns_all_records_for_the_tenant() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    repo.insert(&record_for_connector(tenant_id, "zendesk", now)).await.unwrap();
    repo.insert(&record_for_connector(uuid::Uuid::new_v4(), "zendesk", now)).await.unwrap();

    let filter = RecordSearchFilter { limit: 10, ..Default::default() };
    let found = repo.search(tenant_id, &filter).await.unwrap();
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn search_filters_by_connector_id() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    repo.insert(&record_for_connector(tenant_id, "zendesk", now)).await.unwrap();
    repo.insert(&record_for_connector(tenant_id, "sql", now)).await.unwrap();

    let filter = RecordSearchFilter {
        connector_id: Some("zendesk".to_string()),
        limit: 10,
        ..Default::default()
    };
    let found = repo.search(tenant_id, &filter).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].connector_id, "zendesk");
}

#[tokio::test]
async fn search_filters_by_free_text_query_against_the_raw_payload() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    let matching = record_with_payload(
        tenant_id,
        "zendesk",
        now,
        serde_json::json!({"subject": "printer on fire"}),
    );
    let non_matching = record_with_payload(
        tenant_id,
        "zendesk",
        now,
        serde_json::json!({"subject": "password reset"}),
    );
    repo.insert(&matching).await.unwrap();
    repo.insert(&non_matching).await.unwrap();

    let filter =
        RecordSearchFilter { query: Some("printer".to_string()), limit: 10, ..Default::default() };
    let found = repo.search(tenant_id, &filter).await.unwrap();
    assert_eq!(found, vec![matching]);
}

#[tokio::test]
async fn search_filters_by_date_range() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    let old = record_for_connector(tenant_id, "zendesk", now - chrono::Duration::days(30));
    let recent = record_for_connector(tenant_id, "zendesk", now);
    repo.insert(&old).await.unwrap();
    repo.insert(&recent).await.unwrap();

    let filter = RecordSearchFilter {
        from: Some(now - chrono::Duration::days(1)),
        limit: 10,
        ..Default::default()
    };
    let found = repo.search(tenant_id, &filter).await.unwrap();
    assert_eq!(found, vec![recent]);
}

fn email_record(
    tenant_id: uuid::Uuid,
    subject: &str,
    from: &str,
    attachment_filenames: &[&str],
) -> RawRecord {
    let attachments: Vec<serde_json::Value> = attachment_filenames
        .iter()
        .map(|f| serde_json::json!({"filename": f, "content_type": "application/pdf", "size_bytes": 10}))
        .collect();
    RawRecord::new(
        "graph-mail",
        common::SourceType::Message,
        tenant_id,
        serde_json::json!({"subject": subject, "from": from, "body": "hi", "attachments": attachments}),
    )
}

#[tokio::test]
async fn search_filters_by_subject() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let matching = email_record(tenant_id, "printer on fire", "alice@example.com", &[]);
    let non_matching = email_record(tenant_id, "password reset", "bob@example.com", &[]);
    repo.insert(&matching).await.unwrap();
    repo.insert(&non_matching).await.unwrap();

    let filter = RecordSearchFilter {
        subject: Some("printer".to_string()),
        limit: 10,
        ..Default::default()
    };
    let found = repo.search(tenant_id, &filter).await.unwrap();
    assert_eq!(found, vec![matching]);
}

#[tokio::test]
async fn search_filters_by_email_from() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let matching = email_record(tenant_id, "hi", "alice@example.com", &[]);
    let non_matching = email_record(tenant_id, "hi", "bob@example.com", &[]);
    repo.insert(&matching).await.unwrap();
    repo.insert(&non_matching).await.unwrap();

    let filter = RecordSearchFilter {
        email_from: Some("alice".to_string()),
        limit: 10,
        ..Default::default()
    };
    let found = repo.search(tenant_id, &filter).await.unwrap();
    assert_eq!(found, vec![matching]);
}

#[tokio::test]
async fn search_filters_by_attachment_filename() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let matching = email_record(tenant_id, "hi", "alice@example.com", &["invoice.pdf"]);
    let non_matching = email_record(tenant_id, "hi", "alice@example.com", &["photo.jpg"]);
    repo.insert(&matching).await.unwrap();
    repo.insert(&non_matching).await.unwrap();

    let filter = RecordSearchFilter {
        attachment_filename: Some("invoice".to_string()),
        limit: 10,
        ..Default::default()
    };
    let found = repo.search(tenant_id, &filter).await.unwrap();
    assert_eq!(found, vec![matching]);
}

#[tokio::test]
async fn search_normalized_false_finds_only_records_with_no_normalized_payload() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    let unnormalized =
        RawRecord::new("imap", common::SourceType::Message, tenant_id, serde_json::json!({}));
    let mut normalized =
        RawRecord::new("imap", common::SourceType::Message, tenant_id, serde_json::json!({}));
    normalized.normalized_payload = Some(serde_json::json!({"subject": "hi"}));
    repo.insert(&unnormalized).await.unwrap();
    repo.insert(&normalized).await.unwrap();

    let filter = RecordSearchFilter { normalized: Some(false), limit: 10, ..Default::default() };
    let found = repo.search(tenant_id, &filter).await.unwrap();

    assert_eq!(found, vec![unnormalized]);
}

#[tokio::test]
async fn search_by_subject_does_not_match_records_with_no_subject_field() {
    let repo = InMemoryRawRecordRepository::default();
    let tenant_id = uuid::Uuid::new_v4();
    repo.insert(&record_for_connector(tenant_id, "sql", chrono::Utc::now())).await.unwrap();

    let filter = RecordSearchFilter {
        subject: Some("anything".to_string()),
        limit: 10,
        ..Default::default()
    };
    let found = repo.search(tenant_id, &filter).await.unwrap();
    assert!(found.is_empty());
}
