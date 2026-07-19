-- action_executions had no tenant_id at all — a real gap against CLAUDE.md §5's "every row is
-- tenant-scoped" requirement, only surfaced now because this is the first migration to add a
-- tenant-scoped read/query path for this table (previously insert-only, no HTTP surface).
-- Existing rows predate tenant tracking on this table and have no way to be backfilled
-- (nothing else on the row identifies a tenant) — this is pre-production, so they're dropped
-- rather than carried forward with a fabricated value.
ALTER TABLE action_executions ADD COLUMN tenant_id UUID;
DELETE FROM action_executions WHERE tenant_id IS NULL;
ALTER TABLE action_executions ALTER COLUMN tenant_id SET NOT NULL;

CREATE INDEX idx_action_executions_tenant_id ON action_executions (tenant_id);
