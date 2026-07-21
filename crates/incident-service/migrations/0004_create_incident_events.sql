-- Many-to-many association between an Incident and the Events that make it up (ADR-0111).
-- Event itself stays owned by trigger-engine/ClickHouse; this table only stores the link.
CREATE TABLE incident_events (
    incident_id UUID NOT NULL,
    event_id UUID NOT NULL,
    linked_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (incident_id, event_id)
);

CREATE INDEX idx_incident_events_event ON incident_events (event_id);
