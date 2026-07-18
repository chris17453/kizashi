//! Shared types for every Kizashi service and connector: the wire/DB schemas from spec §5,
//! plus the `Connector` trait every agent implements. This crate's schemas are the contract
//! between services published over the message bus (spec §3) — changes here ripple
//! workspace-wide, so keep it schema-stable and additive where possible.

pub mod action_execution;
pub mod agent;
pub mod analyzed_record;
pub mod bus;
pub mod connector;
pub mod db;
pub mod event;
pub mod event_type_definition;
pub mod normalization_mapping;
pub mod raw_record;
pub mod trigger_definition;

pub use action_execution::{ActionExecution, ActionExecutionStatus};
pub use agent::Agent;
pub use analyzed_record::AnalyzedRecord;
pub use bus::{
    EVENT_CREATED_EXCHANGE, RECORD_ANALYZED_EXCHANGE, RECORD_INGESTED_EXCHANGE,
    RECORD_NORMALIZED_EXCHANGE,
};
pub use connector::{Connector, ConnectorError};
pub use db::{connect_with_schema, ConnectError};
pub use event::{Event, EventStatus};
pub use event_type_definition::EventTypeDefinition;
pub use normalization_mapping::NormalizationMapping;
pub use raw_record::{RawRecord, SourceType};
pub use trigger_definition::{
    ActionRef, ActionType, ThresholdDirection, TriggerCondition, TriggerDefinition,
};
