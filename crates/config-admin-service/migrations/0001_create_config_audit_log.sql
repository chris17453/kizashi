-- Immutable audit log of every admin/config mutation (CLAUDE.md §5). Never updated or deleted
-- in application code — one row per change, written in the same transaction as the entity
-- mutation it records.
CREATE TABLE config_audit_log (
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

CREATE INDEX idx_config_audit_log_entity ON config_audit_log (tenant_id, entity_id, changed_at);
