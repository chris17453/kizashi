#[path = "connector_field_catalog_test.rs"]
#[cfg(test)]
mod connector_field_catalog_test;

/// One environment variable a connector binary reads at startup (`std::env::var(...)` calls in
/// each `crates/connectors/*/src/main.rs`) — the source of truth for what the deploy-script
/// generator needs the operator to supply. Keep this in sync with the connector's actual
/// `main.rs` by hand; there's no way to derive it automatically without a macro/build-script
/// neither crate currently has.
pub struct ConnectorField {
    pub env_var: &'static str,
    pub label: &'static str,
    pub secret: bool,
    pub optional: bool,
}

const fn field(
    env_var: &'static str,
    label: &'static str,
    secret: bool,
    optional: bool,
) -> ConnectorField {
    ConnectorField { env_var, label, secret, optional }
}

/// `(connector_type, display_name)` — connector_type matches the crate name under
/// `crates/connectors/` and the `CONNECTOR_ID` default each binary falls back to, so a script
/// generated for "zendesk" lines up with `docker-compose.yml`'s `zendesk-connector` service and
/// `cargo run -p connector-zendesk`.
pub const CONNECTOR_TYPES: &[(&str, &str)] = &[
    ("zendesk", "Zendesk"),
    ("graph-mail", "Microsoft Graph (Mail)"),
    ("graph-teams", "Microsoft Graph (Teams)"),
    ("sql", "SQL"),
    ("fabric", "Fabric"),
    ("generic", "Generic"),
];

/// `(connector_type, display_name, category, short_description)` — the marketplace-style
/// catalog for the "choose a connector" step (`GET /sensors/generate`), grouped by `category`
/// there instead of a flat `<select>`. A separate list from `CONNECTOR_TYPES` rather than
/// widening that tuple, since `CONNECTOR_TYPES` is also used by Sensors' plain "register an
/// already-deployed sensor" dropdown, which has no use for category/description.
pub const CONNECTOR_CATALOG: &[(&str, &str, &str, &str)] = &[
    ("zendesk", "Zendesk", "Ticketing & Support", "Poll Zendesk tickets and comments"),
    ("graph-mail", "Microsoft Graph (Mail)", "Communication", "Poll a mailbox via Microsoft Graph"),
    (
        "graph-teams",
        "Microsoft Graph (Teams)",
        "Communication",
        "Poll a Teams channel via Microsoft Graph",
    ),
    ("sql", "SQL", "Database", "Poll rows from a SQL database via a configurable query"),
    ("fabric", "Fabric", "Database & Analytics", "Poll a Microsoft Fabric SQL analytics endpoint"),
    ("generic", "Generic", "Custom / Other", "Poll any HTTP JSON source with a bearer token"),
];

pub fn display_name(connector_type: &str) -> Option<&'static str> {
    CONNECTOR_TYPES.iter().find(|(t, _)| *t == connector_type).map(|(_, name)| *name)
}

pub fn fields_for(connector_type: &str) -> Vec<ConnectorField> {
    match connector_type {
        "generic" => vec![
            field("GENERIC_SOURCE_URL", "Source URL", false, false),
            field("GENERIC_BEARER_TOKEN", "Bearer Token", true, true),
        ],
        "sql" => vec![
            field("SQL_SOURCE_DATABASE_URL", "Database URL", true, false),
            field("SQL_QUERY", "Query", false, false),
        ],
        "zendesk" => vec![
            field("ZENDESK_SUBDOMAIN", "Subdomain", false, false),
            field("ZENDESK_EMAIL", "Email", false, false),
            field("ZENDESK_API_TOKEN", "API Token", true, false),
        ],
        "graph-mail" => vec![
            field("ENTRA_TENANT_ID", "Entra Tenant ID", false, false),
            field("ENTRA_CLIENT_ID", "Entra Client ID", false, false),
            field("ENTRA_CLIENT_SECRET", "Entra Client Secret", true, false),
            field("GRAPH_MAIL_USER_ID", "Mailbox User ID", false, false),
        ],
        "graph-teams" => vec![
            field("ENTRA_TENANT_ID", "Entra Tenant ID", false, false),
            field("ENTRA_CLIENT_ID", "Entra Client ID", false, false),
            field("ENTRA_CLIENT_SECRET", "Entra Client Secret", true, false),
            field("GRAPH_TEAMS_TEAM_ID", "Team ID", false, false),
            field("GRAPH_TEAMS_CHANNEL_ID", "Channel ID", false, false),
        ],
        "fabric" => vec![
            field("ENTRA_TENANT_ID", "Entra Tenant ID", false, false),
            field("ENTRA_CLIENT_ID", "Entra Client ID", false, false),
            field("ENTRA_CLIENT_SECRET", "Entra Client Secret", true, false),
            field("FABRIC_SQL_HOST", "SQL Analytics Endpoint Host", false, false),
            field("FABRIC_SQL_PORT", "SQL Analytics Endpoint Port", false, true),
            field("FABRIC_SQL_DATABASE", "Database", false, false),
            field("FABRIC_SQL_QUERY", "Query", false, false),
        ],
        _ => vec![],
    }
}
