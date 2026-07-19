-- Per-tenant AI analysis prompt config (ADR-0019): one row per tenant, upserted on write.
CREATE TABLE analysis_configs (
    tenant_id UUID PRIMARY KEY,
    prompt TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
