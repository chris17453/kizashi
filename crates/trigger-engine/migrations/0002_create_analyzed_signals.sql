-- Durable, window-queryable log of classified signals (ADR-0006). This is what
-- TriggerCondition::CountOverWindow/ThresholdOverWindow are evaluated against.
CREATE TABLE analyzed_signals (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    record_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    group_key TEXT NOT NULL,
    entity_ref TEXT NOT NULL,
    numeric_value DOUBLE PRECISION,
    source_connector_id TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_analyzed_signals_window
    ON analyzed_signals (tenant_id, event_type, group_key, occurred_at);
