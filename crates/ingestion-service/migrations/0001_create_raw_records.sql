-- RawRecord hot store (spec §5.1). Schema-on-read: raw_payload/normalized_payload are JSONB
-- and this table's shape never changes when a new source_type is added (spec §2 principle 2).
CREATE TABLE raw_records (
    id UUID PRIMARY KEY,
    connector_id TEXT NOT NULL,
    source_type JSONB NOT NULL,
    ingested_at TIMESTAMPTZ NOT NULL,
    occurred_at TIMESTAMPTZ,
    raw_payload JSONB NOT NULL,
    normalized_payload JSONB,
    tenant_id UUID NOT NULL
);

-- Every query path is tenant-scoped (CLAUDE.md §5); this index is what makes that cheap.
CREATE INDEX idx_raw_records_tenant_id ON raw_records (tenant_id);
CREATE INDEX idx_raw_records_connector_id ON raw_records (connector_id);
CREATE INDEX idx_raw_records_ingested_at ON raw_records (ingested_at);
