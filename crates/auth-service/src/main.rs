use auth_service::{
    build_router, health_router, AuthState, HttpSessionClient, OidcClients, OidcProviderConfig,
    PostgresLocalUserRepository, PostgresTenantRepository, StandardOidcClient,
};
use std::sync::Arc;

fn oidc_client_from_env(
    prefix: &str,
    redirect_env: &str,
) -> Option<Arc<dyn auth_service::OidcClient>> {
    let client_id = std::env::var(format!("{prefix}_CLIENT_ID")).ok().filter(|v| !v.is_empty())?;
    let client_secret =
        std::env::var(format!("{prefix}_CLIENT_SECRET")).ok().filter(|v| !v.is_empty())?;
    let auth_url = std::env::var(format!("{prefix}_AUTH_URL")).ok().filter(|v| !v.is_empty())?;
    let token_url = std::env::var(format!("{prefix}_TOKEN_URL")).ok().filter(|v| !v.is_empty())?;
    let userinfo_url =
        std::env::var(format!("{prefix}_USERINFO_URL")).ok().filter(|v| !v.is_empty())?;
    let redirect_url = std::env::var(redirect_env).ok().filter(|v| !v.is_empty())?;

    let config = OidcProviderConfig {
        client_id,
        client_secret,
        auth_url,
        token_url,
        userinfo_url,
        redirect_url,
    };
    match StandardOidcClient::new(config) {
        Ok(client) => Some(Arc::new(client)),
        Err(e) => {
            tracing::error!(provider = prefix, error = %e, "failed to build OIDC client, skipping");
            None
        }
    }
}

fn entra_oidc_client() -> Option<Arc<dyn auth_service::OidcClient>> {
    let tenant_id = std::env::var("ENTRA_TENANT_ID").ok().filter(|v| !v.is_empty())?;
    let client_id = std::env::var("ENTRA_CLIENT_ID").ok().filter(|v| !v.is_empty())?;
    let client_secret = std::env::var("ENTRA_CLIENT_SECRET").ok().filter(|v| !v.is_empty())?;
    let redirect_url = std::env::var("ENTRA_REDIRECT_URL").ok().filter(|v| !v.is_empty())?;

    let config = OidcProviderConfig {
        client_id,
        client_secret,
        auth_url: format!("https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/authorize"),
        token_url: format!("https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token"),
        userinfo_url: "https://graph.microsoft.com/oidc/userinfo".to_string(),
        redirect_url,
    };
    match StandardOidcClient::new(config) {
        Ok(client) => Some(Arc::new(client)),
        Err(e) => {
            tracing::error!(error = %e, "failed to build Entra OIDC client, skipping");
            None
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let query_gateway_url =
        std::env::var("QUERY_GATEWAY_URL").expect("QUERY_GATEWAY_URL must be set");
    let internal_secret =
        std::env::var("INTERNAL_API_SECRET").expect("INTERNAL_API_SECRET must be set");
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let pool = common::connect_with_schema(&database_url, "auth_service")
        .await
        .expect("failed to connect to postgres");
    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations_dir)
        .await
        .expect("failed to load migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let mut oidc_clients: OidcClients = std::collections::HashMap::new();
    if let Some(client) = entra_oidc_client() {
        oidc_clients.insert("entra".to_string(), client);
        tracing::info!("configured OIDC provider: entra");
    }
    if let Some(client) = oidc_client_from_env("GENERIC_OAUTH", "GENERIC_OAUTH_REDIRECT_URL") {
        oidc_clients.insert("generic".to_string(), client);
        tracing::info!("configured OIDC provider: generic");
    }

    let state = AuthState {
        local_user_repository: Arc::new(PostgresLocalUserRepository::new(pool.clone())),
        tenant_repository: Arc::new(PostgresTenantRepository::new(pool)),
        session_client: Arc::new(HttpSessionClient::new(
            reqwest::Client::new(),
            query_gateway_url,
            internal_secret,
        )),
        oidc_clients,
    };

    let app = health_router().merge(build_router(state));
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind failed");
    tracing::info!(%addr, "auth-service listening");
    axum::serve(listener, app).await.expect("server error");
}
