-- Mirrors config-admin-service's 0009_add_dedup_fields_to_normalization_mappings.sql (ADR-0112)
-- on this service's own interim-owned copy of NormalizationMapping (ADR-0010/ADR-0018).
ALTER TABLE normalization_mappings
    ADD COLUMN dedup_fields JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN dedup_window_seconds BIGINT;
