#[path = "db_test.rs"]
#[cfg(test)]
mod db_test;

use sqlx::postgres::PgPoolOptions;
use sqlx::Executor;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("invalid schema name `{0}`: must be a valid lowercase Postgres identifier")]
    InvalidSchemaName(String),
    #[error("database connection error: {0}")]
    Connect(#[from] sqlx::Error),
}

/// Every service on the shared Postgres instance owns its tables in its own schema, not the
/// default `public` one — this is what keeps two services from ever colliding on table names
/// (or, before this existed, on sqlx's shared `_sqlx_migrations` version-history table: two
/// services both shipping a "0001_..." migration produced a checksum VersionMismatch the
/// moment both were deployed against the same database). Schema is applied to every pooled
/// connection via `after_connect`, so callers never need to remember to set it themselves.
pub async fn connect_with_schema(
    database_url: &str,
    schema: &str,
) -> Result<sqlx::PgPool, ConnectError> {
    if !is_valid_schema_name(schema) {
        return Err(ConnectError::InvalidSchemaName(schema.to_string()));
    }
    let schema = schema.to_string();
    let pool = PgPoolOptions::new()
        .after_connect(move |conn, _meta| {
            let schema = schema.clone();
            Box::pin(async move {
                conn.execute(format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\"").as_str()).await?;
                conn.execute(format!("SET search_path TO \"{schema}\"").as_str()).await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await?;
    Ok(pool)
}

fn is_valid_schema_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
