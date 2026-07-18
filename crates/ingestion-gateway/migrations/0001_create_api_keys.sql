-- Connector/agent API keys (spec §8: gateway-layer auth). Only the SHA-256 hash is stored —
-- the plaintext key is shown to the operator once at creation time and never persisted
-- (CLAUDE.md §5, "no secrets in code or commits" applies to runtime storage too).
CREATE TABLE api_keys (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    label TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX idx_api_keys_tenant_id ON api_keys (tenant_id);
