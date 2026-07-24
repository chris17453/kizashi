-- A hold is an explicit, auditable override preventing retention disposal for one
-- tenant/data class until an operator releases it.
CREATE TABLE compliance_holds (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    data_class JSONB NOT NULL,
    reason TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    released_at TIMESTAMPTZ
);

CREATE INDEX idx_compliance_holds_active ON compliance_holds (tenant_id, data_class, active);
