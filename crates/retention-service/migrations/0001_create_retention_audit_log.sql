-- Immutable audit log of every retention-policy mutation (CLAUDE.md §5), same shape/pattern as
-- config-admin-service's config_audit_log — see ADR-0011 for why this service owns its own
-- table in v1 rather than sharing config-admin-service's.
CREATE TABLE retention_audit_log (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    change_type JSONB NOT NULL,
    actor TEXT NOT NULL,
    before JSONB,
    after JSONB NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_retention_audit_log_entity ON retention_audit_log (tenant_id, entity_id, changed_at);
