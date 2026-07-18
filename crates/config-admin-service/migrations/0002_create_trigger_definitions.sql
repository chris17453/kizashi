-- TriggerDefinition config store, owned by Config/Admin Service (spec §5.4, ADR-0010). Not
-- yet read by Trigger Engine, which still interim-owns its own copy (ADR-0010) — that cutover
-- is a documented follow-up, not silently done here.
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

CREATE INDEX idx_trigger_definitions_tenant ON trigger_definitions (tenant_id);
