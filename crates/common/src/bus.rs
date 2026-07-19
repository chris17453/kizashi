//! Message-bus topic names (spec §3): the wire contract every producer/consumer pair agrees
//! on. Defined once here rather than duplicated as local string constants in each service, so
//! a typo in one service can't silently create a second, disconnected topic.

pub const RECORD_INGESTED_EXCHANGE: &str = "record.ingested";
pub const RECORD_NORMALIZED_EXCHANGE: &str = "record.normalized";
pub const RECORD_ANALYZED_EXCHANGE: &str = "record.analyzed";
pub const EVENT_CREATED_EXCHANGE: &str = "event.created";
pub const TRIGGER_CHANGED_EXCHANGE: &str = "trigger.changed";
pub const ANALYSIS_CONFIG_CHANGED_EXCHANGE: &str = "analysis_config.changed";
