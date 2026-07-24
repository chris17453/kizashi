CREATE TABLE report_runs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    schedule_id UUID NOT NULL,
    schedule_name TEXT NOT NULL,
    recipient TEXT NOT NULL,
    format TEXT NOT NULL,
    status TEXT NOT NULL,
    error TEXT,
    artifact_url TEXT,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_report_runs_tenant_schedule ON report_runs (tenant_id, schedule_id, started_at DESC);
