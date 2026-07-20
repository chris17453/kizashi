-- Enforces action_executions' append-only contract (CLAUDE.md §5) at the database level. The
-- table's own comment already claimed "never updated in place — a retry is a new row," but
-- unlike every other audit-log table in the platform (auth_audit_log, config_audit_log,
-- retention_audit_log, the ingestion-gateway and egress-gateway audit logs), nothing at the DB
-- level actually enforced it — only the ExecutionRepository trait's shape (insert/list, no
-- update/delete method) did, which a bug or direct DB access could bypass.
CREATE OR REPLACE FUNCTION action_executions_reject_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'action_executions is append-only: % is not permitted', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER action_executions_immutable
    BEFORE UPDATE OR DELETE ON action_executions
    FOR EACH ROW
    EXECUTE FUNCTION action_executions_reject_mutation();
