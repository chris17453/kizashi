-- Preserve the stable event identity used for safe partial-duplicate correlation. Existing links
-- remain valid but have no inferred identity and therefore never become partial-match candidates.
ALTER TABLE incident_events
    ADD COLUMN IF NOT EXISTS event_type TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS entity_ref TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_incident_events_identity
    ON incident_events (event_type, entity_ref, incident_id);
