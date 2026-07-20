-- TOTP-based multi-factor authentication (ADR-0051). `mfa_secret` is only ever populated after
-- a successful `mfa/verify` call (never on enroll alone -- an unconfirmed secret must not be
-- able to gate login, or a typo during enrollment could lock a user out permanently).
-- `mfa_enabled` is the single source of truth `local_login` checks; a secret can exist with
-- `mfa_enabled = false` mid-enrollment.
ALTER TABLE local_users ADD COLUMN mfa_secret TEXT;
ALTER TABLE local_users ADD COLUMN mfa_enabled BOOLEAN NOT NULL DEFAULT false;

-- Short-lived server-side state bridging `local_login`'s password check and `mfa/challenge`'s
-- code check -- a Postgres table, not in-memory, since auth-service (unlike Console UI,
-- ADR-0014) has no single-instance assumption to lean on.
CREATE TABLE mfa_challenges (
    id UUID PRIMARY KEY,
    local_user_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    challenge_token TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_mfa_challenges_token ON mfa_challenges (challenge_token);
