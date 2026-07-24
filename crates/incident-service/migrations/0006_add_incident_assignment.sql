ALTER TABLE incidents ADD COLUMN IF NOT EXISTS assigned_to TEXT;
CREATE INDEX IF NOT EXISTS idx_incidents_tenant_assignee ON incidents (tenant_id, assigned_to);
