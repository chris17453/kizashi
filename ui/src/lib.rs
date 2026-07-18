//! Console UI (spec §7): a server-rendered Rust web app (ADR-0014) — axum + askama, no WASM
//! build step, tested the same way as every other service in this repo
//! (`tower::ServiceExt::oneshot` against an in-process router).

mod auth_client;
mod events_client;
mod health_client;
mod session;
mod session_guard;
mod triggers_client;

mod events_handler;
mod health_handler;
mod healthz;
mod login_handler;
mod logout_handler;
mod triggers_handler;

pub use auth_client::{AuthClient, AuthClientError, HttpAuthClient};
pub use events_client::{EventSummary, EventsClient, EventsClientError, HttpEventsClient};
pub use health_client::{
    HealthClient, HealthClientError, HttpHealthClient, PlatformHealthSummary, ServiceHealthSummary,
};
pub use session::{InMemorySessionStore, Session, SessionStore};
pub use triggers_client::{
    HttpTriggersClient, TriggerSummary, TriggersClient, TriggersClientError,
};

pub use events_handler::get_events;
pub use health_handler::get_health;
pub use healthz::healthz;
pub use login_handler::{get_login, post_login};
pub use logout_handler::get_logout;
pub use triggers_handler::get_triggers;

use axum::routing::get;
use axum::Router;
use std::sync::Arc;

pub const SESSION_COOKIE_NAME: &str = "kizashi_session";

#[derive(Clone)]
pub struct AppState {
    pub session_store: Arc<dyn SessionStore>,
    pub auth_client: Arc<dyn AuthClient>,
    pub events_client: Arc<dyn EventsClient>,
    pub triggers_client: Arc<dyn TriggersClient>,
    pub health_client: Arc<dyn HealthClient>,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/login", get(get_login).post(post_login))
        .route("/logout", get(get_logout))
        .route("/events", get(get_events))
        .route("/triggers", get(get_triggers))
        .route("/health", get(get_health))
        .with_state(state)
}
