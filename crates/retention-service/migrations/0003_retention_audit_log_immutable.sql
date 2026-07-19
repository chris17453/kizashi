-- Enforces retention_audit_log's append-only contract (CLAUDE.md §5) at the database level,
-- same rationale as config-admin-service's config_audit_log_immutable trigger.
CREATE OR REPLACE FUNCTION retention_audit_log_reject_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'retention_audit_log is append-only: % is not permitted', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER retention_audit_log_immutable
    BEFORE UPDATE OR DELETE ON retention_audit_log
    FOR EACH ROW
    EXECUTE FUNCTION retention_audit_log_reject_mutation();
