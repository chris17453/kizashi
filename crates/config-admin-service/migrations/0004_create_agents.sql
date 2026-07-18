-- Agent registry: the first-class record of "which connector runs for which tenant" that
-- previously didn't exist anywhere in the system — the 6 connector binaries were configured
-- only by env vars, with no service that knew of their existence, let alone let an operator
-- register/list/disable one. connector_type mirrors the crate names under
-- crates/connectors/ (zendesk, graph_mail, graph_teams, sql, fabric, generic).
CREATE TABLE agents (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    connector_type TEXT NOT NULL,
    name TEXT NOT NULL,
    config JSONB NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_agents_tenant ON agents (tenant_id);
