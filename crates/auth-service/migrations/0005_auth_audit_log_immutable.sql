-- Enforces auth_audit_log's append-only contract (CLAUDE.md §5) at the database level, not just
-- by application convention: no Rust code path issues UPDATE/DELETE against this table, but
-- nothing previously stopped a bug or a manual psql session from doing so. A row-level trigger
-- rejects any attempt outright, regardless of which role or code path issues it.
CREATE OR REPLACE FUNCTION auth_audit_log_reject_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'auth_audit_log is append-only: % is not permitted', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER auth_audit_log_immutable
    BEFORE UPDATE OR DELETE ON auth_audit_log
    FOR EACH ROW
    EXECUTE FUNCTION auth_audit_log_reject_mutation();
