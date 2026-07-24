CREATE TABLE action_reviews (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    invocation_id UUID NOT NULL REFERENCES action_invocations(id),
    status VARCHAR(32) NOT NULL,
    assignee VARCHAR(255),
    note TEXT NOT NULL DEFAULT '',
    reviewed_by VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, invocation_id)
);

CREATE INDEX idx_action_reviews_tenant_status ON action_reviews (tenant_id, status, updated_at DESC);
