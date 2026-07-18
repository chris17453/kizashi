-- User-facing bearer tokens (ADR-0008). Only the SHA-256 hash is stored — the plaintext token
-- is never persisted (CLAUDE.md §5). Same shape as ingestion-gateway's api_keys table; this is
-- the interim mechanism Auth Service (spec §6, service #10) will write into once it exists.
CREATE TABLE query_api_tokens (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    label TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX idx_query_api_tokens_tenant_id ON query_api_tokens (tenant_id);
