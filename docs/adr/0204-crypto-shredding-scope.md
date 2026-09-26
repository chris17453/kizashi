# ADR-0204: Crypto-shredding scope — Yochō data first, platform-ready design

- **Status:** proposed (default adopted for Phase 0; reversible)
- **Date:** 2026-09-26

## Context

Yochō requires erasure by per-subject keys (crypto-shredding) because signals in ClickHouse,
Parquet archives and backups cannot be cheaply rewritten. Kizashi today has a static-key
AES-256-GCM helper for config secrets, and data-subject export/delete for local users only
(ADR-0054), explicitly excluding ingested content. The plan asks: platform-wide or Yochō-only?

Legal sign-off on whether crypto-erasure satisfies GDPR/CCPA for our contracts is still open.

## Decision

1. Implement crypto-shredding in `yocho-crypto` and apply it to **Yochō-owned data** in Phase 0:
   signal provenance, thread/ticket/call skeleton participants and subjects, extracted snippets.
2. Design it platform-generic: subject keys are keyed by (tenant_id, subject_type, subject_id);
   nothing in the crate knows about churn. Adopting it for Kizashi `raw_records` is a later ADR.
3. Envelope encryption: per-subject data-encryption keys (DEKs), wrapped by a tenant key-encryption
   key held by the `KeyProvider` (ADR-0202). Wrapped DEKs are stored in Postgres; unwrapped DEKs
   are cached in memory with a short TTL so the key store stays off the hot path.
4. AEAD is AES-256-GCM, matching the existing helper. Lookups on encrypted fields use HMAC-based
   blind indexes with a per-tenant index key, never the DEK.
5. Shredding a subject deletes its wrapped DEK, writes an immutable audit row, and emits
   `subject.shredded` so caches evict. Postgres and raw buffer rows are also hard-deleted where
   cheap, per the plan's table.
6. Plaintext by design: `tenant_id`, entity ids, timestamps, numeric signal values.

## Consequences

- Losing a KEK is unrecoverable; KEK backup is part of the deployment runbook, not optional.
- Until legal sign-off, crypto-erasure is documented as "technical erasure"; hard delete stays
  available where storage allows.
