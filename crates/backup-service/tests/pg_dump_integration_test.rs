//! Integration test against the real `pg_dump` binary and a real Postgres instance (CLAUDE.md
//! §2) — this is the one piece of the backup pipeline that genuinely cannot be verified with a
//! test double, since the whole point is producing a restorable archive of a real database.
//! Requires DATABASE_URL.

use backup_service::{PgDumpRunner, ProcessPgDumpRunner};

#[tokio::test]
async fn a_real_pg_dump_against_a_reachable_database_produces_a_nonempty_custom_format_archive() {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set to run this test");
    let runner = ProcessPgDumpRunner::new(database_url);

    let bytes = runner.dump().await.unwrap();

    // pg_dump's custom format always starts with the magic bytes "PGDMP".
    assert!(bytes.starts_with(b"PGDMP"), "expected a custom-format pg_dump archive");
}
