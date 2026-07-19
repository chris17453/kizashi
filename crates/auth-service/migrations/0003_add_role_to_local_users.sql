-- RBAC v1 (ADR-0016): a role column on the user identity that already exists here — one role
-- per user per tenant, not a separate role-assignment table (see ADR for rationale).
-- Existing rows default to 'admin' so a pre-existing demo/dev user isn't locked out of write
-- paths the moment this migration runs; new users should be created with an explicit role.
ALTER TABLE local_users ADD COLUMN role TEXT NOT NULL DEFAULT 'admin';
