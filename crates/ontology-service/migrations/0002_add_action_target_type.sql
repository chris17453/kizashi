ALTER TABLE action_types
    ADD COLUMN IF NOT EXISTS target_object_type_id UUID REFERENCES object_types(id);

CREATE INDEX IF NOT EXISTS idx_action_types_target_type
    ON action_types (tenant_id, target_object_type_id);
