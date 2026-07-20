# 0058. Analysis config API key encryption at rest

## Context

ADR-0031 (adding provider/model/endpoint/api_key fields to `AnalysisConfig`) flagged this when
it shipped: "`api_key` is stored in plaintext, same as every other config-as-data field... a real
compliance gap for a product audited by customers' compliance teams." ADR-0050 closed the
*display* half of that gap — the Console UI and audit log never show the real key, only
`api_key_configured: bool` — but the database column itself stayed unencrypted. A tenant's AI
provider credential (an OpenAI/Azure key an admin typed into a form) sitting in plaintext in
`analysis_configs.api_key` is exactly the kind of finding a SOC2/ISO27001 auditor flags on sight,
regardless of what the UI does or doesn't display.

## Decision

**AES-256-GCM, encrypted at the repository boundary.** `ApiKeyEncryptor`
(`crates/config-admin-service/src/encryption.rs`) wraps a 32-byte key (loaded from
`CONFIG_ENCRYPTION_KEY`, base64-encoded in env, generated with `openssl rand -base64 32`).
`PostgresAnalysisConfigRepository` encrypts `api_key` on every `upsert` and decrypts it on every
`get`/read-before-write — no other code changes, since every caller (analysis-service's outbound
provider calls, the audit-log redaction that already existed) keeps working against the same
`Option<String>` plaintext shape as before. A fresh random nonce is generated per encryption
(GCM nonces must never repeat under a key) and stored alongside the ciphertext
(`nonce || ciphertext`, base64-encoded) in the same `TEXT` column — no schema migration needed.

Chose AES-256-GCM over a KMS/envelope-encryption scheme for v1: this app already has exactly one
place that needs a symmetric secret sourced from env (`INTERNAL_API_SECRET`), and adding a cloud
KMS dependency for one encrypted column would be a disproportionate amount of new infrastructure
for what this needs to guarantee today (ciphertext at rest, not multi-region key rotation or HSM-
backed key custody). Revisit if/when a customer's compliance requirements specifically demand
KMS-backed keys.

## Consequences

- **Losing or rotating `CONFIG_ENCRYPTION_KEY` makes every existing encrypted `api_key`
  permanently unrecoverable** — there's no key-versioning or re-encryption path in this change.
  Treat it exactly like `INTERNAL_API_SECRET`: a real production secret, backed up, never
  regenerated casually. A future key-rotation feature would need to decrypt-then-re-encrypt every
  row under the old key before the new key takes over — not built here.
- Every environment that runs `config-admin-service` (including this session's local dev stack)
  now requires `CONFIG_ENCRYPTION_KEY` to be set — no default, matching `AWS_SECRET_ACCESS_KEY`'s
  existing "fail loudly at startup rather than silently degrade" convention in
  `docker-compose.yml`.
- This closes the plaintext-at-rest gap for `analysis_configs.api_key` specifically. It does not
  address other config-as-data fields that could theoretically carry a secret in the future
  (there are none today) — if a new config entity ever needs a credential field, it should reuse
  `ApiKeyEncryptor` rather than reintroducing plaintext storage.
