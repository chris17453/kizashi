ALTER TABLE record_fingerprints
    ADD COLUMN suppressed_count BIGINT NOT NULL DEFAULT 0;
