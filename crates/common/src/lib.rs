//! Shared types for every Kizashi service and connector: the wire/DB schemas from spec §5,
//! plus the `Connector` trait every agent implements. This crate's schemas are the contract
//! between services published over the message bus (spec §3) — changes here ripple
//! workspace-wide, so keep it schema-stable and additive where possible.

pub mod action_execution;
pub mod analysis_config;
pub mod analyzed_record;
pub mod bus;
pub mod connector;
pub mod db;
pub mod egress_client;
pub mod email_payload;
pub mod event;
pub mod event_type_definition;
pub mod normalization_mapping;
pub mod raw_record;
pub mod role;
pub mod saved_search_query;
pub mod sensor;
pub mod sensor_change_event;
pub mod trigger_change_event;
pub mod trigger_definition;

pub use action_execution::{ActionExecution, ActionExecutionStatus};
pub use analysis_config::{AnalysisConfig, AnalysisProvider};
pub use analyzed_record::AnalyzedRecord;
pub use bus::{
    ANALYSIS_CONFIG_CHANGED_EXCHANGE, EVENT_CREATED_EXCHANGE, MAPPING_CHANGED_EXCHANGE,
    RECORD_ANALYZED_EXCHANGE, RECORD_INGESTED_EXCHANGE, RECORD_NORMALIZED_EXCHANGE,
    SENSOR_CHANGED_EXCHANGE, TRIGGER_CHANGED_EXCHANGE,
};
pub use connector::{Connector, ConnectorError};
pub use db::{connect_with_schema, ConnectError};
pub use egress_client::{build_outbound_client, EgressClientError};
pub use email_payload::{EmailAttachment, EmailPayload};
pub use event::{Event, EventStatus};
pub use event_type_definition::EventTypeDefinition;
pub use normalization_mapping::NormalizationMapping;
pub use raw_record::{RawRecord, SourceType};
pub use role::{ParseRoleError, Role};
pub use saved_search_query::SavedSearchQuery;
pub use sensor::Sensor;
pub use sensor_change_event::SensorChangeEvent;
pub use trigger_change_event::TriggerChangeEvent;
pub use trigger_definition::{
    ActionRef, ActionType, CorrelatedCondition, ThresholdDirection, TriggerCondition,
    TriggerDefinition,
};
