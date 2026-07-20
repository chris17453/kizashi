-- Immutable audit log of every PUT /v1/allowlist config change (CLAUDE.md §5) -- distinct from
-- egress_audit_log above, which records CONNECT-attempt (proxy decision) traffic, not config
-- mutations. Same shape as config_admin_service.config_audit_log/retention_service's own audit
-- table, so this can be read back through the same generic GET /v1/audit-log/:entity_id route
-- and HttpAuditLogClient the Console UI already uses for those services -- entity_id is the
-- tenant's own id (this is a singleton-per-tenant resource, same convention AnalysisConfig
-- uses).
CREATE TABLE allowlist_audit_log (
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

CREATE INDEX idx_allowlist_audit_log_entity ON allowlist_audit_log (tenant_id, entity_id, changed_at);

CREATE OR REPLACE FUNCTION allowlist_audit_log_reject_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'allowlist_audit_log is append-only: % is not permitted', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER allowlist_audit_log_immutable
    BEFORE UPDATE OR DELETE ON allowlist_audit_log
    FOR EACH ROW EXECUTE FUNCTION allowlist_audit_log_reject_mutation();
