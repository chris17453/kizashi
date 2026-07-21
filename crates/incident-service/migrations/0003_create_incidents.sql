-- Incident config store, owned by Incident Service (ADR-0111). Groups related Events into one
-- trackable problem. v1 is manual-only: no auto-correlation writes this table yet.
CREATE TABLE incidents (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    severity TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ
);

CREATE INDEX idx_incidents_tenant ON incidents (tenant_id);
CREATE INDEX idx_incidents_tenant_status ON incidents (tenant_id, status);
