CREATE TABLE pipeline_workflow_cases (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    execution_id UUID NOT NULL REFERENCES pipeline_executions(id),
    source_event_id TEXT,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    summary TEXT NOT NULL,
    due_at TIMESTAMPTZ,
    decided_by TEXT,
    decision_note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ,
    UNIQUE (tenant_id, execution_id, source_event_id, kind)
);

CREATE INDEX idx_pipeline_workflow_cases_queue
    ON pipeline_workflow_cases (tenant_id, status, due_at, created_at DESC);
