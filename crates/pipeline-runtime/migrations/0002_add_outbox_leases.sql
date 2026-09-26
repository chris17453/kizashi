ALTER TABLE pipeline_outbox
    ADD COLUMN lease_expires_at TIMESTAMPTZ,
    ADD COLUMN publish_attempts INTEGER NOT NULL DEFAULT 0 CHECK (publish_attempts >= 0),
    ADD COLUMN last_error TEXT,
    ADD COLUMN dead_lettered_at TIMESTAMPTZ;

CREATE INDEX idx_pipeline_outbox_leaseable
    ON pipeline_outbox (created_at)
    WHERE published_at IS NULL;
