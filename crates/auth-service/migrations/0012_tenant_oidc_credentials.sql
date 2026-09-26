-- Tenant-managed OIDC provider metadata and encrypted client secret. The secret is
-- deliberately kept out of JSON/audit payloads; only auth-service can decrypt it.
CREATE TABLE tenant_oidc_providers (
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    client_id TEXT NOT NULL,
    client_secret_ciphertext BYTEA NOT NULL,
    client_secret_nonce BYTEA NOT NULL,
    auth_url TEXT NOT NULL,
    token_url TEXT NOT NULL,
    userinfo_url TEXT NOT NULL,
    redirect_url TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, provider),
    CHECK (length(provider) BETWEEN 1 AND 64)
);

CREATE INDEX tenant_oidc_providers_tenant_idx ON tenant_oidc_providers (tenant_id);
