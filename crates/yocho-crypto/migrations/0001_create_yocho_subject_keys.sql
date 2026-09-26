-- Per-subject wrapped DEKs (ADR-0204). Only wrapped key material is ever stored; the plaintext
-- DEK exists solely in process memory. Shredding nulls wrapped_dek and sets shredded_at,
-- leaving a key-less tombstone so an erased subject is distinguishable from an unknown one.
CREATE TABLE IF NOT EXISTS yocho_subject_keys (
    tenant_id    UUID        NOT NULL,
    subject_type TEXT        NOT NULL,
    subject_id   TEXT        NOT NULL,
    wrapped_dek  BYTEA,
    kek_id       TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    shredded_at  TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, subject_type, subject_id),
    CONSTRAINT yocho_subject_keys_live_or_tombstone CHECK (
        (shredded_at IS NULL AND wrapped_dek IS NOT NULL AND kek_id IS NOT NULL)
        OR (shredded_at IS NOT NULL AND wrapped_dek IS NULL)
    )
);

-- The only permitted mutation is the one-way shred transition (live -> tombstone). Rows are
-- never deleted and a tombstone can never regain key material, even via direct SQL.
CREATE OR REPLACE FUNCTION yocho_subject_keys_guard()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'yocho_subject_keys rows cannot be deleted; shred instead';
    END IF;
    IF OLD.shredded_at IS NOT NULL THEN
        RAISE EXCEPTION 'yocho_subject_keys: shredded subject % is immutable', OLD.subject_id;
    END IF;
    IF NEW.shredded_at IS NULL
        OR NEW.wrapped_dek IS NOT NULL
        OR NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
        OR NEW.subject_type IS DISTINCT FROM OLD.subject_type
        OR NEW.subject_id IS DISTINCT FROM OLD.subject_id
        OR NEW.kek_id IS DISTINCT FROM OLD.kek_id
        OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'yocho_subject_keys: only the shred transition is permitted';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS yocho_subject_keys_guard ON yocho_subject_keys;
CREATE TRIGGER yocho_subject_keys_guard
    BEFORE UPDATE OR DELETE ON yocho_subject_keys
    FOR EACH ROW
    EXECUTE FUNCTION yocho_subject_keys_guard();
