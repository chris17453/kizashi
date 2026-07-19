-- Immutable audit log of every API-key lifecycle event (CLAUDE.md §5) — same shape/pattern as
-- config-admin-service's config_audit_log. API keys are the first mutable config entity
-- ingestion-gateway owns, so this table (and its immutability trigger) ship in the same PR as
-- key creation/revocation, per CLAUDE.md §5's "no follow-up" rule.
CREATE TABLE ingestion_gateway_audit_log (
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

CREATE INDEX idx_ingestion_gateway_audit_log_entity
    ON ingestion_gateway_audit_log (tenant_id, entity_id, changed_at);

CREATE OR REPLACE FUNCTION ingestion_gateway_audit_log_reject_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'ingestion_gateway_audit_log is append-only: % is not permitted', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER ingestion_gateway_audit_log_immutable
    BEFORE UPDATE OR DELETE ON ingestion_gateway_audit_log
    FOR EACH ROW
    EXECUTE FUNCTION ingestion_gateway_audit_log_reject_mutation();
