#[path = "pg_dump_runner_test.rs"]
#[cfg(test)]
pub(crate) mod pg_dump_runner_test;

use async_trait::async_trait;
use thiserror::Error;
use tokio::process::Command;

#[derive(Debug, Error)]
pub enum PgDumpError {
    #[error("failed to launch pg_dump: {0}")]
    Launch(String),
    #[error("pg_dump exited with status {0}: {1}")]
    NonZeroExit(i32, String),
}

/// Shells out to the real `pg_dump` binary (ADR-0055) rather than reimplementing a Postgres
/// dump format in Rust -- `pg_dump` is what any operator/DBA already trusts to produce a
/// restorable backup, and reinventing it would be exactly the kind of "looks compliant, isn't
/// actually restorable" gap CLAUDE.md's "no half-truths" rule warns against.
#[async_trait]
pub trait PgDumpRunner: Send + Sync {
    async fn dump(&self) -> Result<Vec<u8>, PgDumpError>;
}

pub struct ProcessPgDumpRunner {
    database_url: String,
}

impl ProcessPgDumpRunner {
    pub fn new(database_url: String) -> Self {
        Self { database_url }
    }
}

#[async_trait]
impl PgDumpRunner for ProcessPgDumpRunner {
    async fn dump(&self) -> Result<Vec<u8>, PgDumpError> {
        // `--format=custom` produces a compressed, `pg_restore`-only archive -- smaller than
        // plain SQL and the format `pg_restore` (not psql) expects to read back.
        let output = Command::new("pg_dump")
            .arg(format!("--dbname={}", self.database_url))
            .arg("--format=custom")
            .output()
            .await
            .map_err(|e| PgDumpError::Launch(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(PgDumpError::NonZeroExit(output.status.code().unwrap_or(-1), stderr));
        }

        Ok(output.stdout)
    }
}
