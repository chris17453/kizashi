-- Saved /data page search filters (spec §7 "saved queries/views", ADR-0029). Tenant-wide
-- bookmarks, not per-user (no user identity flows through sessions yet) and deliberately not
-- audit-logged like every other entity here — a bookmark has zero effect on the ingestion/
-- normalization/analysis/trigger pipeline, unlike triggers/mappings/agents/retention policies.
CREATE TABLE saved_search_queries (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    name TEXT NOT NULL,
    filter JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_saved_search_queries_tenant ON saved_search_queries (tenant_id);
