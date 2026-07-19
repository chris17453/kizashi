-- RBAC v1 (ADR-0016): stores the minting user's role alongside their token so a later
-- resolution recovers both the tenant and the permission level, without a second lookup.
-- Existing rows default to 'admin', matching local_users' migration default.
ALTER TABLE query_api_tokens ADD COLUMN role TEXT NOT NULL DEFAULT 'admin';
