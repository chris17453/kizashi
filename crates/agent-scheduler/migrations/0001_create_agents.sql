-- Agent Scheduler's own read-mostly copy of the Agent registry (ADR-0020), kept current by
-- upserting/deleting on `agent.changed` bus messages — the sole source of truth remains
-- config-admin-service; this table is a synced replica used only to decide what's due to poll.
CREATE TABLE agents (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    connector_type TEXT NOT NULL,
    name TEXT NOT NULL,
    config JSONB NOT NULL,
    enabled BOOLEAN NOT NULL,
    last_polled_at TIMESTAMPTZ
);

CREATE INDEX idx_agents_enabled ON agents (enabled);
