-- Append-only audit of every subject-key mutation (CLAUDE.md §5), enforced at the database
-- level like action_executions (ADR-0104): UPDATE, DELETE and TRUNCATE are rejected.
CREATE TABLE IF NOT EXISTS yocho_key_audit (
    id           UUID        PRIMARY KEY,
    tenant_id    UUID        NOT NULL,
    subject_type TEXT        NOT NULL,
    subject_id   TEXT        NOT NULL,
    action       TEXT        NOT NULL CHECK (action IN ('created', 'shredded', 'shred_repeated')),
    kek_id       TEXT,
    actor        TEXT        NOT NULL,
    occurred_at  TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX IF NOT EXISTS yocho_key_audit_subject_idx
    ON yocho_key_audit (tenant_id, subject_type, subject_id, occurred_at);

CREATE OR REPLACE FUNCTION yocho_key_audit_reject_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'yocho_key_audit is append-only: % is not permitted', TG_OP;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS yocho_key_audit_immutable ON yocho_key_audit;
CREATE TRIGGER yocho_key_audit_immutable
    BEFORE UPDATE OR DELETE ON yocho_key_audit
    FOR EACH ROW
    EXECUTE FUNCTION yocho_key_audit_reject_mutation();

DROP TRIGGER IF EXISTS yocho_key_audit_no_truncate ON yocho_key_audit;
CREATE TRIGGER yocho_key_audit_no_truncate
    BEFORE TRUNCATE ON yocho_key_audit
    FOR EACH STATEMENT
    EXECUTE FUNCTION yocho_key_audit_reject_mutation();
