#[path = "sensor_script_handler_test.rs"]
#[cfg(test)]
mod sensor_script_handler_test;

use crate::connector_field_catalog::{display_name, fields_for, ConnectorField, CONNECTOR_TYPES};
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use std::collections::BTreeMap;

#[derive(Template)]
#[template(path = "sensor_generate_select.html")]
struct SelectConnectorTypeTemplate {
    show_nav: bool,
    is_admin: bool,
    connector_types: &'static [(&'static str, &'static str)],
}

/// GET /sensors/generate — step 1: pick a connector type. Split into its own step (rather than
/// one big form with every connector's fields crammed in) because this app renders no
/// JavaScript (ADR-0014) — there's no way to show/hide fields based on a dropdown without a
/// page load, so the page load *is* the mechanism.
pub async fn get_generate_select(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    Html(
        SelectConnectorTypeTemplate { show_nav: true, is_admin, connector_types: CONNECTOR_TYPES }
            .render()
            .unwrap(),
    )
    .into_response()
}

struct FieldView {
    env_var: &'static str,
    label: &'static str,
    secret: bool,
    optional: bool,
}

impl From<&ConnectorField> for FieldView {
    fn from(f: &ConnectorField) -> Self {
        FieldView { env_var: f.env_var, label: f.label, secret: f.secret, optional: f.optional }
    }
}

#[derive(Template)]
#[template(path = "sensor_generate_form.html")]
struct GenerateFormTemplate {
    show_nav: bool,
    is_admin: bool,
    connector_type: String,
    connector_type_label: &'static str,
    fields: Vec<FieldView>,
    gateway_url: String,
    api_key: String,
    api_key_auto_generated: bool,
}

#[derive(Debug, serde::Deserialize)]
pub struct SelectConnectorTypeQuery {
    pub connector_type: String,
}

/// GET /sensors/generate/form?connector_type=X — step 2: the connector-specific field form.
/// The gateway URL and (for operators) the API key are pre-filled from the platform's own
/// admin configuration rather than left for the operator to hunt down and paste in — the
/// gateway URL comes from `AppState` (already the case), and a fresh, single-use deploy key
/// is minted automatically via `ApiKeysClient::create_api_key` so there's no separate trip to
/// the API Keys page and back. A Viewer-role session can't create keys (RBAC v1, ADR-0016);
/// for that case the field is left blank with a link to the API Keys page instead of silently
/// failing.
pub async fn get_generate_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SelectConnectorTypeQuery>,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let Some(label) = display_name(&query.connector_type) else {
        return Html(
            SelectConnectorTypeTemplate {
                show_nav: true,
                is_admin,
                connector_types: CONNECTOR_TYPES,
            }
            .render()
            .unwrap(),
        )
        .into_response();
    };
    let fields: Vec<FieldView> =
        fields_for(&query.connector_type).iter().map(FieldView::from).collect();

    let (api_key, api_key_auto_generated) = if session.role.at_least(common::Role::Operator) {
        let label = format!("{}-deploy-key", query.connector_type);
        match state
            .api_keys_client
            .create_api_key(session.tenant_id, session.role, &label, &session.username)
            .await
        {
            Ok(plaintext) => (plaintext, true),
            Err(e) => {
                tracing::error!(error = %e, "failed to auto-generate a deploy API key");
                (String::new(), false)
            }
        }
    } else {
        (String::new(), false)
    };

    Html(
        GenerateFormTemplate {
            show_nav: true,
            is_admin,
            connector_type: query.connector_type,
            connector_type_label: label,
            fields,
            gateway_url: state.ingestion_gateway_public_url.clone(),
            api_key,
            api_key_auto_generated,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}

#[derive(Template)]
#[template(path = "sensor_generate_result.html")]
struct GenerateResultTemplate {
    show_nav: bool,
    is_admin: bool,
    connector_type_label: &'static str,
    bash_script: String,
    powershell_script: String,
    docker_command: String,
}

fn shell_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn build_scripts(
    connector_type: &str,
    connector_type_label: &'static str,
    name: &str,
    tenant_id: uuid::Uuid,
    gateway_url: &str,
    api_key: &str,
    field_values: &BTreeMap<String, String>,
) -> (String, String, String) {
    let mut bash = String::from("#!/usr/bin/env bash\nset -euo pipefail\n\n");
    let mut powershell = String::new();
    let mut docker = format!("docker compose run --rm \\\n  -e TENANT_ID={} \\\n  -e CONNECTOR_ID={} \\\n  -e INGESTION_GATEWAY_API_KEY={} \\\n", shell_quote(&tenant_id.to_string()), shell_quote(name), shell_quote(api_key));

    bash.push_str(&format!("export TENANT_ID={}\n", shell_quote(&tenant_id.to_string())));
    bash.push_str(&format!("export CONNECTOR_ID={}\n", shell_quote(name)));
    bash.push_str(&format!("export INGESTION_GATEWAY_URL={}\n", shell_quote(gateway_url)));
    bash.push_str(&format!("export INGESTION_GATEWAY_API_KEY={}\n", shell_quote(api_key)));

    powershell.push_str(&format!("$env:TENANT_ID = {}\n", shell_quote(&tenant_id.to_string())));
    powershell.push_str(&format!("$env:CONNECTOR_ID = {}\n", shell_quote(name)));
    powershell.push_str(&format!("$env:INGESTION_GATEWAY_URL = {}\n", shell_quote(gateway_url)));
    powershell.push_str(&format!("$env:INGESTION_GATEWAY_API_KEY = {}\n", shell_quote(api_key)));

    for field in fields_for(connector_type) {
        let value = field_values.get(field.env_var).map(String::as_str).unwrap_or("");
        if value.is_empty() && field.optional {
            continue;
        }
        bash.push_str(&format!("export {}={}\n", field.env_var, shell_quote(value)));
        powershell.push_str(&format!("$env:{} = {}\n", field.env_var, shell_quote(value)));
        docker.push_str(&format!("  -e {}={} \\\n", field.env_var, shell_quote(value)));
    }

    bash.push_str(&format!("\ncargo run --release -p connector-{connector_type}\n"));
    powershell.push_str(&format!("\ncargo run --release -p connector-{connector_type}\n"));
    docker.push_str(&format!("  {connector_type}-connector\n"));

    let _ = connector_type_label;
    (bash, powershell, docker)
}

/// POST /sensors/generate — step 3: renders ready-to-run bash, PowerShell, and `docker compose
/// run` scripts with every value the operator entered substituted in. No secret is ever
/// invented here — the API key and every connector credential comes from what the operator
/// typed into the form, never generated or stored by this handler.
///
/// Parses the urlencoded body by hand into a plain map rather than a typed `Form<T>` struct:
/// the per-connector fields are dynamic (a different set of env-var-named keys depending on
/// `connector_type`), and `serde_urlencoded`'s `#[serde(flatten)]` support for map fields is
/// unreliable enough not to depend on for something this simple to do directly.
pub async fn post_generate_script(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let pairs: Vec<(String, String)> = match serde_urlencoded::from_bytes(&body) {
        Ok(pairs) => pairs,
        Err(_) => return axum::http::StatusCode::BAD_REQUEST.into_response(),
    };
    let mut form: BTreeMap<String, String> = pairs.into_iter().collect();
    let connector_type = form.remove("connector_type").unwrap_or_default();
    let name = form.remove("name").unwrap_or_default();
    let gateway_url = form.remove("gateway_url").unwrap_or_default();
    let api_key = form.remove("api_key").unwrap_or_default();

    let label = display_name(&connector_type).unwrap_or("Unknown connector");
    let (bash_script, powershell_script, docker_command) = build_scripts(
        &connector_type,
        label,
        &name,
        session.tenant_id,
        &gateway_url,
        &api_key,
        &form,
    );
    Html(
        GenerateResultTemplate {
            show_nav: true,
            is_admin,
            connector_type_label: label,
            bash_script,
            powershell_script,
            docker_command,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
