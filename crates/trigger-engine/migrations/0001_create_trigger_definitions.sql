-- TriggerDefinition config store (spec §5.4). v1 owns this data directly (ADR-0006 /
-- normalization-service precedent) rather than depending on Config/Admin Service.
CREATE TABLE trigger_definitions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    name TEXT NOT NULL,
    event_type_match TEXT NOT NULL,
    condition JSONB NOT NULL,
    window_seconds BIGINT NOT NULL,
    actions JSONB NOT NULL,
    enabled BOOLEAN NOT NULL
);

CREATE INDEX idx_trigger_definitions_lookup
    ON trigger_definitions (tenant_id, event_type_match, enabled);
