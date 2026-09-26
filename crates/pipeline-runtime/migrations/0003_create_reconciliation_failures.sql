CREATE TABLE pipeline_reconciliation_failures (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    execution_id UUID NOT NULL REFERENCES pipeline_executions(id),
    source_event_id TEXT NOT NULL,
    error TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ,
    UNIQUE (tenant_id, execution_id, source_event_id)
);

CREATE INDEX idx_pipeline_reconciliation_failures_open
    ON pipeline_reconciliation_failures (tenant_id, created_at DESC)
    WHERE resolved_at IS NULL;
