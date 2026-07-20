use super::*;

pub struct InMemoryPgDumpRunner {
    pub bytes: Vec<u8>,
}

#[async_trait]
impl PgDumpRunner for InMemoryPgDumpRunner {
    async fn dump(&self) -> Result<Vec<u8>, PgDumpError> {
        Ok(self.bytes.clone())
    }
}

pub struct FailingPgDumpRunner;

#[async_trait]
impl PgDumpRunner for FailingPgDumpRunner {
    async fn dump(&self) -> Result<Vec<u8>, PgDumpError> {
        Err(PgDumpError::NonZeroExit(1, "simulated failure".to_string()))
    }
}
