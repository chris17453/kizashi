//! Shared connector runtime (ADR-0013): the pieces every connector's CronJob-scheduled poll
//! cycle (spec §3) needs — posting polled records to Ingestion Gateway, running one poll
//! cycle, and the Entra ID client-credentials flow ADR-0003 specifies for Entra-backed
//! sources (`graph-mail`, `graph-teams`, `fabric`).

mod entra_client_credentials;
mod ingestion_client;
mod poll_runner;

pub use common::{build_outbound_client, EgressClientError};
pub use entra_client_credentials::{fetch_access_token, EntraAuthError};
pub use ingestion_client::{HttpIngestionClient, IngestionClient, IngestionClientError};
pub use poll_runner::{run_poll_cycle, PollRunError, PollSummary};
