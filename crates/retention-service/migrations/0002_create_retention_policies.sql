-- One retention policy per (tenant_id, data_class): how long RawRecord rows are kept in the
-- hot store before being archived and hard-deleted (spec §9). `data_class` is JSONB-encoded to
-- match how config-admin-service encodes its own enums, and to leave room for future data
-- classes (normalized, event) without a schema migration (ADR-0011: v1 only enforces `raw`).
CREATE TABLE retention_policies (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    data_class JSONB NOT NULL,
    ttl_days INTEGER NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    UNIQUE (tenant_id, data_class)
);

CREATE INDEX idx_retention_policies_tenant_id ON retention_policies (tenant_id);
