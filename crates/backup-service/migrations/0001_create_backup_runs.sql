CREATE TABLE backup_runs (
    id UUID PRIMARY KEY,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    status TEXT NOT NULL,
    target TEXT NOT NULL,
    size_bytes BIGINT,
    error TEXT
);

CREATE INDEX idx_backup_runs_started_at ON backup_runs (started_at DESC);
