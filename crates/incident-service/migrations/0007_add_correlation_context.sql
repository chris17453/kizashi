-- Persist the normalized event group key used by event-driven incident correlation. Existing
-- links remain valid but intentionally have no inferred key; new links can opt into correlation.
ALTER TABLE incident_events
    ADD COLUMN IF NOT EXISTS group_key TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_incident_events_group_key
    ON incident_events (group_key, incident_id);
