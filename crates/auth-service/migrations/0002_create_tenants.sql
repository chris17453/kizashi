-- First-class tenant registry (spec §8). Previously every service only ever carried a bare
-- tenant_id UUID as a foreign key with nothing naming it -- fine for machine-to-machine calls,
-- unusable for a human logging in, who has no way to know or type a UUID for their own
-- workspace. This is the one place a person identifies a tenant, so it needs a name.
CREATE TABLE tenants (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
