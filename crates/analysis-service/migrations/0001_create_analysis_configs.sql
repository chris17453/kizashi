-- Analysis Service's first-ever Postgres schema (ADR-0019): a local, synced-via-bus copy of
-- each tenant's AI analysis prompt, kept current by consuming config-admin-service's
-- analysis_config.changed messages (same pattern as trigger-engine's trigger_definitions,
-- ADR-0018).
CREATE TABLE analysis_configs (
    tenant_id UUID PRIMARY KEY,
    prompt TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
