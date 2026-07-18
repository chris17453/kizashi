# ADR-0011: Retention/Archival Service v1 scope: self-hosted S3 archival, self-owned policy store

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §6 service #12 (Retention/Archival Service) must enforce retention policy, move aged data
to Blob/S3 in the ADR-0005 NDJSON+gzip format, support reimport, and hard-delete on disposal
(spec §9). Spec §6 service #11 (Config/Admin Service) is listed as the owner of "retention
policy" configuration — but ADR-0010 deliberately did not build retention-policy CRUD there
yet, since at the time no real consumer existed. This service is that consumer, which raises
two scope questions this ADR resolves: where retention policy lives for v1, and which archival
backend to build against first.

`.env.example` already carries `AZURE_STORAGE_CONNECTION_STRING`, `AWS_S3_BUCKET`,
`AWS_REGION`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` from the initial bootstrap — placeholders
for exactly this decision, unused until now.

CLAUDE.md §2 requires end-to-end integration tests against real infrastructure, not mocks,
"since we own it" (no-vendor-lock-in principle already applied to Postgres/RabbitMQ/ClickHouse
in docker-compose). Retention/Archival Service's raw store access must also go through
Ingestion Service's HTTP API rather than touching its Postgres table directly (spec §2
principle 1, "API-mediated everything" — the same trust boundary `normalization-service`'s
`RecordClient` already established for the `record.normalized` write path).

## Decision

1. **Archival backend: S3-compatible object storage via the `aws-sdk-s3` crate, tested against
   a self-hosted MinIO container in docker-compose.** MinIO speaks the S3 API and is
   self-hostable, matching the "no vendor lock-in, self-hosted deps" pattern already used for
   Postgres/RabbitMQ/ClickHouse — a real S3-compatible backend in local/CI testing, not a
   stub. `ArchiveStore` is a trait (`write_batch`/`read_batch`), so a real AWS S3 or Azure Blob
   backend can be added later behind the same interface without touching call sites; Azure Blob
   is deliberately not built in this PR since it needs its own SDK/auth flow and MinIO already
   proves the archive format and sweep/reimport logic end-to-end against a real backend.
2. **Retention/Archival Service owns its own `retention_policies` table in v1**, with its own
   audit log (same in-same-transaction pattern as `config-admin-service`'s
   `record_audit_entry`, per CLAUDE.md §5), rather than waiting on Config/Admin Service to grow
   retention-policy CRUD first. This mirrors the deviation ADR-0010 already flagged as expected
   (trigger-engine/normalization-service reading their own local config rather than blocking on
   Config/Admin Service) — moving retention policy management into Config/Admin Service, with
   this service as an HTTP client of it, is tracked as follow-up, not silently deferred.
3. **Retention Service reaches the raw store only through Ingestion Service's HTTP API**
   (`GET /v1/records?older_than=&limit=` and `DELETE /v1/records/:id`, added to Ingestion
   Service in this same PR) — never a direct Postgres connection into Ingestion Service's
   schema, preserving spec §2 principle 1.
4. **Reimport re-feeds through Ingestion Service's `POST /v1/records` directly, not through
   Ingestion Gateway.** ADR-0005 said reimport goes "back through the Ingestion Gateway's
   normal ingestion path" — in practice Ingestion Gateway's auth model is a per-tenant API key
   held by the *connector*, which does not fit a trusted internal service replaying archived
   records for an arbitrary tenant on a schedule. Every other cross-service write in this
   codebase (Normalization Service's `RecordClient`, Trigger Engine's trigger lookups) already
   calls the owning service directly and trusts the caller, the same trust boundary Ingestion
   Gateway exists to enforce only for external/agent-facing traffic. Reimport uses that
   established internal trust boundary instead: it still re-triggers `record.ingested` →
   normalization → analysis exactly as ADR-0005 requires (replayability is unaffected), it just
   skips the external-facing hop that has no route for an internal actor. This is a deliberate,
   documented deviation from ADR-0005's literal wording, not a silent gap.
5. **Sweep/reimport are HTTP-triggered (`POST /v1/sweep`, `POST /v1/reimport`), not an
   in-process scheduler.** External scheduling (a Kubernetes CronJob or equivalent) matches how
   the spec already describes connector polling as "CronJob-scheduled" (spec §3) — keeps the
   service stateless and its core logic (`sweep`/`reimport` as plain async functions taking an
   explicit `now: DateTime<Utc>`) unit-testable without wall-clock coupling.

## Consequences

- Easier: MinIO in docker-compose gives a real, self-hosted S3-compatible integration test
  target with zero cloud credentials required for local dev/CI, consistent with every other
  infra dependency in this repo; the `ArchiveStore` trait means swapping or adding a real AWS
  S3 or Azure Blob backend later is additive, not a rewrite; owning `retention_policies` locally
  unblocks this service without a second PR against `config-admin-service`.
- Harder: retention policy configuration now lives in two places conceptually (this service's
  own table, plus the CRUD surface Config/Admin Service will eventually need for the Console
  UI's "Data lifecycle UI" per spec §7) until the follow-up migration lands — tracked, not
  hidden. Azure Blob customers cannot archive there yet; they're on AWS S3 (or an S3-compatible
  self-hosted target) until that backend is added behind the same trait. Point 4's deviation
  from ADR-0005 means a future multi-tenant SaaS reimport-by-an-external-actor scenario (a
  customer replaying their own archive via a public API) would need a real API-key-scoped route
  added later — not needed for v1's operator-triggered reimport.
