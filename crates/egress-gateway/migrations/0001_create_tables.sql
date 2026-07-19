-- Egress Gateway's own schema (ADR-0021): a per-tenant domain allowlist it owns outright (no
-- other service reads this, so no event-driven sync case like Triggers/Agents/AnalysisConfig),
-- and an append-only audit log of every CONNECT attempt it has handled.
CREATE TABLE tenant_allowlists (
    tenant_id TEXT PRIMARY KEY,
    domains TEXT[] NOT NULL
);

CREATE TABLE egress_audit_log (
    id BIGSERIAL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    connector_id TEXT NOT NULL,
    destination_host TEXT NOT NULL,
    destination_port INTEGER NOT NULL,
    allowed BOOLEAN NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_egress_audit_log_tenant_occurred_at
    ON egress_audit_log (tenant_id, occurred_at DESC);

-- Immutability at the database level (CLAUDE.md §5), same BEFORE UPDATE OR DELETE trigger
-- pattern as config_admin_service/retention_service/ingestion_gateway's audit logs — this
-- migration/runtime role has no separate least-privilege role split anywhere in this codebase,
-- so RAISE EXCEPTION is the enforcement mechanism, not a REVOKE.
CREATE OR REPLACE FUNCTION egress_audit_log_reject_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'egress_audit_log is append-only: % is not permitted', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER egress_audit_log_immutable
    BEFORE UPDATE OR DELETE ON egress_audit_log
    FOR EACH ROW EXECUTE FUNCTION egress_audit_log_reject_mutation();
