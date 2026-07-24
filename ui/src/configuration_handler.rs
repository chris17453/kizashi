use crate::ontology_client;
use crate::session_guard::require_session;
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

struct ConfigCard {
    label: String,
    value: String,
    detail: String,
    href: String,
    state: String,
}

struct ConfigFlowStage {
    index: String,
    label: String,
    detail: String,
    href: String,
    state: String,
}

#[derive(Template)]
#[template(path = "configuration.html")]
struct ConfigurationTemplate {
    show_nav: bool,
    is_admin: bool,
    cards: Vec<ConfigCard>,
    flow: Vec<ConfigFlowStage>,
    errors: Vec<String>,
    egress_count: usize,
    control_good_count: usize,
    control_attention_count: usize,
}

pub async fn get_configuration(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match require_session(state.session_store.as_ref(), &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let is_admin = session.role.at_least(common::Role::Admin);
    let tenant_id = session.tenant_id;

    let (sensors, sensor_stats, triggers, mappings, retention, egress, analysis) = tokio::join!(
        state.sensors_client.list_sensors(tenant_id, 1000, 0),
        state.stats_client.connector_stats(tenant_id),
        state.triggers_client.list_triggers(tenant_id, 1000, 0),
        state.normalization_mappings_client.list_mappings(tenant_id),
        state.retention_policies_client.list_policies(tenant_id),
        state.egress_allowlist_client.get_allowlist(tenant_id),
        state.analysis_config_client.get_analysis_config(tenant_id),
    );

    let mut errors = Vec::new();
    let sensors = match sensors {
        Ok(page) => page.sensors,
        Err(error) => {
            errors.push(format!("sensors: {error}"));
            vec![]
        }
    };
    let triggers = match triggers {
        Ok(page) => page.triggers,
        Err(error) => {
            errors.push(format!("triggers: {error}"));
            vec![]
        }
    };
    let sensor_stats = match sensor_stats {
        Ok(items) => items,
        Err(error) => {
            errors.push(format!("connector health: {error}"));
            vec![]
        }
    };
    let mappings = match mappings {
        Ok(items) => items,
        Err(error) => {
            errors.push(format!("field mappings: {error}"));
            vec![]
        }
    };
    let retention = match retention {
        Ok(items) => items,
        Err(error) => {
            errors.push(format!("retention: {error}"));
            vec![]
        }
    };
    let egress = match egress {
        Ok(items) => items,
        Err(error) => {
            errors.push(format!("egress: {error}"));
            vec![]
        }
    };
    let analysis_provider = match analysis {
        Ok(Some(config)) => match config.provider {
            common::AnalysisProvider::AzureFoundry => "azure_foundry".to_string(),
            common::AnalysisProvider::OpenAiCompatible => "openai_compatible".to_string(),
        },
        Ok(None) => "not configured".to_string(),
        Err(error) => {
            errors.push(format!("AI analysis: {error}"));
            "unavailable".to_string()
        }
    };

    let sensor_count = sensors.len();
    let enabled_sensor_count = sensors.iter().filter(|item| item.enabled).count();
    let now = chrono::Utc::now();
    let stale_sensor_count = sensors
        .iter()
        .filter(|sensor| {
            if !sensor.enabled {
                return false;
            }
            sensor_stats
                .iter()
                .find(|stat| stat.connector_id == sensor.name)
                .map(|stat| now - stat.last_ingested_at > chrono::Duration::hours(1))
                .unwrap_or(true)
        })
        .count();
    let trigger_count = triggers.len();
    let enabled_trigger_count = triggers.iter().filter(|item| item.enabled).count();
    let mapping_count = mappings.len();
    let retention_count = retention.len();
    let enabled_retention_count = retention.iter().filter(|item| item.enabled).count();
    let egress_count = egress.len();
    let ontology_posture = match ontology_client::global() {
        Some(client) => {
            let (types, objects, actions) = tokio::join!(
                client.list_object_types(&session.bearer_token),
                client.list_objects(&session.bearer_token, None),
                client.list_action_types(&session.bearer_token),
            );
            match (types, objects, actions) {
                (Ok(types), Ok(objects), Ok(actions)) => {
                    Some((types.len(), objects.len(), actions.len()))
                }
                (types, objects, actions) => {
                    if let Err(error) = types {
                        errors.push(format!("ontology types: {error}"));
                    }
                    if let Err(error) = objects {
                        errors.push(format!("ontology objects: {error}"));
                    }
                    if let Err(error) = actions {
                        errors.push(format!("ontology actions: {error}"));
                    }
                    None
                }
            }
        }
        None => None,
    };
    let (ontology_type_count, ontology_object_count, action_contract_count) =
        ontology_posture.unwrap_or((0, 0, 0));
    let cards = vec![
        ConfigCard {
            label: "Connectors".into(),
            value: sensor_count.to_string(),
            detail: format!(
                "{enabled_sensor_count} enabled sensor{} · {stale_sensor_count} need{} attention",
                if enabled_sensor_count == 1 { "" } else { "s" },
                if stale_sensor_count == 1 { "s" } else { "" }
            ),
            href: "/sensors".into(),
            state: if sensor_count > 0
                && enabled_sensor_count == sensor_count
                && stale_sensor_count == 0
            {
                "good".into()
            } else {
                "risk".into()
            },
        },
        ConfigCard {
            label: "Detection rules".into(),
            value: trigger_count.to_string(),
            detail: format!(
                "{enabled_trigger_count} active trigger{}",
                if enabled_trigger_count == 1 { "" } else { "s" }
            ),
            href: "/triggers".into(),
            state: if enabled_trigger_count > 0 { "good".into() } else { "risk".into() },
        },
        ConfigCard {
            label: "Normalization".into(),
            value: mapping_count.to_string(),
            detail: "source mappings in the model".into(),
            href: "/normalization-mappings".into(),
            state: if mapping_count > 0 { "good".into() } else { "risk".into() },
        },
        ConfigCard {
            label: "AI analysis".into(),
            value: analysis_provider.clone(),
            detail: "classification policy".into(),
            href: "/analysis-config".into(),
            state: if analysis_provider == "not configured" || analysis_provider == "unavailable" {
                "risk".into()
            } else {
                "good".into()
            },
        },
        ConfigCard {
            label: "Retention".into(),
            value: format!("{enabled_retention_count}/{retention_count}"),
            detail: "policies enabled".into(),
            href: "/retention-policies".into(),
            state: if retention_count > 0 && enabled_retention_count == retention_count {
                "good".into()
            } else {
                "risk".into()
            },
        },
        ConfigCard {
            label: "Egress boundary".into(),
            value: egress_count.to_string(),
            detail: "allowed external domains".into(),
            href: "/egress-allowlist".into(),
            state: if egress_count > 0 { "good".into() } else { "risk".into() },
        },
    ]
    .into_iter()
    .chain(
        std::iter::once((ontology_type_count, ontology_object_count, action_contract_count))
            .flat_map(|(type_count, object_count, action_count)| {
                [
                    ConfigCard {
                        label: "Ontology model".into(),
                        value: object_count.to_string(),
                        detail: format!("{type_count} object types · live modeled entities"),
                        href: "/ontology".into(),
                        state: if type_count > 0 && object_count > 0 {
                            "good".into()
                        } else {
                            "risk".into()
                        },
                    },
                    ConfigCard {
                        label: "Action contracts".into(),
                        value: action_count.to_string(),
                        detail: "governed response definitions".into(),
                        href: "/actions/library".into(),
                        state: if action_count > 0 { "good".into() } else { "risk".into() },
                    },
                ]
            }),
    )
    .collect::<Vec<_>>();
    let control_good_count = cards.iter().filter(|card| card.state == "good").count();
    let control_attention_count = cards.iter().filter(|card| card.state == "risk").count();
    let flow = vec![
        ConfigFlowStage {
            index: "01".into(),
            label: "Connect".into(),
            detail: format!(
                "{enabled_sensor_count}/{sensor_count} sensors enabled{}",
                if stale_sensor_count > 0 {
                    format!(" · {stale_sensor_count} need attention")
                } else {
                    String::new()
                }
            ),
            href: "/sensors".into(),
            state: if sensor_count > 0
                && enabled_sensor_count == sensor_count
                && stale_sensor_count == 0
            {
                "good".into()
            } else {
                "risk".into()
            },
        },
        ConfigFlowStage {
            index: "02".into(),
            label: "Normalize".into(),
            detail: format!("{mapping_count} field mappings"),
            href: "/normalization-mappings".into(),
            state: if mapping_count > 0 { "good".into() } else { "risk".into() },
        },
        ConfigFlowStage {
            index: "03".into(),
            label: "Understand".into(),
            detail: format!("{analysis_provider} analysis"),
            href: "/analysis-config".into(),
            state: if analysis_provider == "not configured" || analysis_provider == "unavailable" {
                "risk".into()
            } else {
                "good".into()
            },
        },
        ConfigFlowStage {
            index: "04".into(),
            label: "Model".into(),
            detail: format!("{ontology_object_count} live ontology entities"),
            href: "/ontology".into(),
            state: if ontology_type_count > 0 && ontology_object_count > 0 {
                "good".into()
            } else {
                "risk".into()
            },
        },
        ConfigFlowStage {
            index: "05".into(),
            label: "Detect".into(),
            detail: format!("{enabled_trigger_count}/{trigger_count} triggers active"),
            href: "/triggers".into(),
            state: if enabled_trigger_count > 0 { "good".into() } else { "risk".into() },
        },
        ConfigFlowStage {
            index: "06".into(),
            label: "Respond".into(),
            detail: format!("{action_contract_count} governed contracts"),
            href: "/incidents".into(),
            state: if action_contract_count > 0 { "good".into() } else { "risk".into() },
        },
    ];

    Html(
        ConfigurationTemplate {
            show_nav: true,
            is_admin,
            cards,
            flow,
            errors,
            egress_count,
            control_good_count,
            control_attention_count,
        }
        .render()
        .unwrap(),
    )
    .into_response()
}
