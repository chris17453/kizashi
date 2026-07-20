-- Login/auth anomaly visibility (ADR-0053): every local-login attempt, successful or not, so an
-- admin can see a brute-force pattern or a specific account under attack. `tenant_id` is nullable
-- because a failed attempt against an unknown workspace name never resolves to a real tenant_id
-- (see local_login's "same 401 for unknown workspace/username/wrong password" anti-enumeration
-- design) -- recording it as NULL still preserves the signal "someone tried a bogus workspace
-- name" without fabricating a tenant association that doesn't exist.
CREATE TABLE login_attempts (
    id UUID PRIMARY KEY,
    tenant_id UUID,
    username TEXT NOT NULL,
    success BOOLEAN NOT NULL,
    reason TEXT NOT NULL,
    attempted_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_login_attempts_tenant_time ON login_attempts (tenant_id, attempted_at DESC);

-- Append-only, same discipline as auth_audit_log (CLAUDE.md §5) -- a login attempt record must
-- never be editable or deletable, including by an admin, or it stops being trustworthy evidence.
CREATE OR REPLACE FUNCTION login_attempts_reject_mutation()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'login_attempts is append-only: % is not permitted', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER login_attempts_immutable
    BEFORE UPDATE OR DELETE ON login_attempts
    FOR EACH ROW
    EXECUTE FUNCTION login_attempts_reject_mutation();
