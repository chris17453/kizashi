//! Shared types for every Kizashi service and connector: the wire/DB schemas from spec §5,
//! plus the `Connector` trait every agent implements. This crate's schemas are the contract
//! between services published over the message bus (spec §3) — changes here ripple
//! workspace-wide, so keep it schema-stable and additive where possible.

pub mod action_execution;
pub mod action_template;
#[path = "action_template_test.rs"]
#[cfg(test)]
mod action_template_test;
pub mod analysis_config;
pub mod analyzed_record;
pub mod app_definition;
pub mod bus;
pub mod connector;
pub mod data_source;
pub mod db;
pub mod egress_client;
pub mod email_payload;
pub mod event;
pub mod event_type_definition;
pub mod http_metrics;
pub mod incident;
pub mod mapping_change_event;
pub mod normalization_mapping;
pub mod oauth2_http;
pub mod pipeline_definition;
pub mod pipeline_execution;
pub mod raw_record;
pub mod report_run;
pub mod role;
pub mod saved_search_query;
pub mod sensor;
pub mod sensor_change_event;
pub mod trigger_change_event;
pub mod trigger_definition;
pub mod workflow_case;

pub use action_execution::{ActionExecution, ActionExecutionStatus};
pub use action_template::{validate_action_template_config, ActionTemplate};
pub use analysis_config::{AnalysisConfig, AnalysisProvider};
pub use analyzed_record::AnalyzedRecord;
pub use app_definition::{validate_app_definition, AppDefinition};
pub use bus::{
    ANALYSIS_CONFIG_CHANGED_EXCHANGE, EVENT_CREATED_EXCHANGE, MAPPING_CHANGED_EXCHANGE,
    PIPELINE_EXECUTION_EXCHANGE, RECORD_ANALYZED_EXCHANGE, RECORD_INGESTED_EXCHANGE,
    RECORD_NORMALIZED_EXCHANGE, SENSOR_CHANGED_EXCHANGE, TRIGGER_CHANGED_EXCHANGE,
};
pub use connector::{Connector, ConnectorError};
pub use data_source::{validate_data_source, DataSource, DataSourceKind, DataSourceMode};
pub use db::{connect_with_schema, ConnectError};
pub use egress_client::{build_outbound_client, EgressClientError};
pub use email_payload::{EmailAttachment, EmailPayload};
pub use event::{Event, EventStatus};
pub use event_type_definition::EventTypeDefinition;
pub use http_metrics::{instrument_router, metrics_handler, record_request, HttpMetrics};
pub use incident::{
    Incident, IncidentNote, IncidentSeverity, IncidentStatus, ParseIncidentFieldError,
};
pub use mapping_change_event::MappingChangeEvent;
pub use normalization_mapping::NormalizationMapping;
pub use oauth2_http::{execute_oauth2_request, OAuth2HttpError};
pub use pipeline_definition::{validate_pipeline_definition, PipelineDefinition, PipelineMode};
pub use pipeline_execution::{PipelineExecution, PipelineExecutionStatus};
pub use raw_record::{RawRecord, SourceType};
pub use report_run::ReportRun;
pub use role::{ParseRoleError, Role};
pub use saved_search_query::SavedSearchQuery;
pub use sensor::Sensor;
pub use sensor_change_event::SensorChangeEvent;
pub use trigger_change_event::TriggerChangeEvent;
pub use trigger_definition::{
    ActionRef, ActionType, CorrelatedCondition, ThresholdDirection, TriggerCondition,
    TriggerDefinition,
};
pub use workflow_case::{WorkflowCase, WorkflowCaseKind, WorkflowCaseStatus};
pub mod ontology;

pub use ontology::{
    ActionInvocation, ActionType as OntologyActionType, Link, LinkType, Object, ObjectType,
};
