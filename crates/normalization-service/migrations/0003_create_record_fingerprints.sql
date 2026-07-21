-- Exact-duplicate suppression state (ADR-0112). One row per (tenant_id, fingerprint) seen for
-- a mapping with dedup_fields configured. Not audit-logged like config entities -- this is
-- high-churn operational state, not operator-authored config.
CREATE TABLE record_fingerprints (
    tenant_id UUID NOT NULL,
    fingerprint TEXT NOT NULL,
    first_seen_record_id UUID NOT NULL,
    last_seen_record_id UUID NOT NULL,
    occurrence_count BIGINT NOT NULL,
    first_seen_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, fingerprint)
);
