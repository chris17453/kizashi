-- Append-only action execution audit log (spec §5.5, CLAUDE.md §5). Never updated in place —
-- a retry is a new row referencing the same trigger_id/event_id.
CREATE TABLE action_executions (
    id UUID PRIMARY KEY,
    trigger_id UUID NOT NULL,
    event_id UUID NOT NULL,
    action_type JSONB NOT NULL,
    status JSONB NOT NULL,
    executed_at TIMESTAMPTZ NOT NULL,
    detail JSONB NOT NULL
);

CREATE INDEX idx_action_executions_event_id ON action_executions (event_id);
CREATE INDEX idx_action_executions_trigger_id ON action_executions (trigger_id);
