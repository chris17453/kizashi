-- Local login credentials (spec §8: "local login... hashed credentials — bcrypt/argon2").
-- password_hash is a full Argon2id PHC string (embeds its own salt), never plaintext.
CREATE TABLE local_users (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    username TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX idx_local_users_tenant_username ON local_users (tenant_id, username);
