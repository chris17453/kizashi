-- Immutable audit log of every user-management mutation (CLAUDE.md §5: "every admin/config
-- change is logged immutably... RBAC changes"). Never updated or deleted in application code —
-- one row per change, written in the same transaction as the entity mutation it records.
CREATE TABLE auth_audit_log (
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

CREATE INDEX idx_auth_audit_log_entity ON auth_audit_log (tenant_id, entity_id, changed_at);
