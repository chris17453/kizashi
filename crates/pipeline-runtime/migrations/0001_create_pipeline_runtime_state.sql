CREATE TABLE pipeline_executions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    pipeline_definition_id UUID NOT NULL,
    pipeline_version INTEGER NOT NULL CHECK (pipeline_version > 0),
    command BOOLEAN NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (length(idempotency_key) BETWEEN 1 AND 256),
    status TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK (attempt > 0),
    input JSONB NOT NULL,
    result JSONB,
    requested_at TIMESTAMPTZ NOT NULL,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    UNIQUE (tenant_id, pipeline_definition_id, idempotency_key)
);

CREATE INDEX idx_pipeline_executions_tenant_requested
    ON pipeline_executions (tenant_id, requested_at DESC);

CREATE TABLE pipeline_outbox (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    execution_id UUID NOT NULL REFERENCES pipeline_executions(id),
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ
);

CREATE INDEX idx_pipeline_outbox_unpublished
    ON pipeline_outbox (created_at) WHERE published_at IS NULL;

CREATE TABLE pipeline_confirmation_inbox (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    execution_id UUID NOT NULL REFERENCES pipeline_executions(id),
    source_event_id TEXT NOT NULL CHECK (length(source_event_id) BETWEEN 1 AND 256),
    payload JSONB NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    UNIQUE (tenant_id, source_event_id)
);
