-- Immutable, tenant-scoped event contracts. A new schema is a new version row;
-- existing versions remain available for historical event interpretation.
CREATE TABLE event_type_definitions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    name TEXT NOT NULL,
    field_schema JSONB NOT NULL,
    version INTEGER NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name, version)
);

CREATE INDEX idx_event_type_definitions_tenant_name
    ON event_type_definitions (tenant_id, name, version DESC);
