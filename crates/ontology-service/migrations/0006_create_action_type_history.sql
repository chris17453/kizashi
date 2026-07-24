CREATE TABLE action_type_history (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    action_type_id UUID NOT NULL,
    change_type VARCHAR(32) NOT NULL,
    actor VARCHAR(255) NOT NULL,
    before_state JSONB,
    after_state JSONB,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_action_type_history_lookup ON action_type_history (tenant_id, action_type_id, changed_at DESC);
