-- Opt-in exact-duplicate fingerprinting config (ADR-0112). Empty dedup_fields = dedup disabled
-- for this mapping; existing rows default to that, so this is a zero-behavior-change addition.
ALTER TABLE normalization_mappings
    ADD COLUMN dedup_fields JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN dedup_window_seconds BIGINT;
