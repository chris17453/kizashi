use axum::{body::Body, http::Request, routing::get, Router};
use std::sync::Arc;
use tower::ServiceExt; // for `oneshot`

use crate::{
    ontology_handler::list_ontology, state::AppState, InMemorySessionStore, 
    HttpAuthClient, HttpMfaClient, HttpBrandingClient, HttpOidcClient, InMemoryPendingOidcFlowStore,
    HttpEventsClient, HttpTriggersClient, HttpIncidentsClient, HttpHealthClient, HttpSensorsClient,
    HttpApiKeysClient, HttpBacklogClient, HttpExecutionClient, HttpIngestionStatsClient,
    HttpAnalysisConfigClient, HttpNormalizationMappingsClient, HttpRetentionPoliciesClient,
    HttpEgressAllowlistClient, HttpBackupStatusClient, HttpUsersClient, HttpLoginAttemptsClient,
    HttpSavedSearchQueriesClient, HttpAuditLogClient, IngestionGatewayApiKeyAuditLogClient
};

// We don't have to write a full integration test here since we just want to ensure it compiles
// and doesn't panic.
