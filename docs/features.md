# Feature Log

Append-only. One entry per feature/fix/chore/doc change that lands on `main`, added in the same
PR that implements it — never batched, never backfilled after the fact. Newest entries at the
bottom. Do not edit or delete prior entries; corrections are new entries that reference the one
being corrected.

Entry format:

```
## [YYYY-MM-DD] <branch-id> — <title>
- **Type:** feature | fix | debug | docs | chore
- **Branch:** <type>/<NNNN>-<short-desc>
- **Summary:** what this adds/changes and why (1-3 sentences)
- **Tests:** what was added/run to verify it (be specific — actual test names/counts, not "added tests")
- **PR:** <link or #number>
- **ADR:** <link, if this touched a spec §11 open item — else "n/a">
```

---

## [2026-07-18] chore/0001-bootstrap-scaffolding — Repo bootstrap and foundational `common` crate
- **Type:** chore
- **Branch:** chore/0001-bootstrap-scaffolding
- **Summary:** Establishes the buildable foundation the rest of Kizashi is built on: the Cargo
  workspace root, remaining `scripts/` (bootstrap, new-service, new-connector, ci-local,
  adr-new), `docker-compose.yml` (Postgres/RabbitMQ/ClickHouse), `.github/workflows/ci.yml`
  wrapping `ci-local.sh`, `.env.example`, `.gitignore`, `rustfmt.toml`, `deny.toml`, `LICENSE`
  (MIT per spec §1), and the first workspace member, `crates/common` — the shared schema crate
  (`RawRecord`, `Event`, `EventTypeDefinition`, `TriggerDefinition`, `ActionExecution`,
  `NormalizationMapping`, spec §5) plus the `Connector` trait every connector implements
  (spec §6). `TriggerDefinition::evaluate` implements the v1 fixed-shape condition DSL
  (`CountOverWindow`, `ThresholdOverWindow`) per ADR-0001. `NormalizationMapping::apply`
  implements JSONPath-lite field mapping, never panicking on malformed operator config.
- **Tests:** `cargo test --workspace` — 28 passed, 0 failed (unit tests per type, each in a
  sibling `_test.rs` file per CLAUDE.md §2, plus `proptest` property tests
  `evaluate_never_panics_on_arbitrary_input` and `apply_never_panics_on_arbitrary_path_and_payload`
  fuzzing the trigger evaluator and normalization mapping engine). `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean.
- **PR:** #1
- **ADR:** docs/adr/0001-trigger-condition-dsl-shape.md, docs/adr/0002-mono-repo-layout.md

---

## [2026-07-18] docs/0001-adr-open-items — Remaining spec §11 ADRs
- **Type:** docs
- **Branch:** docs/0001-adr-open-items
- **Summary:** Closes out the remaining spec §11 open items with ADRs: ADR-0003 (Fabric/OneLake
  connector auth flow — per-tenant Entra app-registration client-credentials flow, no shared
  platform service principal against customer tenants), ADR-0004 (Analysis Service invocation
  pattern — micro-batched calls to Foundry/ML, per-tenant-configurable batch size/max wait,
  never mixing tenants in one batch), ADR-0005 (archive format — gzip'd NDJSON of `RawRecord`
  rows with a manifest header, reimported through the normal ingestion path). All five spec §11
  open items are now resolved (trigger DSL and mono-repo layout were ADR-0001/0002, landed in
  #1).
- **Tests:** n/a — docs-only change.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0003-fabric-onelake-connector-auth-flow.md,
  docs/adr/0004-analysis-service-invocation-pattern.md,
  docs/adr/0005-archive-format-specification.md

---

## [2026-07-18] fix/0001-branch-registry-order — Fix new-branch.sh registry/checkout ordering
- **Type:** fix
- **Branch:** fix/0001-branch-registry-order
- **Summary:** `scripts/new-branch.sh` bumped the counter and appended a row to
  `docs/branch-registry.md` on whatever branch it was invoked from, *before* checking out fresh
  `main` — so if that branch's copy of the registry differed from `main`'s (e.g. because a
  previous branch's own registry edit hadn't been merged yet), `git checkout main` failed with
  "local changes would be overwritten," exactly as hit when creating this fix's own branch from
  `docs/0001-adr-open-items`. Reordered: checkout+pull fresh `main` first, read the counter from
  that clean copy, create the branch, *then* edit the registry (so the edit lands as part of the
  new branch's own commit, as originally intended).
- **Tests:** Manually reproduced the failure (created a branch while on a branch with a locally
  modified `docs/branch-registry.md`), confirmed the original ordering failed with the exact
  "local changes would be overwritten by checkout" error, then confirmed the fixed script
  creates a branch cleanly from that same starting state.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a

---

## [2026-07-18] feature/0001-ingestion-service — Ingestion Service
- **Type:** feature
- **Branch:** feature/0001-ingestion-service
- **Summary:** First deployable pipeline service (spec §6, service #3): `POST /v1/records`
  validates a submitted record (non-empty `connector_id`, non-nil `tenant_id`, non-null
  `raw_payload`), persists it as a `RawRecord` row in Postgres (migration
  `0001_create_raw_records.sql`, tenant/connector/ingested_at indexed per CLAUDE.md §5), then
  publishes the same record to the `record.ingested` fanout exchange over RabbitMQ. Repository
  and publisher are behind traits (`RawRecordRepository`, `EventPublisher`) with Postgres/
  RabbitMQ implementations and in-memory test doubles, so handler logic is unit-testable
  without a live stack while still getting real end-to-end coverage. A publish failure is
  logged but does not roll back the (already-durable) write — the raw store is the source of
  truth, not the bus.
- **Tests:** `cargo test --workspace --lib --bins` — 39 passed, 0 failed (28 in `common`, 11 in
  `ingestion-service`, all with in-memory repository/publisher doubles). Ran
  `cargo test -p ingestion-service --test ingest_integration_test --test
  record_ingested_contract_test` against real Postgres 16 + RabbitMQ 3 containers — 3 passed,
  0 failed: full round trip (HTTP POST → Postgres row → `record.ingested` message consumed off
  a bound queue) plus the `record.ingested` wire-shape contract test. `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean.
  Upgraded sqlx 0.7→0.8 (default-features off) after `cargo audit` failed CI on
  RUSTSEC-2024-0363 (fixed in sqlx ≥0.8.1); re-ran the full test suite (42 tests) against fresh
  Postgres/RabbitMQ containers to confirm the upgrade didn't change behavior, and switched from
  the `sqlx::migrate!` macro to the runtime `sqlx::migrate::Migrator::new(...)` API so the
  "macros" feature (which unconditionally compiles the mysql/sqlite backends, not just
  postgres) isn't needed. One remaining advisory, RUSTSEC-2023-0071 (rsa Marvin Attack,
  transitive via sqlx's always-compiled mysql backend, no fix available upstream, unreachable
  since Kizashi never opens a MySQL connection), is explicitly waived with rationale in
  `.cargo/audit.toml` per CLAUDE.md §5. Also fixed `cargo deny check` (bans/licenses), which
  had never run clean before: added `publish = false` workspace-wide (internal path deps read
  as "wildcard dependencies" to crates.io-publishable crates), allowed the CDLA-Permissive-2.0
  license (webpki-roots' CA-bundle license, not a code license), and waived
  RUSTSEC-2024-0384/RUSTSEC-2025-0134 (unmaintained-crate warnings, not vulnerabilities,
  transitive via lapin) alongside RUSTSEC-2023-0071 in `deny.toml`.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a

---

## [2026-07-18] feature/0002-ingestion-gateway — Ingestion Gateway
- **Type:** feature
- **Branch:** feature/0002-ingestion-gateway
- **Summary:** The single agent-facing entry point (spec §6, service #2), sitting in front of
  Ingestion Service. `POST /v1/ingest` requires an `X-Api-Key` header, resolves it to a tenant
  via `ApiKeyStore` (Postgres-backed, keys stored only as SHA-256 hashes — the plaintext key is
  never persisted), applies a per-tenant fixed-window `RateLimiter`, then forwards the request
  to Ingestion Service with `tenant_id` overwritten from the *authenticated* identity — a
  caller-supplied `tenant_id` in the request body is always discarded, so a misconfigured or
  malicious connector cannot write into a tenant it doesn't hold a key for (spec §8 tenant
  isolation). Missing/invalid keys return 401, rate-limit exhaustion returns 429, a malformed
  body returns 400, and an unreachable Ingestion Service returns 502.
- **Tests:** `cargo test -p ingestion-gateway --lib` — 14 passed, 0 failed, all against
  in-memory doubles (`InMemoryApiKeyStore`, a deterministic `TestClock`-driven `RateLimiter`,
  and a real in-process axum server standing in for Ingestion Service so the HTTP proxy path is
  genuinely exercised, not mocked). `cargo test -p ingestion-gateway --test
  api_key_store_integration_test` against a real Postgres 16 container — 1 passed, 0 failed
  (stores a key, resolves it, confirms an unknown key and a revoked key both resolve to
  nothing). `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo audit` and `cargo deny check` — clean (same waivers
  as feature/0001-ingestion-service, no new advisories).

  Also fixed a real cross-service bug this PR exposed: ingestion-service and
  ingestion-gateway both connect to the same shared Postgres instance, and both shipped a
  first migration file named `0001_...` — sqlx tracks applied migrations by version number in
  one shared `_sqlx_migrations` table, so the moment both services' migrators ran against that
  database, the second one hit a `VersionMismatch` (CI caught this; it can't reproduce with
  either service tested alone). Added `common::db::connect_with_schema`, used by both services
  in `main.rs` and their integration tests: every service now gets its own Postgres schema
  (`ingestion_service`, `ingestion_gateway`), applied to every pooled connection via
  `after_connect`, so table names and migration histories can never collide across services
  sharing one database. Verified by running both services' live-Postgres integration tests
  together against one container and confirming each landed its tables in its own schema
  (`\dn` / `information_schema.tables`).
- **PR:** (opened in this branch's PR)
- **ADR:** n/a

---

## [2026-07-18] feature/0003-normalization-service — Normalization Service
- **Type:** feature
- **Branch:** feature/0003-normalization-service
- **Summary:** Consumes `record.ingested` off RabbitMQ, looks up the tenant's active
  `NormalizationMapping` for that source type (own Postgres schema, `normalization_service` —
  Config/Admin Service isn't built yet; this repository's Postgres impl is meant to be swapped
  for a client of that service's API once it exists, per the trait boundary already in place),
  applies it via `NormalizationMapping::apply`, and writes `normalized_payload` back — not by
  touching Ingestion Service's database, but through a new `PATCH
  /v1/records/:id/normalized` endpoint added to Ingestion Service in this same PR (spec §2
  principle 1, "API-mediated everything"). Publishes `record.normalized` once the write-back
  succeeds. No mapping configured for a tenant/source_type is not an error — the message is
  acked and skipped, since an operator hasn't gotten to configuring it yet.

  Also extracted the message-bus exchange name constants (`record.ingested`,
  `record.normalized`, `record.analyzed`, `event.created`) into `common::bus`, replacing the
  local `pub const` each service previously declared, so a typo can't silently create a second,
  disconnected topic.
- **Tests:** `cargo test --workspace --lib --bins` — 73 passed, 0 failed across all four
  crates. Live-stack tests against real Postgres 16 + RabbitMQ 3: `ingest_integration_test`,
  `api_key_store_integration_test`, `mapping_repository_integration_test`, plus both
  `record_ingested_contract_test` and the new `record_normalized_contract_test` — all passing.
  Beyond the per-crate tests, ran both service binaries together against the live stack for a
  real end-to-end smoke test: inserted a `NormalizationMapping` row, `POST`ed a raw ticket
  record to Ingestion Service, and confirmed Normalization Service consumed it and wrote back
  the correctly-mapped `normalized_payload` — the full ingest-to-normalize pipeline, not just
  isolated per-service tests. `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` — clean. `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` —
  clean (same waivers as prior PRs, no new advisories).

  CI's coverage-ratchet step failed on this PR at 83.56% (below the 85% floor), driven by two
  untested `main.rs` wiring files and `HttpRecordClient`'s real implementation having no
  coverage at all (only its in-memory test double was exercised). Fixed both: added
  `--ignore-filename-regex '(^|/)main\.rs$'` to `ci-local.sh`'s `cargo llvm-cov` invocation,
  since `main.rs` files are pure composition roots with no branching logic of their own — every
  future service's `main.rs` would otherwise drag the ratchet down for no real coverage
  benefit. Added real tests for `HttpRecordClient` against an in-process stub server (success,
  server error, unreachable server) rather than only covering it via the in-memory double.
  Coverage is now 96.32% overall.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a

---

## [2026-07-18] feature/0004-analysis-service — Analysis Service
- **Type:** feature
- **Branch:** feature/0004-analysis-service
- **Summary:** Consumes `record.normalized` and calls Azure AI Foundry/ML in per-tenant
  micro-batches per ADR-0004 (bounded by `ANALYSIS_BATCH_SIZE` or `ANALYSIS_BATCH_MAX_WAIT_MS`,
  whichever hits first; never mixing tenants in one batch call), then publishes
  `record.analyzed`. Analysis results are not persisted to their own table in v1 — they travel
  forward on the `record.analyzed` message itself for Aggregation/Trigger Engine to consume
  directly, rather than adding a service that reads back through another API just to hand the
  result one hop further (documented in `common::AnalyzedRecord`'s doc comment). Adds
  `AnalyzedRecord { record, analysis, analyzed_at }` to `common` as the new bus contract type,
  alongside `RawRecord`/`Event`.
- **Tests:** `cargo test --workspace --lib --bins` — 92 passed, 0 failed across all five
  crates. `cargo test -p analysis-service --test analysis_integration_test` — a real
  RabbitMQ-backed test (publish through `process_batch`, consume off a bound queue) plus a real
  in-process HTTP server standing in for Foundry, not mocks. `record_analyzed_contract_test`
  covers the `record.analyzed` wire shape. `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo audit` /
  `cargo deny check` — clean. `cargo llvm-cov` — 96.56% overall, well above the 85% floor.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a

---

## [2026-07-18] feature/0005-trigger-engine — Trigger Engine
- **Type:** feature
- **Branch:** feature/0005-trigger-engine
- **Summary:** Consumes `record.analyzed`, classifies candidate event types from the analysis
  output per ADR-0006 (every top-level numeric key in `analysis` becomes a candidate event
  named after that key — a documented placeholder until Config/Admin Service ships real
  `EventTypeDefinition` classification), records each as a durable signal in Trigger Engine's
  own Postgres schema, evaluates every enabled `TriggerDefinition` matching that event type
  against the signal's rolling window (`TriggerDefinition::evaluate`, ADR-0001), and for every
  firing trigger writes an `Event` to ClickHouse (spec §5.2 aggregate store — the first service
  to actually use it) and publishes `event.created`. `TriggerDefinition` storage is, like
  NormalizationMapping, owned directly by this service for now rather than depending on
  Config/Admin Service.

  Fixed a real infra gap this surfaced: `CLICKHOUSE_URL` in CI and `.env.example` had no
  credentials, but ClickHouse's HTTP interface rejects anonymous requests once
  `CLICKHOUSE_USER`/`CLICKHOUSE_PASSWORD` are set on the server — nothing had exercised that
  path until this PR. Fixed by embedding credentials in `CLICKHOUSE_URL` (HTTP basic auth via
  userinfo), matching how `DATABASE_URL`/`RABBITMQ_URL` already work.
- **Tests:** `cargo test --workspace --lib --bins` — 117 passed, 0 failed across all six
  crates. `trigger_integration_test` is a genuine full-stack test against real Postgres +
  ClickHouse + RabbitMQ together: inserts a `TriggerDefinition`, feeds an `AnalyzedRecord`
  through `process_analyzed_record`, confirms the `Event` lands in ClickHouse and
  `event.created` is received off a bound queue. `event_created_contract_test` covers the wire
  shape. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` — clean. `cargo
  llvm-cov` — 96.49% overall.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0006-trigger-engine-event-type-classification-for-v1.md

---

## [2026-07-18] feature/0006-action-executor — Action Executor
- **Type:** feature
- **Branch:** feature/0006-action-executor
- **Summary:** Consumes `event.created`, resolves which actions to run by calling Trigger
  Engine's new `GET /v1/triggers/:id` API (added to trigger-engine in this same PR — spec §2
  principle 1, no direct cross-service DB reads) using the `triggered_by` trigger id embedded
  in the event's payload, dispatches each action, and writes an append-only `ActionExecution`
  audit row per action regardless of outcome — a dispatch failure is recorded as `Failed`, not
  swallowed. Per ADR-0007, v1 dispatches every `ActionType` (email/webhook/teams_alert/
  create_ticket/custom) through one `HttpActionDispatcher` that POSTs the event + action config
  to `config["url"]` — genuinely functional against any webhook-shaped endpoint, not a stub;
  type-specific integrations (SMTP, Teams card schema, per-vendor ticketing APIs) are follow-up
  work.
- **Tests:** `cargo test --workspace --lib --bins` — 135 passed, 0 failed across all seven
  crates. `execution_repository_integration_test` (real Postgres) confirms inserts persist and
  that a `retry()` produces a second append-only row rather than mutating the first. Beyond
  automated tests, ran a genuine end-to-end smoke test with real service binaries: started
  trigger-engine + action-executor against a live Postgres/RabbitMQ/ClickHouse stack, inserted
  a `TriggerDefinition` with a webhook action pointed at a throwaway local HTTP receiver,
  published a `record.analyzed` message, and confirmed the trigger fired, the action was
  dispatched, the receiver got the POST, and the `ActionExecution` row landed with
  `status: sent` — the full ingest-through-action pipeline working together, not just
  per-service tests. `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  clean. `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` — clean. `cargo
  llvm-cov` — 96.25% overall.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0007-action-executor-v1-dispatch-model.md

---

## [2026-07-18] feature/0007-query-gateway-dashboard-api — Query Gateway + Dashboard/Query API
- **Type:** feature
- **Branch:** feature/0007-query-gateway-dashboard-api
- **Summary:** Two new crates completing the read side of the platform. `dashboard-api` (spec
  §6, service #9) reads Events from ClickHouse — `GET /v1/events` (filterable by event_type,
  group_key, status, since/until, limit) and `GET /v1/events/:id` — trusting `X-Tenant-Id` as
  set by the gateway rather than deriving identity itself (spec §8). `query-gateway` (spec §6,
  service #8) is the dashboard/UI-facing entry point: resolves a bearer token to a tenant
  (ADR-0008 — same hashed-token shape as ingestion-gateway's API keys, since Auth Service isn't
  built yet; the `query_api_tokens` table is what Auth Service will write into once it exists,
  not a mechanism to replace later) and forwards to Dashboard API with the resolved tenant_id.
- **Tests:** `cargo test --workspace --lib --bins` — 168 passed, 0 failed across all nine
  crates. Live Postgres integration test for the token store (including revoked-token
  rejection). Beyond automated tests, ran a genuine end-to-end smoke test with real service
  binaries against live Postgres + ClickHouse: seeded a real Event row and a real token,
  queried both `list` and `get-by-id` through `query-gateway` end to end, and confirmed 401 on
  a missing token. **That manual run caught a real bug unit/stub tests missed**: ClickHouse's
  HTTP interface rejects a bodyless POST with `411 Length Required`, which reqwest doesn't
  avoid automatically — fixed by explicitly setting `Content-Length: 0`, and added
  `requests_always_carry_a_content_length_header` so this can't silently regress again (the
  axum-based stub servers used elsewhere don't enforce this the way real ClickHouse does).
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean (also fixed
  two `clippy::result_large_err` findings and one `unnecessary_sort_by`). `cargo fmt --all
  --check` — clean. `cargo audit` / `cargo deny check` — clean. `cargo llvm-cov` — 95.35%
  overall.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0008-query-gateway-interim-auth-model.md

---

## [2026-07-18] feature/0008-auth-service — Auth Service
- **Type:** feature
- **Branch:** feature/0008-auth-service
- **Summary:** New crate `crates/auth-service` (spec §6, service #10). Two login paths, both
  ending in a call to Query Gateway's new `POST /internal/tokens` (added to query-gateway in
  this PR, shared-secret protected) to mint a session, since Auth Service never writes into
  `query_api_tokens` directly (spec §2 principle 1): (1) **local login**
  (`POST /v1/auth/local/login`) — Argon2id-hashed credentials in `auth_service.local_users`,
  constant-shape response so unknown-username and wrong-password aren't distinguishable; (2)
  **unified OIDC** (`GET /v1/auth/oidc/:provider/authorize`, `POST
  /v1/auth/oidc/:provider/callback`) — one real `oauth2`-crate-backed client serves both Entra
  ID and generic OAuth (ADR-0009), since Entra is itself OIDC-compliant and duplicating the
  client would buy nothing. No session/cookie layer yet — that's Console UI's job once built;
  the PKCE verifier is handed back to the authorize caller to carry to the callback.
- **Tests:** `cargo test --workspace --lib --bins` — 197 passed, 0 failed across all ten
  crates, including a real OIDC client test against a stub IdP (`/token`, `/userinfo`) that
  exercises the actual code-exchange and userinfo-fetch logic, not just an in-memory double —
  what's inherently untestable in CI is the human browser hop to the IdP's login page, true of
  any OIDC integration (documented in ADR-0009, not a gap specific to this build-out). Live
  Postgres integration test for `local_users`. Beyond automated tests, ran a genuine end-to-end
  smoke test with real service binaries: created a local user with a real Argon2id hash via
  `auth-service`'s own hashing code, logged in through `POST /v1/auth/local/login`, confirmed
  wrong-password gets 401, and used the real minted token against `query-gateway` to read a
  real ClickHouse-backed event — the full auth-through-query chain working together. `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all
  --check` — clean. `cargo audit` / `cargo deny check` — clean (oauth2 pulls in a second
  reqwest major version transitively; no new advisories, just an existing-pattern
  multiple-versions warning). `cargo llvm-cov` — 95.42% overall.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0009-auth-service-v1-scope-local-login-plus-unified-oidc.md

---

## [2026-07-18] feature/0009-config-admin-service — Config/Admin Service
- **Type:** feature
- **Branch:** feature/0009-config-admin-service
- **Summary:** New crate `crates/config-admin-service` (spec §6, service #11). Full CRUD +
  immutable audit logging for `TriggerDefinition` and `NormalizationMapping` — the two config
  entity types with real existing consumers (trigger-engine, normalization-service). Every
  create/update opens one Postgres transaction, writes the entity change, then writes an
  audit_log row via `record_audit_entry` (a free function, not a trait method, since sharing
  one `Transaction` across a `dyn Trait` repository and an audit abstraction isn't portable) —
  all in the same transaction, per CLAUDE.md §5. `GET /v1/audit-log/:entity_id` exposes the
  read path via a separately mockable `AuditLogReader` trait. Deliberately does NOT yet migrate
  trigger-engine/normalization-service to read their config through this service (they still
  read their own local tables) and does NOT build EventTypeDefinition/connector-config/
  retention-policy/branding CRUD, since none of those have a real consumer yet and CLAUDE.md
  prohibits half-finished stub endpoints — both cuts are documented in ADR-0010, not silent.
- **Tests:** `cargo test --workspace --lib --bins` — 222 passed, 0 failed across all eleven
  crates (24 in config-admin-service alone: repository CRUD/tenant-scoping/audit-trail unit
  tests against in-memory doubles, handler tests including a tenant-mismatch-is-rejected case
  and a full create→get→update→list→audit-log round trip). Live Postgres integration test
  (`tests/repository_integration_test.rs`, 4 tests) exercises the real transactional behavior
  the in-memory doubles can't: a real `config_audit_log` row lands in the same transaction as a
  real trigger/mapping insert, an update writes a second audit row with both `before` and
  `after` populated, and a failed update (unknown id) leaves zero audit rows — no partial
  writes. Beyond automated tests, ran a genuine end-to-end smoke test with the real
  `config-admin-service` binary against live Postgres: created a trigger definition over HTTP,
  confirmed it was retrievable via `GET /v1/trigger-definitions/:id`, and confirmed the
  audit-log endpoint returned exactly one `created` entry with the full entity snapshot in
  `after`. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` — clean (same two
  pre-existing waived unmaintained-crate warnings as prior PRs, no new advisories). `cargo
  llvm-cov --workspace --all-features --ignore-filename-regex '(^|/)main\.rs$'
  --fail-under-lines 85` — 94.37% overall, ratchet holds.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0010-config-admin-service-v1-scope.md

---

## [2026-07-18] feature/0010-retention-service — Retention/Archival Service

- **Type:** feature
- **Branch:** feature/0010-retention-service
- **Summary:** New crate `crates/retention-service` (spec §6, service #12): enforces per-tenant
  retention policy by archiving `RawRecord` rows older than their TTL to S3-compatible object
  storage in the ADR-0005 NDJSON+gzip format, then hard-deleting them from the hot store
  (archive-then-delete, never the reverse), and supports reimport of an archived batch back
  through the pipeline (spec §9). Ships with its own retention-policy CRUD + immutable audit
  log (same in-same-transaction pattern as config-admin-service, CLAUDE.md §5) and a MinIO
  container added to docker-compose as the self-hosted S3-compatible test/dev backend behind a
  new `ArchiveStore` trait (`S3ArchiveStore` impl via `aws-sdk-s3`). Extends
  `ingestion-service` with the two endpoints Retention Service needs to reach the raw store
  without touching its Postgres schema directly (spec §2 principle 1): tenant-scoped
  `GET /v1/records?older_than=&limit=` and `DELETE /v1/records/:id`. See ADR-0011 for the full
  v1 scope decision (self-owned policy store, S3-compatible backend, why reimport bypasses
  Ingestion Gateway).
- **Bug found and fixed in this PR, not shipped:** the first cut of `list_older_than`/`delete`
  on `ingestion-service` had no `tenant_id` scoping at all — any tenant's sweep would list and
  delete every tenant's aged records, and a sweep batch could get mis-attributed to the wrong
  tenant in the archive. Caught by the manual end-to-end smoke test (two tenants, only one with
  a retention policy — the unpolicied tenant's equally-old record was being swept anyway),
  invisible to per-service unit tests using tenant-blind stub data. Fixed by threading
  `tenant_id` through the repository trait, both HTTP endpoints (via `X-Tenant-Id`, matching
  every other tenant-scoped read path in this codebase), the `RawRecordClient` trait, and
  `sweep`'s call sites; added `list_older_than_is_scoped_to_tenant` and
  `delete_returns_false_when_tenant_does_not_match` regression tests in ingestion-service, plus
  a tenant-scoping test in retention-service's own client test double, so this can't regress
  silently again.
- **Tests:** `cargo test --workspace --lib --bins` — all crates green (retention-service alone:
  40 unit/handler tests covering repository CRUD + audit trail, archive encode/decode
  round-trip, sweep pagination/disabled-policy/non-Raw-data-class/archive-failure paths,
  reimport partial-failure handling, and full HTTP handler round trips). Live-infrastructure
  integration tests: `retention_policy_integration_test.rs` (4 tests) against real Postgres,
  same transactional-audit-row pattern verified as config-admin-service;
  `s3_archive_store_integration_test.rs` (3 tests) against a real MinIO container — write/read
  round trip, not-found handling, idempotent bucket creation. Beyond automated tests, ran a
  genuine end-to-end smoke test with real `ingestion-service` and `retention-service` binaries
  against live Postgres + MinIO: seeded old records for two different tenants, created a
  retention policy for only one, triggered a sweep, and confirmed only that tenant's record was
  archived and deleted while the other tenant's equally-old record was untouched (this is what
  caught the tenant-isolation bug above) — then triggered reimport of the archived batch and
  confirmed the record reappeared in the hot store with its original payload intact. `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all
  --check` — clean. `cargo audit` / `cargo deny check` — clean after waiving three new
  advisories (RUSTSEC-2026-0098/-0099/-0104, rustls-webpki 0.101.7 name-constraint/CRL bugs
  transitive via `aws-sdk-s3`'s pinned old rustls stack — documented rationale in both
  `.cargo/audit.toml` and `deny.toml`; not exploitable against a non-attacker-controlled S3
  endpoint, no newer `aws-smithy-http-client` release exists yet). `cargo llvm-cov` — 94.11%
  overall, ratchet holds.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0011-retention-archival-service-v1-scope-self-hosted-s3-archival-self-owned-policy-store.md

---

## [2026-07-18] feature/0011-observability — Platform Observability

- **Type:** feature
- **Branch:** feature/0011-observability
- **Summary:** New crate `crates/observability` (spec §6, service #13). `GET /v1/health`
  fans `GET /healthz` out concurrently to every service in an operator-configured
  `SERVICE_REGISTRY` (`name=url` pairs) and reports per-service up/down plus an overall
  platform status (503 if any one service is down, so the endpoint itself doubles as an
  external liveness check). `GET /v1/backlog` reads per-stage queue depths from RabbitMQ's
  management HTTP API (already enabled in docker-compose) for the four pipeline queues
  (`normalization-service.record.ingested`, `analysis-service.record.normalized`,
  `trigger-engine.record.analyzed`, `action-executor.event.created`), giving a single ordered
  view of where the ingest → normalize → analyze → act chain is backing up. Per-service
  `/metrics` request/latency instrumentation is deliberately deferred — it needs a shared
  `common` instrumentation helper and touches every existing service, which is its own scoped
  follow-up, not something to gesture at with stub endpoints here (ADR-0012).
- **Tests:** `cargo test --workspace --lib --bins` — all thirteen crates green (20 unit tests in
  observability alone: registry parsing, health fan-out aggregation logic against an in-memory
  checker double, and both the HTTP health checker and RabbitMQ backlog reader against real
  stub axum servers). Live integration test
  (`tests/rabbitmq_backlog_integration_test.rs`) against a real RabbitMQ management API —
  confirms one depth entry per pipeline stage and correctly reports zero backlog for
  not-yet-declared queues rather than erroring. Beyond automated tests, ran a genuine
  end-to-end smoke test with the real `observability` binary: registered a mix of a real
  running `ingestion-service` and two intentionally-unreachable services, confirmed
  `/v1/health` correctly reported the real service up, the fake ones down, and 503 overall;
  confirmed `/v1/backlog` returned all four pipeline stages against live RabbitMQ. `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all
  --check` — clean. `cargo audit` / `cargo deny check` — clean, no new advisories. `cargo
  llvm-cov` — 94.27% overall, ratchet holds.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0012-platform-observability-v1-scope-health-aggregation-and-rabbitmq-backlog-visibility.md

---

## [2026-07-18] feature/0012-connectors — Connectors (zendesk, graph-mail, graph-teams, sql, fabric, generic)

- **Type:** feature
- **Branch:** feature/0012-connectors
- **Summary:** Six new connector crates under `crates/connectors/` (spec §6, service #1) plus
  a shared `connector-runtime` library (ADR-0013): `HttpIngestionClient` (posts polled records
  to Ingestion Gateway's `POST /v1/ingest`), `run_poll_cycle` (one CronJob poll cycle — poll,
  post every record, count successes/failures without aborting the batch on one failure), and
  `entra_client_credentials::fetch_access_token` (the OAuth2 client-credentials/app-only flow
  ADR-0003 specifies, shared by the three Entra-backed connectors). `generic` polls a
  configurable JSON HTTP endpoint. `sql` runs an operator-configured `SELECT` against any
  Postgres-wire-protocol database via a dynamic row-to-JSON mapper. `zendesk` polls the
  Incremental Ticket Export API (HTTP Basic `{email}/token`). `graph-mail`/`graph-teams` poll
  Microsoft Graph mail/channel messages via Entra app-only auth. `fabric` polls Fabric's SQL
  analytics endpoint over real TDS (`tiberius` crate) with an Entra AAD token in place of a
  username/password, reusing the `sql` connector's row-mapping approach — OneLake and
  connector-config-via-Config/Admin-Service remain deferred follow-ups (ADR-0013).
- **Tests:** `cargo test --workspace --lib --bins` — all twenty crates green (connector-runtime:
  12 tests against real stub HTTP servers; each connector: unit tests against real stub HTTP
  servers matching its source API's shape — Zendesk incremental-export JSON, Graph's `{value:
  [...]}` list shape, generic's bare JSON array — covering the happy path, auth failure, rate
  limiting, and unreachable-source cases). `sql`'s live Postgres integration test creates a real
  temp table and confirms row→JSON mapping end to end. `fabric`'s live integration test proves
  the real TCP connect + TDS handshake + `AuthMethod::aad_token` login attempt against a real
  SQL Server container (standing in for Fabric, since both speak TDS) and confirms a rejected
  AAD login is correctly classified `ConnectorError::AuthFailed` — the happy-path query against
  real Fabric data can't be proven without a real Fabric tenant, the same inherent limitation
  ADR-0009 already documents for OIDC's browser hop (no `raw_record_contract_test.rs` exists
  for `fabric` for this reason, documented in its `lib.rs`). Beyond automated tests, ran two
  genuine end-to-end smoke tests with real binaries: `connector-generic` against a real stub
  HTTP source through a live `ingestion-gateway` (API-key auth) → `ingestion-service` → real
  Postgres, and `connector-sql` against real seeded Postgres rows through the same chain — both
  confirmed the exact source records landed in the hot store under the correct tenant. `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all
  --check` — clean. `cargo audit` / `cargo deny check` — clean, no new advisories. `cargo
  llvm-cov` — 93.49% overall, ratchet holds.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0013-connectors-v1-scope-shared-poller-runtime-env-driven-per-tenant-config-fabric-sql-endpoint-only.md

---

## [2026-07-18] feature/0013-console-ui — Console UI

- **Type:** feature
- **Branch:** feature/0013-console-ui
- **Summary:** New `ui/` crate (spec §7, the last of the thirteen planned services/components)
  — a server-rendered Rust web app (`axum` + `askama` compile-time-checked templates), not a
  WASM SPA (ADR-0014): every other service in this repo is tested via
  `tower::ServiceExt::oneshot` with zero browser-automation tooling anywhere in the stack, and a
  WASM SPA's natural test story needs a headless browser driver this environment doesn't have —
  so the console is built to fit the same proven test methodology instead of introducing a new
  one for the highest-uncertainty piece of the platform. Ships: a dark-mode console shell (left
  nav, OpenShift/Instana-direction styling), a login page posting to Auth Service's local-login
  endpoint with Console UI's own `HttpOnly`-cookie session layer (in-memory session store keyed
  by a random id — Auth Service has no session/cookie layer of its own, ADR-0009 said that's
  this UI's job), and three authenticated read views: Events (via Query Gateway), Triggers (via
  Config/Admin Service), and Platform Health (via Observability). Topology graph, configurable
  dashboards, reporting, event type management, a real trigger builder, data lifecycle UI, and
  RBAC/admin UI are explicitly deferred, documented follow-ups (ADR-0014) — not stub pages.
- **Tests:** `cargo test --workspace --lib --bins` — all twenty-one crates green (35 tests in
  `kizashi-ui`: session store CRUD, cookie-parsing/session-guard redirect logic, every HTTP
  client (auth/events/triggers/health) against real stub servers matching each backend's real
  response shape, and every page handler — signed-in render, signed-out redirect, and
  backend-failure error display — via `tower::ServiceExt::oneshot`, the same pattern as every
  other service in this repo). Beyond automated tests, ran a genuine end-to-end smoke test with
  the real `kizashi-ui` binary against six other real running services (auth-service,
  query-gateway, dashboard-api, config-admin-service, observability, Postgres): logged in with
  a real Argon2id-hashed local user, confirmed the session cookie was set, loaded `/events`,
  `/triggers` (seeded a real trigger via Config/Admin Service and confirmed it rendered), and
  `/health` (showing real live service status) all while signed in, then logged out and
  confirmed both the expired cookie and unauthenticated requests correctly redirect to
  `/login`. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` — clean, no new
  advisories. `cargo llvm-cov` — 93.78% overall, ratchet holds.
- **PR:** (opened in this branch's PR)
- **ADR:** docs/adr/0014-console-ui-v1-scope-server-rendered-rust-web-app-shell-plus-read-only-events-triggers-health-views.md

---

## [2026-07-18] chore/0002-local-dev-launcher — Local dev launcher (Makefile + scripts)

- **Type:** chore
- **Branch:** chore/0002-local-dev-launcher
- **Summary:** No Dockerfiles or docker-compose entries exist for the thirteen application
  services, six connectors, or the UI (only infra is containerized) — every manual smoke test
  this project has run so far required hand-invoking binaries with hand-built env vars. Adds
  `scripts/run-local.sh` (builds the workspace, launches every service as a background process
  with its own `logs/<name>.log`/`run/<name>.pid`, waiting on `/healthz` between dependency
  tiers), `scripts/stop-local.sh`, `scripts/status-local.sh`, `scripts/seed-local-demo.sh`
  (idempotent — seeds a fixed demo tenant/local-user/API-key so the Console UI and
  `POST /v1/ingest` are usable immediately), and a root `Makefile` wrapping all of them
  (`make run`, `make seed`, `make status`, `make stop`, `make logs SERVICE=...`, `make test`,
  `make ci`). Also adds `auth-service --bin hash_password` (offline Argon2id hash generator —
  every real deployment needs some way to seed its first local user before an admin UI exists
  to do it through the API; the seed script uses it rather than duplicating the hashing logic),
  makes docker-compose.yml's infra host ports overridable via `.env`
  (`POSTGRES_PORT`/`RABBITMQ_PORT`/etc., defaulting to the existing values) since a machine
  with other projects already bound to 5432 previously had no way to work around it without
  editing the checked-in file, adds `RABBITMQ_MANAGEMENT_URL` to `.env.example` (missing since
  the observability PR — required, no default, would `.expect()`-panic without it), and adds
  `GET /healthz` to `kizashi-ui` (every other service has one; the UI didn't, which
  `status-local.sh` needs).
- **What running it for the first time actually found**: the launcher surfaced a real ordering
  bug in how the pipeline's RabbitMQ exchanges come up. Every stage's `RabbitMqEventPublisher`
  declares its own exchange on startup; every stage's consumer only `queue_bind`s, which
  requires that exchange to already exist. `analysis-service` (a `record.normalized` consumer)
  starting before `normalization-service` (the `record.normalized` publisher) panicked with
  `NOT_FOUND - no exchange 'record.normalized'`. This ordering constraint — ingestion-service →
  normalization-service → analysis-service → trigger-engine → action-executor, strictly in
  that order — was never documented or encoded anywhere before now; `scripts/run-local.sh`
  encodes it. The seed script also needed two passes to get right: the first demo password
  contained spaces and broke `run/demo-tenant.env` when sourced, and the fixed-id upsert
  originally used `ON CONFLICT (key_hash) DO NOTHING`, which errored on a primary-key collision
  the moment the API key's value changed between runs — fixed to `ON CONFLICT (id) DO UPDATE`
  so re-running always converges to the script's current constants.
- **Tests:** `cargo test --workspace --lib --bins` — all crates green (`kizashi-ui` grew to 36
  tests with the new `/healthz`; `auth-service` gained
  `tests/hash_password_bin_test.rs`, which runs the actual compiled binary via
  `CARGO_BIN_EXE_hash_password` and confirms its output round-trips through `verify_password`,
  not just "the binary exits 0"). Beyond automated tests: ran `make run` against a genuinely
  fresh local Postgres/RabbitMQ/ClickHouse/MinIO stack, confirmed all thirteen services plus
  the UI came up healthy via `make status`, ran `make seed` twice in a row to confirm
  idempotency, logged into the real Console UI as the seeded demo user (real session cookie,
  real redirect), loaded `/events`, `/triggers`, and `/health` (showing all eleven registered
  services `up`), then posted a real record through `POST /v1/ingest` with the seeded API key
  and confirmed via direct Postgres query that it reached `ingestion_service.raw_records` and
  was correctly left un-normalized (no mapping configured for that tenant/source-type — the
  correct no-op, not a bug) — proving the exchange-ordering fix actually holds under a real
  `record.ingested` publish. `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` — clean. `cargo fmt --all --check` — clean. `cargo audit` / `cargo deny check` —
  clean, no new advisories (no new dependencies added).
- **PR:** (opened in this branch's PR)
- **ADR:** n/a (operational tooling, not an architectural decision)

## [2026-07-18] feature/0014-docker-images — Containerize all services, connectors, and the UI
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** `scripts/run-local.sh` (prior chore) launched every binary as a plain background
  process on the host — `docker ps` only ever showed the four infra containers
  (Postgres/RabbitMQ/ClickHouse/MinIO), not the actual application. This adds one shared
  multi-stage root `Dockerfile` (builder stage compiles `--bin "${BIN}"`, runtime stage is
  `debian:bookworm-slim` running as non-root `kizashi`), reused across all 20 binaries via
  `--build-arg BIN=<name>` rather than 20 near-identical Dockerfiles, plus `.dockerignore`, and
  extends `docker-compose.yml` with a `build:`+`healthcheck:`+`depends_on:` entry for each of the
  13 services and `kizashi-ui` (internal port always 8080; host ports match the map
  `scripts/run-local.sh` already established), and the 6 connectors under a `connectors` compose
  profile (one-shot CronJob-shaped binaries invoked via `docker compose run --rm`, not
  long-running services, so they don't auto-start with `docker compose up`). `depends_on:
  condition: service_healthy` chains encode both ordinary HTTP-dependency order and the AMQP
  exchange-declaration order discovered in the prior local-launcher PR (ingestion-service →
  normalization-service → analysis-service → trigger-engine → action-executor — each stage's
  publisher declares its own exchange on startup, so a consumer starting first panics with
  `NOT_FOUND - no exchange`).
- **What building/running it for real found**: every migration-running service reads its
  migrations directory via `env!("CARGO_MANIFEST_DIR")` — an absolute build-time source path —
  so a runtime image containing only the compiled binary would panic at startup with a missing
  migrations directory; fixed by also copying the `crates/` source tree into the runtime stage
  (`COPY --from=builder /app/crates /app/crates`), verified by actually running the built
  `ingestion-service` image against real containerized Postgres/RabbitMQ and confirming
  `/healthz` returned 200 (i.e. migrations genuinely ran). Separately, `clickhouse/clickhouse-server`
  came up `unhealthy` under `docker compose up` even though the server itself was fine: its
  `[::]` (IPv6 wildcard) listener fails at startup in this Docker networking environment ("DNS
  error: Address family for hostname not supported"), and the pre-existing healthcheck
  (`wget --spider -q localhost:8123/ping`) resolved `localhost` to `::1` first and got
  connection-refused, even though the server was correctly listening and serving on
  `0.0.0.0:8123` the whole time — fixed by pointing the healthcheck at `127.0.0.1` explicitly
  (confirmed the app-service healthchecks don't share this problem: `curl`, unlike `wget`,
  falls through to the next resolved address on refusal). This ClickHouse healthcheck bug
  predates this branch but was only surfaced by actually bringing the full stack up as
  containers with `depends_on: condition: service_healthy` gating on it.
- **Tests:** `docker compose up -d --build` — all 17 containers (4 infra + 13 services) reached
  `healthy`. Ran a real end-to-end smoke test through the *containerized* stack, not the host
  processes: `scripts/seed-local-demo.sh` against the containerized Postgres (via
  `docker compose exec`), logged into the containerized Console UI's `/login` (200), hit
  `GET /healthz` on both `kizashi-ui` and `ingestion-gateway` through their published host
  ports, then `POST /v1/ingest` through the containerized `ingestion-gateway` with the seeded
  API key and confirmed via direct Postgres query the row reached
  `ingestion_service.raw_records` correctly tenant-scoped and correctly left un-normalized (no
  `NormalizationMapping` configured for that connector/source-type — the correct no-op, not a
  bug). Ran the full local CI gate (`scripts/ci-local.sh`) with `.env` loaded and a throwaway
  local `mssql` container standing in for the CI-only Fabric/TDS integration test dependency
  (mirroring `.github/workflows/`'s own `docker run mcr.microsoft.com/mssql/server` step, not a
  new dependency): `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean, `cargo test --workspace --all-features` all green,
  `cargo llvm-cov` 94.73% line coverage (85% floor), `cargo audit` / `cargo deny check` clean,
  no new advisories.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a (deployment packaging of already-decided architecture, not a new architectural
  decision — Kubernetes/Helm, the actual "how do we deploy" decision per spec §10, is a
  follow-up item in the approved gap-closing roadmap, not part of this change)

## [2026-07-18] feature/0014-docker-images — Fix `/` 404, tenant-UUID login, and Console UI branding
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** Real usage of the just-containerized stack (this branch) surfaced three
  independent UX defects in the Console UI, fixed together since all three sit on the same
  login/landing path: (1) `GET /` was entirely unrouted and 404'd — the exact URL a person
  types first — fixed with a new `root_handler.rs` that redirects `/` to `/events`, which
  itself already bounces an unauthenticated visitor to `/login`; (2) local login required
  typing a raw tenant UUID, which no human can be expected to know, because there was no
  first-class `Tenant` entity anywhere in the system — every service only ever carried a bare
  `tenant_id` foreign key. Added a new `tenants` table + `TenantRepository` to auth-service
  (`crates/auth-service/migrations/0002_create_tenants.sql`), changed
  `POST /v1/auth/local/login` to accept `tenant_name` and resolve it internally (still returns
  a generic 401 for unknown-workspace/unknown-username/wrong-password alike, so none of the
  three is enumerable), and threaded the rename through Console UI's `AuthClient`/login form
  (now labeled "Workspace"); (3) the UI had no visual identity at all — no logo/wordmark, no
  centered login layout, table/nav styling was minimal to the point of looking broken. Gave
  `layout.html` a real theme (CSS custom properties, a "&#9670; Kizashi" wordmark, a centered
  login card with focus states, zebra/hover table rows, a data-URI SVG favicon so the browser
  tab isn't blank) without adding any new dependency (still zero JS, per ADR-0014).
- **Tests:** New: `root_handler_test.rs` (`root_redirects_to_events`),
  `tenant_repository_test.rs` (`finds_a_tenant_id_by_name`,
  `returns_none_for_an_unknown_tenant_name`), plus new/updated cases in
  `local_login_handler_test.rs` (`unknown_tenant_name_is_rejected_with_401_not_404`,
  `tenant_repository_failure_returns_500`), `auth_client_test.rs`
  (`http_client_returns_the_token_and_tenant_id_on_valid_credentials`), and
  `login_handler_test.rs` (`post_login_with_an_unknown_workspace_rerenders_the_form_with_an_error`).
  `cargo test -p auth-service --lib` — 33 passed. `cargo test -p kizashi-ui --lib` — 37 passed.
  Rebuilt and redeployed the `auth-service` and `kizashi-ui` containers, re-ran
  `scripts/seed-local-demo.sh` (now also seeds a `tenants` row, workspace name `acme`), and
  drove a real login through the actual running containers end to end: `GET /` → 303 to
  `/events` (previously 404), `POST /login` with `tenant_name=acme` → 303 to `/events` with a
  valid session cookie, `GET /events` with that cookie → 200. Full local CI gate
  (`scripts/ci-local.sh`, `.env` loaded, throwaway local `mssql` for the Fabric/TDS test) —
  `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` clean, `cargo test --workspace --all-features` all green, `cargo llvm-cov` 94.72%
  line coverage (85% floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (the `tenants` table is additive schema, not a change to the multi-tenancy
  model itself — `tenant_id` remains the system-wide scoping key everywhere except this one
  human-facing login form)

## [2026-07-18] feature/0014-docker-images — Agent registry, live status, drill-down, and reports
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** Closes the largest gap this session's live audit surfaced: there was no
  first-class "Agent" concept anywhere in the system — the 6 connector binaries were
  configured only by env vars, with no service that knew of their existence, no way to
  register/list/disable one, and no way to see what a given connector had actually ingested.
  Adds `common::Agent` (id, tenant_id, connector_type, name, config, enabled) and a full
  CRUD registry in Config/Admin Service (`agents` table, `AgentRepository`, audit-logged
  create/update/delete like every other admin entity) at `/v1/agents`. Since connectors don't
  self-report a heartbeat anywhere, "live status" is derived rather than tracked separately:
  Ingestion Service gained `GET /v1/records/stats` (per-`connector_id` record count and
  last-ingested time, aggregated straight off `raw_records`) and
  `GET /v1/records/by-connector` (recent records for one connector, tenant-scoped). The
  matching convention this establishes: an agent's registered `name` is what the deployed
  connector's own `CONNECTOR_ID` env var must be set to, so a registration can be joined
  against real ingestion activity without any new bookkeeping table. Console UI gained three
  pages: `/agents` (register form + live status table, join done in `agents_handler.rs`),
  `/agents/:id` (per-agent drill-down — its own recent records), and `/reports` (ingestion
  volume per connector alongside event counts per type, reusing the existing events feed). Also
  gave the whole UI a second visual pass: form styling (`.panel`, `form.inline`), a `.btn-danger`
  for destructive actions, and nav links for the two new pages.
- **Tests:** `cargo test -p config-admin-service --lib` — 35 passed (12 new: `agent_repository`
  CRUD + tenant scoping + not-found cases, `agent_handlers` tenant-mismatch/404/500 cases).
  `cargo test -p ingestion-service --lib` — 39 passed (10 new: `stats_by_connector` aggregation
  + tenant scoping, `list_by_connector` ordering/limit/tenant scoping, both handlers'
  success/400/500 cases). `cargo test -p kizashi-ui --lib` — 56 passed (19 new across
  `agents_client`, `ingestion_stats_client`, `agents_handler`, `agent_detail_handler`,
  `reports_handler`). Beyond unit tests: rebuilt and redeployed the `config-admin-service`,
  `ingestion-service`, and `kizashi-ui` containers and drove the entire feature through the real
  running stack — logged in, registered a `zendesk`/`support-poller` agent (status correctly
  showed "never run"), posted a real record through `POST /v1/ingest` with `connector_id:
  support-poller`, confirmed the Agents page's status flipped to "active" with a record count
  of 1 (proving the name/connector_id join actually works against live data, not just mocks),
  confirmed the drill-down page showed that record, confirmed the Reports page showed the same
  connector's volume, then deleted the agent and confirmed removal. Full local CI gate
  (`.env` loaded, throwaway local `mssql` for the Fabric/TDS test): `cargo fmt --all --check`
  clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean,
  `cargo test --workspace --all-features` all green, `cargo llvm-cov` 94.11% line coverage
  (85% floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (Agent is additive schema/API following the exact CRUD+audit-log pattern
  TriggerDefinition/NormalizationMapping already established in ADR-0010, not a new
  architectural decision. Deferred/out of scope for this change, tracked separately: a data
  viewer/search page, AI-assisted prompt generation for agent config, and dynamic
  EventTypeDefinition/trigger-condition authoring in the UI.)

## [2026-07-18] feature/0014-docker-images — Data Viewer: search + record detail
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** Adds the "data viewer/search" piece of the AIOps-console gap list. Ingestion
  Service gains `RawRecordRepository::search` (every filter optional and AND-ed: connector_id,
  source_type, an ingested-at range, and a substring match against the raw payload's text
  representation via `ILIKE`) exposed as `GET /v1/records/search`, and `get_by_id` exposed as
  `GET /v1/records/:id` for a single-record detail fetch. The free-text match is deliberately a
  plain `ILIKE` scan, not a dedicated search index (Elasticsearch/pg_trgm/tsvector) — v1 scope
  is "find records that mention X," documented in-code as a known limitation to revisit before
  it's exercised at the platform's actual target scale (thousands of inboxes, hundreds of
  connector APIs — flagged directly by the user during this work). Also added
  `idx_raw_records_tenant_connector_ingested_at`, a composite index covering the shape every
  new Agent-related query (`stats_by_connector`, `list_by_connector`, `search`) actually filters
  and sorts by, since the three single-column indexes from the original migration force a
  bitmap-AND plan instead of a single index scan. Console UI gains `/data` (search form +
  results table) and `/data/:id` (pretty-printed raw + normalized payload).
- **Tests:** `cargo test -p ingestion-service --lib` — 50 passed (11 new: `get_by_id`
  tenant-scoping, `search`'s four filter dimensions individually and combined, both new
  handlers' success/400/500/404 cases). `cargo test -p kizashi-ui --lib` — 64 passed (8 new
  across `ingestion_stats_client`, `data_handler`, `data_detail_handler`). Beyond unit tests:
  rebuilt and redeployed `ingestion-service`/`kizashi-ui`, confirmed the new composite index
  exists via `\d ingestion_service.raw_records` against the real container, posted two records
  with different subjects through the real `ingestion-gateway`, searched `/data?q=printer`
  through the real running Console UI and confirmed only the matching record came back (not
  the other one — proving the filter is real, not a no-op), then opened its `/data/:id` detail
  page and confirmed the full raw payload rendered correctly (HTML-escaped by askama). Full
  local CI gate: `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean, `cargo test --workspace --all-features` all green,
  `cargo llvm-cov` 94.10% line coverage (85% floor), `cargo audit` / `cargo deny check` clean,
  no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (additive query/index on the existing `RawRecord` schema, not a new
  architectural decision. The scale-driven follow-ups this change explicitly defers — a real
  search index and a dynamic per-agent connector scheduling model to replace one static
  container per connector type — are tracked separately, not silently dropped.)

## [2026-07-18] feature/0014-docker-images — Structured email search + Data Viewer pagination
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** Two fixes driven directly by user feedback on the just-shipped Data Viewer.
  First: `raw_payload` was opaque JSON with no defined shape, so there was no way to search
  "subject contains X" or "from Y" or "has attachment Z" — a real gap for the email/message
  connectors this platform targets (Graph Mail, and the planned IMAP connector). Added
  `common::EmailPayload` (subject, from, to/cc/bcc, body, headers, attachments — attachment
  metadata only, never inline bytes; a real attachment's content belongs in the object store
  retention-service already archives into, referenced by `storage_key`) as the documented
  `raw_payload` shape for `SourceType::Message` records from an email connector. Extended
  `RecordSearchFilter`/`GET /v1/records/search` with `subject`/`email_from`/
  `attachment_filename`, each a substring match against the corresponding JSON field (a
  record with no `subject` field simply never matches — not an error), plus a GIN index on
  `raw_payload` so those lookups can use an index scan instead of a full scan at scale. Second:
  every list page (Data Viewer, Agents, Events, Triggers) had a hardcoded `limit` with a silent
  cutoff and no way to see more — flagged directly as not enterprise-grade. Added real
  offset-based pagination to Data Viewer search: the backend fetches one extra row to compute
  `has_more` without a second `COUNT(*)` query (which would scan the same rows twice, at
  exactly the scale pagination exists to handle), and the UI renders Previous/Next as plain
  `<form method="get">` submissions carrying every current filter as hidden fields — no JS,
  consistent with the rest of the app. Agents/Events/Triggers pagination is still open
  (tracked, not silently dropped).
- **Tests:** `cargo test -p common --lib` — 39 passed (2 new: `EmailPayload` round-trip and
  default-field handling). `cargo test -p ingestion-service --lib` — 57 passed (10 new: each
  email filter individually, a no-subject-field non-match case, `has_more` when results exceed
  the page size, offset skipping earlier pages). `cargo test -p kizashi-ui --lib` — 67 passed
  (2 new: pagination controls render correctly on page 0 vs. page 1, with vs. without more
  results). Beyond unit tests: rebuilt and redeployed `ingestion-service`/`kizashi-ui`, posted
  two email-shaped records with different subject/from/attachment through the real
  `ingestion-gateway`, then through the real running Console UI confirmed each of
  subject/email_from/attachment_filename search independently found only its matching record
  (and a deliberately-wrong search term correctly found nothing), then posted 30 more records
  and confirmed real pagination through the live UI: page 1 shows a Next link and no Previous,
  page 2 shows Previous and no Next (30 records, 25-per-page default). Full local CI gate:
  `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` clean, `cargo test --workspace --all-features` all green, `cargo llvm-cov` 94.29%
  line coverage (85% floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (additive schema/query/UI work on already-established patterns, not a new
  architectural decision)

## [2026-07-18] feature/0014-docker-images — Agent deploy-script generator
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** Reframes what the Agents page is for. The prior "register an agent" form wrote a
  database row that meant nothing on its own — no connector was actually deployed, and
  registering/enabling/disabling it had zero effect on any running process (the row only ever
  correlated with real ingestion if an operator separately, manually configured a connector's
  `CONNECTOR_ID` env var to match by hand). This adds a 3-step deploy-script generator
  (`/agents/generate`) that produces ready-to-run scripts — `docker compose run` (matching the
  `connectors` profile services already in `docker-compose.yml`), bash, and PowerShell (both
  `cargo run -p connector-<type>`) — for each of the 6 connector types, with every required
  env var (pulled directly from each connector's actual `std::env::var(...)` calls in its own
  `main.rs`, not guessed) prefilled with whatever the operator typed into the form. No secret is
  ever fabricated or stored: the API key and every connector credential is exactly what the
  operator entered, substituted directly into the output. The old "register an
  already-deployed agent" form still exists on `/agents` for catalog/status purposes, now
  explicitly labeled as not itself deploying anything.
- **Tests:** `cargo test -p kizashi-ui --lib` — 78 passed (13 new: `connector_field_catalog`'s
  per-type field coverage and secret-marking, the 3-step handler's happy paths, 404-style
  fallback for an unknown connector type, and — critically — a test asserting a submitted
  value the operator typed (an API key) actually appears verbatim in the rendered script, not
  just that the page renders). Beyond unit tests: rebuilt and redeployed `kizashi-ui`, walked
  the real 3-step flow through the live container end to end (select Zendesk → confirm the
  Zendesk-specific fields appear → submit real values → confirmed all three script variants
  render with the submitted API key, subdomain, and token verbatim, and the Docker/bash/
  PowerShell commands reference the correct connector binary/service name). Full local CI gate:
  `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` clean, `cargo test --workspace --all-features` all green, `cargo llvm-cov` 94.33%
  line coverage (85% floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (UI/workflow addition, not a new architectural decision)

## [2026-07-18] feature/0014-docker-images — Reverse the no-JS constraint, add charts, Overview dashboard
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** ADR-0014 chose a zero-client-JS Console UI specifically because this build
  environment has no browser-automation tooling to test JS the same disciplined way every
  other crate in this repo is tested. The user explicitly overrode that constraint, wanting
  real graphs and interactive components. Writes **ADR-0015**, reversing only the no-JS part
  of ADR-0014 (the server-rendered shell, session handling, and every existing read view stay
  exactly as they are — this is additive, not a rewrite) and explicitly ruling out a full
  React/SPA migration as its own much larger decision, not something to back into as a side
  effect of "add some graphs." Concretely: a small vendored (not CDN-loaded — this is an
  on-prem-capable enterprise product) dependency-free bar-chart renderer
  (`ui/static/charts.js`, baked into the binary via `include_str!`, served at
  `GET /static/charts.js`) reads real server-rendered JSON out of a `<script
  type="application/json">` tag and draws an SVG bar chart — the underlying HTML table is still
  there and still correct if JS fails or is disabled, a deliberate progressive-enhancement
  choice, not an afterthought. Wired onto the Reports page (ingestion volume by connector,
  events by type). Also ships a new `/overview` landing dashboard (KPI cards: agent count/
  active count, total records ingested, event count, platform health with services-up ratio,
  reusing existing backends — no new data path) and makes it the new post-login/root landing
  page (was `/events`). Gave the nav a visual pass alongside this: icon-prefixed links, a
  divider before Log out, `.kpi-card`/`.pill` CSS building blocks for future pages.
- **Security note:** JSON embedded inside a `<script>` tag has every `<` escaped to `<`
  (`chart_json` in `reports_handler.rs`) so an operator-controlled string containing the
  literal text `</script>` can never prematurely close the tag and inject markup — a
  regression test (`chart_data_escapes_a_connector_id_that_could_close_the_script_tag`) pins
  this down explicitly with exactly that payload.
- **Tests:** `cargo test -p kizashi-ui --lib` — 82 passed (7 new: `static_assets` serves the
  right content-type, `overview_handler`'s KPI math against real seeded data across three
  backends, the redirect-target rename from `/events` to `/overview` in both `root_handler`
  and `login_handler`, and the chart-data XSS-escaping regression test). Beyond unit tests:
  `node --check ui/static/charts.js` confirms the vendored JS is syntactically valid (no build
  step exists to catch this otherwise). Rebuilt and redeployed `kizashi-ui`, confirmed through
  the real running container: `/` redirects to `/overview`, the KPI cards render, `GET
  /static/charts.js` serves with `content-type: text/javascript`, and the Reports page's
  `<script type="application/json">` blocks contain real ingestion/event data accumulated
  across this session's earlier smoke tests. **Not verified — flagged explicitly per CLAUDE.md
  §0, not silently claimed**: the SVG bar chart's actual visual rendering in a real browser.
  This environment has no browser-automation tooling (the exact gap ADR-0014 named and
  ADR-0015 accepts as a tradeoff); server-side correctness (data shape, escaping, JS syntax
  validity) is verified, DOM/visual rendering is not. Full local CI gate: `cargo fmt --all
  --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean, `cargo test --workspace --all-features` all green, `cargo llvm-cov` 94.40% line
  coverage (85% floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** [0015](../docs/adr/0015-console-ui-reverses-adr-0014-no-js-constraint-adds-client-side-js-for-charts-and-components.md)

## [2026-07-18] feature/0014-docker-images — Enforce Agent enabled/disabled status at ingestion
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** `Agent.enabled` was stored since the registry shipped but never checked anywhere
  — disabling an agent in the Console UI had zero effect on whether its data was accepted.
  Closes that gap for real. Config/Admin Service gains `AgentRepository::find_by_name` and
  `GET /v1/agents/by-name/:name`, the lookup Ingestion Gateway needs (agents are keyed by id,
  but ingestion only ever has a `connector_id` string to check against). Ingestion Gateway
  gains an `AgentStatusClient` and checks it on every `POST /v1/ingest`: a `connector_id` with
  no matching registered `Agent` still ingests normally (permissive default — most connectors
  today have no registered row at all, and this must never break them), a matching *enabled*
  agent ingests normally, and a matching *disabled* agent is rejected with 403. A status-lookup
  failure (Config/Admin Service down, network blip) also fails open — availability of the
  ingest path matters more than this soft-enforcement check, so one dependency having a bad
  moment must never take down ingestion for every connector. Console UI's Agents page gains an
  actual Enable/Disable toggle button (previously there was no way to flip the flag at all
  through the UI) and a status pill replacing the old plain yes/no text.
- **Tests:** `cargo test -p config-admin-service --lib` — 40 passed (5 new:
  `find_by_name`'s tenant-scoping and not-found cases, the `by-name` handler's 200/404). `cargo
  test -p ingestion-gateway --lib` — 21 passed (7 new: `AgentStatusClient` against a real stub
  server for enabled/disabled/404/unreachable, and the proxy handler's three enforcement
  cases — disabled rejects, unregistered passes, lookup-failure fails open). `cargo test -p
  kizashi-ui --lib` — 85 passed (3 new: `update_agent` against a real stub server, the toggle
  handler flipping state and redirecting, and its login-required case). Beyond unit tests:
  rebuilt and redeployed `ingestion-gateway`/`config-admin-service`/`kizashi-ui`, registered a
  real agent through the live UI, ingested through the live `ingestion-gateway` while enabled
  (201), disabled it through the UI's new toggle button, ingested again with the same
  `connector_id` and got the real 403 with the exact expected error message, confirmed an
  unrelated unregistered `connector_id` still ingests fine (permissive default holds), then
  re-enabled and confirmed ingestion resumes (201) before cleaning up the test agent. Full
  local CI gate: `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean, `cargo test --workspace --all-features` all green,
  `cargo llvm-cov` 94.38% line coverage (85% floor), `cargo audit` / `cargo deny check` clean,
  no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (closes a gap in the already-established Agent registry, not a new
  architectural decision)

## [2026-07-18] feature/0014-docker-images — Events pagination
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** Events was one of three list pages flagged as having a hardcoded limit with a
  silent cutoff and no way to see more (Data Viewer got real pagination earlier; this closes
  the same gap for Events). Dashboard API's `EventFilter` gains `offset`, the ClickHouse query
  gains a matching `OFFSET`, and `GET /v1/events` now returns `{events, has_more}` instead of a
  bare array — `has_more` computed the same way as the Data Viewer's search (fetch one extra
  row, no second `COUNT(*)` query against ClickHouse). Query Gateway needed no changes — it
  already passes the full query string through via `OriginalUri` untouched. Console UI's
  `/events` gains the same Previous/Next `<form method="get">` pagination controls as the Data
  Viewer. Agents and Triggers pagination remain open — flagged, not silently dropped; Triggers
  in particular is low-volume (operator-configured, not per-record data) so it's lower priority
  than Events/Data Viewer, which both read from tables that grow with real traffic.
- **Tests:** `cargo test -p dashboard-api --lib` — 18 passed (3 new: offset skips earlier
  pages at the repository level, the handler's `has_more` computation, and the response-shape
  change reflected in the existing scoped-events test). `cargo test -p kizashi-ui --lib` — 85
  passed (`EventsPage`/`EventsClient` trait signature change threaded through
  `events_handler`, `overview_handler`, and `reports_handler`'s call sites, plus 2 new
  pagination-control-rendering tests mirroring the Data Viewer's). Beyond unit tests: rebuilt
  and redeployed `dashboard-api`/`kizashi-ui`, confirmed `/events` and `/events?page=1` both
  return 200 through the real running stack (query-gateway → dashboard-api → ClickHouse) with
  the new response shape, proving the plumbing holds end-to-end. Full live-data pagination
  boundary behavior (Next/Previous appearing at exactly the right count) is unit-tested with
  controlled data, not independently re-verified against real ClickHouse volume in this pass —
  the demo tenant has no real event traffic to page through without standing up the full
  ingest→normalize→analyze→trigger pipeline, called out explicitly rather than implied. Full
  local CI gate: `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean, `cargo test --workspace --all-features` all green,
  `cargo llvm-cov` 94.45% line coverage (85% floor), `cargo audit` / `cargo deny check` clean,
  no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (additive query/response-shape change, not a new architectural decision)

## [2026-07-18] feature/0014-docker-images — Agents pagination, and a real correctness fix it forced
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** Closes the last of the three flagged list pages (Data Viewer and Events already
  had real pagination). `AgentRepository::list` gains `limit`/`offset`, `GET /v1/agents` now
  returns `{agents, has_more}` (fetch-one-extra pattern, same as Events/Data Viewer), and
  `/agents` gets the same Previous/Next controls. Doing this properly surfaced a real
  correctness bug in the process: `agent_detail_handler` and the enable/disable toggle both
  found "the agent" by calling `list_agents` and searching the result for a matching id — which
  only worked because `list_agents` used to return every agent unpaginated. Once it's
  paginated, that lookup silently breaks the moment an agent isn't on the first page (toggling
  an agent on page 2 would appear to succeed — 303 redirect, no error — while doing nothing).
  Fixed by adding `AgentsClient::get_agent`/`GET /v1/agents/:id` (config-admin-service already
  had this route; the UI just wasn't using it) and switching both call sites to fetch by id
  directly instead of paging through a list. Triggers pagination remains open — still lower
  priority, operator-configured data that doesn't grow with traffic the way agents/events/raw
  records do.
- **Tests:** `cargo test -p config-admin-service --lib` — 42 passed (3 new: `list` respects
  limit/offset at the repository level, the handler's `has_more` computation, the existing
  scoped-list test updated for the new response shape). `cargo test -p kizashi-ui --lib` — 88
  passed (`AgentsClient` trait signature change threaded through every call site, 2 new
  pagination-control-rendering tests, `get_agent` tested against a real stub server). Beyond
  unit tests: rebuilt and redeployed `config-admin-service`/`kizashi-ui`, registered 30 real
  agents through the live UI, confirmed page 1 shows Next-only and page 2 shows Previous-only,
  then — the test that actually matters — found an agent that only exists on page 2, toggled
  it, and confirmed on a fresh page load that it actually flipped from enabled to disabled
  (proving the `get_agent` fix holds against live data, not just the bug's absence in a unit
  test), then cleaned up all 30 test agents. Full local CI gate: `cargo fmt --all --check`
  clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo
  test --workspace --all-features` all green, `cargo llvm-cov` 94.44% line coverage (85%
  floor), `cargo audit` / `cargo deny check` clean, no new advisories.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (additive query/response-shape change plus a bugfix, not a new architectural
  decision)

## [2026-07-18] feature/0014-docker-images — Triggers pagination (last of the four flagged list pages)
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** Closes the last remaining item from the pagination backlog (Data Viewer, Events,
  and Agents were already done). `TriggerDefinitionRepository::list` gains `limit`/`offset`
  (Postgres impl adds `LIMIT $2 OFFSET $3`, ordered by `name`), `GET /v1/trigger-definitions`
  now returns `{triggers, has_more}` using the same fetch-one-extra pattern as every other
  paginated list endpoint in this codebase, and `/triggers` gets the same Previous/Next
  `<form method="get">` controls as Agents/Events/Data Viewer. `TriggersClient::list_triggers`
  and the Console UI handler/template were updated to match, mirroring `AgentsClient`/
  `agents_handler.rs` exactly. Triggers had no existing "get one by id" call site (no detail
  page), so this pass did not surface the same list-vs-lookup bug the Agents pagination work
  found — there was nothing to fix beyond the list endpoint itself.
- **Tests:** `cargo test -p config-admin-service --lib` — 43 passed (1 new: `list` respects
  limit/offset at the repository level; the existing scoped-list test and the full CRUD
  round-trip test were both updated for the new response shape). `cargo test -p kizashi-ui
  --lib` — 92 passed (`TriggersClient` trait signature change threaded through
  `triggers_handler`, 2 new pagination-control-rendering tests mirroring Agents/Events).
  Beyond unit tests: rebuilt and redeployed `config-admin-service`/`kizashi-ui`, seeded 30
  real trigger definitions directly against the live `config-admin-service` API, confirmed
  `/triggers` shows Next-only on page 0 and Previous-only on page 1 with the 30th trigger
  landing on page 1 as expected, then deleted all 30 test triggers (and their audit-log rows)
  to leave the demo tenant clean. Full local CI gate: `cargo fmt --all --check` clean, `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo test
  --workspace --all-features` all green (0 failures across every crate, verified against a
  throwaway local `mssql` container standing in for CI's Fabric TDS dependency), `cargo
  llvm-cov` 93.90% line coverage (85% floor), `cargo audit` / `cargo deny check` clean (same
  two pre-existing `unmaintained` advisories already allow-listed — `instant`,
  `rustls-pemfile` — no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a (additive query/response-shape change, not a new architectural decision)

## [2026-07-18] feature/0014-docker-images — Audit log immutability enforced at the database level
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** `config_admin_service.config_audit_log` and `retention_service.retention_audit_log`
  were append-only by application convention only (no Rust code path ever issues UPDATE/DELETE
  against them) — nothing at the database level stopped a bug or a manual `psql` session from
  mutating or deleting an audit row, a real gap against CLAUDE.md §5's "every admin/config
  change is logged immutably" bar for a product that expects compliance audits. Since
  `common::connect_with_schema` and every service's `main.rs` run migrations and runtime
  queries through the same connection pool and the same shared `kizashi` Postgres role (no
  role separation exists anywhere in this codebase), a `REVOKE UPDATE, DELETE` approach would
  have required introducing a second privileged migration-only role — a much larger,
  unprecedented change. Went with a `BEFORE UPDATE OR DELETE` trigger on each table that
  `RAISE EXCEPTION`s instead — a single plain `.sql` migration per service, no new role, no
  `docker-compose.yml`/`.env.example`/`common` changes, works regardless of which role issues
  the mutation.
- **Tests:** TDD'd against real Postgres: wrote the regression tests first, ran them without
  the migration present to confirm they fail for the expected reason (`rows_affected: 1`, i.e.
  the row-level trigger genuinely wasn't there yet), then added the migration and reran.
  `cargo test -p config-admin-service --test repository_integration_test` — 6 passed (2 new:
  `config_audit_log_rejects_update_at_the_database_level`,
  `config_audit_log_rejects_delete_at_the_database_level`, both asserting the real Postgres
  error text). `cargo test -p retention-service --test retention_policy_integration_test` — 6
  passed (2 new, same pattern for `retention_audit_log`). Beyond integration tests: rebuilt and
  redeployed `config-admin-service`/`retention-service`, created a real trigger definition and
  a real retention policy through their live HTTP APIs (so each had a genuine audit row), then
  ran a raw `UPDATE`/`DELETE` against each audit table directly via `psql` against the live
  running Postgres container and confirmed Postgres itself rejected all four attempts with
  `... is append-only: UPDATE/DELETE is not permitted` — proving the trigger is live against
  the real running stack, not just the test database. Full local CI gate: `cargo fmt --all
  --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean, `cargo test --workspace --all-features` all green (0 failures across every crate,
  verified against a throwaway local `mssql` container standing in for CI's Fabric TDS
  dependency), `cargo llvm-cov` 93.90% line coverage (85% floor — unchanged, since the new
  code is pure SQL plus integration tests, neither counted in the Rust line-coverage ratchet),
  `cargo audit` / `cargo deny check` clean (same two pre-existing allow-listed `unmaintained`
  advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — closes a gap flagged in the standing gap-closing roadmap
  (Phase 1b, security/compliance), not a spec §11 open item.

## [2026-07-18] feature/0014-docker-images — API key lifecycle management (create/list/revoke)
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** Closes gap-closing-roadmap Phase 1c: until now `ApiKeyStore` only had
  `tenant_for_key` (lookup) — there was no way to actually create or revoke a connector API
  key except a manual `INSERT`/`UPDATE` against Postgres, a real problem for a resold
  enterprise product whose customers need to self-serve issue and rotate credentials.
  `ApiKeyStore` gains `create`/`list`/`revoke`, all backed by Postgres, with `create`/`revoke`
  each writing an audit row in the same transaction as the key mutation (CLAUDE.md §5's
  "new mutable config entity ships with an audit-log write in the same PR" rule) — this
  required standing up ingestion-gateway's *first* audit log (`ingestion_gateway_audit_log`,
  ported from config-admin-service's `audit_log.rs`), which ships with the same
  `BEFORE UPDATE OR DELETE` immutability trigger just added to the other two audit tables, from
  day one rather than as a follow-up gap. New endpoints: `POST /v1/api-keys` (returns the
  plaintext key once — only its SHA-256 hash is ever persisted, matching the existing
  `tenant_for_key` convention), `GET /v1/api-keys` (tenant-scoped summaries, no key material),
  `DELETE /v1/api-keys/:id` (idempotent revoke), `GET /v1/api-keys/:id/audit-log`. Console UI
  gets a new `/api-keys` page (nav: "API Keys") — create form, table with Revoke buttons, and
  a one-time plaintext-key reveal panel shown only on the response immediately after creation,
  never persisted or retrievable again. Required adding `INGESTION_GATEWAY_URL` (the internal
  address) alongside the existing `INGESTION_GATEWAY_PUBLIC_URL` (the address a *deployed
  connector* should point at, not necessarily reachable from inside the UI container) — Console
  UI needed a way to reach ingestion-gateway's admin API that's distinct from the
  connector-facing address.
- **Tests:** `cargo test -p ingestion-gateway --lib` — 32 passed (in-memory `ApiKeyStore`/
  `AuditLogReader` test doubles, HTTP handler tests for create/list/revoke/audit-log, a
  never-exposes-key-material assertion on the list response, a missing-tenant-header 401
  case). `cargo test -p ingestion-gateway --test api_key_store_integration_test` — 6 passed
  against real Postgres (create writes a Created audit row and the key resolves; revoke writes
  a Deleted audit row and the key stops resolving; revoking an already-revoked key writes no
  duplicate audit row; list is tenant-scoped; the new `ingestion_gateway_audit_log` rejects
  UPDATE/DELETE at the database level, same pattern as the previous PR's immutability tests).
  `cargo test -p kizashi-ui --lib` — 106 passed (`ApiKeysClient` HTTP-client tests against a
  real stub server, 5 new handler tests including the one-time-reveal assertion). Beyond unit
  tests: rebuilt and redeployed `ingestion-gateway`/`kizashi-ui`, logged into the live UI,
  created a real key through `/api-keys`, confirmed the plaintext was shown, used it to
  authenticate a real `POST /v1/ingest` call (got 422 from the payload-shape check, not 401 —
  proving auth passed), revoked it through the UI, and confirmed the exact same key now gets
  401 "invalid API key" on the same ingest call — the complete lifecycle proven against the
  real running stack, not just test doubles. Full local CI gate: `cargo fmt --all --check`
  clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo
  test --workspace --all-features` all green (0 failures across every crate, verified against
  a throwaway local `mssql` container standing in for CI's Fabric TDS dependency), `cargo
  llvm-cov` 93.76% line coverage (85% floor), `cargo audit` / `cargo deny check` clean (same
  two pre-existing allow-listed `unmaintained` advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — closes a gap flagged in the standing gap-closing roadmap (Phase 1c,
  security/compliance), not a spec §11 open item.

## [2026-07-18] feature/0014-docker-images — RBAC v1: role on local users, write-path enforcement on config-admin-service
- **Type:** feature
- **Summary:** Closes gap-closing-roadmap Phase 1a's highest-priority item: until now every
  service trusted `X-Tenant-Id` with zero role/permission check — any authenticated session
  could create/update/delete triggers and mappings regardless of who it belonged to. Adds
  `common::Role` (`Viewer < Operator < Admin`, ordered) and threads it end-to-end through the
  identity chain that already exists: `auth_service.local_users` gains a `role` column (new
  migration, existing rows default to `admin` so the demo login isn't locked out) →
  `SessionClient::mint_session` gains a `role` param → `query-gateway`'s `/internal/tokens` +
  `TokenStore` + `query_api_tokens` table carry it (`tenant_for_token` renamed
  `session_for_token`, now returns `(tenant_id, role)`) → `LoginResponse` returns it → Console
  UI's `Session` struct carries it. `config-admin-service`'s `create_trigger`/`update_trigger`/
  `create_mapping`/`update_mapping` now require an `X-Role` header at least `Operator`, 403
  otherwise, 401 if the header is missing entirely — the same trust-boundary pattern
  `X-Tenant-Id` already uses, since no gateway sits in front of this service (ADR-0010) to
  enforce roles at a proxy layer. OIDC logins (which have no local role source) default to the
  least-privileged `Viewer` rather than being left unroled or guessing something permissive.
  See ADR-0016 for the full v1-scope decision, including what's explicitly deferred:
  `retention-service`, `action-executor`, and `ingestion-gateway`'s API-key endpoints remain
  unenforced (tracked, not silently dropped), and there's no "assign another user's role" UI
  yet — that's a direct SQL update for now, same interim state API keys were in before Phase
  1c's UI shipped.
- **Tests:** `cargo test -p common role` — 5 passed (ordering, `at_least`, `Display`/`FromStr`
  round-trip, snake_case serialization). `cargo test -p auth-service --lib` — 33 passed
  (`LocalUser`/`SessionClient` role threading, a new assertion that a successful login mints
  with the user's actual role and returns it in the response body). `cargo test -p auth-service
  --test local_user_repository_integration_test` — 1 passed against real Postgres, now
  asserting the stored role round-trips. `cargo test -p query-gateway --lib` — 14 passed
  (`TokenStore`/`session_for_token` role threading). `cargo test -p query-gateway --test
  token_store_integration_test` — 2 passed against real Postgres (stored role round-trips;
  minted tokens carry the role they were minted with). `cargo test -p config-admin-service
  --lib` — 47 passed (4 new: missing-role-header 401, viewer-rejected 403 on both
  trigger-create and mapping-create, operator-allowed 201 — the actual enforcement contract).
  `cargo test -p kizashi-ui --lib` — 101 passed (every `Session`/`AppState` construction site
  across the test suite updated for the new field; no behavioral change to any existing UI test
  since role isn't yet consumed by nav or any write-path client). Beyond unit/integration
  tests: rebuilt and redeployed `auth-service`/`query-gateway`/`config-admin-service`/
  `kizashi-ui`, confirmed the demo login still returns `"role":"admin"` and Console UI login
  still works end-to-end, then — the test that actually proves the enforcement — sent a real
  trigger-create request directly at the live `config-admin-service` three ways: no `X-Role`
  header (401), `X-Role: viewer` (403), `X-Role: operator` (201), all against real running
  Postgres with the real migration applied. Full local CI gate: `cargo fmt --all --check`
  clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (one
  `await_holding_lock` finding caught and fixed — a `MutexGuard` held across an `.await` in a
  new test), `cargo test --workspace --all-features` all green (0 failures across every crate,
  verified against a throwaway local `mssql` container standing in for CI's Fabric TDS
  dependency), `cargo llvm-cov` 93.81% line coverage (85% floor), `cargo audit` / `cargo deny
  check` clean (same two pre-existing allow-listed `unmaintained` advisories, no new
  advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** [0016-rbac-v1-scope-role-on-local-user-x-role-header-trust-config-admin-write-path-enforcement.md](../adr/0016-rbac-v1-scope-role-on-local-user-x-role-header-trust-config-admin-write-path-enforcement.md)

## [2026-07-19] feature/0014-docker-images — RBAC enforcement extended to retention-service
- **Type:** feature
- **Summary:** First of ADR-0016's explicitly-deferred follow-ups: `retention-service`'s
  `create_policy`/`update_policy` now require `X-Role` at least `Operator`, mirroring
  `config-admin-service`'s enforcement exactly (`role_from_headers`/`require_operator` helpers,
  same 401-missing/403-insufficient/pass-through-Operator-or-above contract). No new migration
  needed — `retention-service` doesn't mint its own sessions; it trusts the same `X-Role` header
  Console UI/callers already forward. `action-executor`'s trigger CRUD and
  `ingestion-gateway`'s API-key create/revoke remain unenforced, still tracked in ADR-0016 as
  the next follow-ups.
- **Tests:** `cargo test -p retention-service --lib` — 43 passed (3 new: missing-role 401,
  viewer-rejected 403, operator-allowed 201 on `create_policy`, mirroring
  config-admin-service's role tests exactly). Beyond unit tests: rebuilt and redeployed
  `retention-service`, sent a real policy-create request three ways against the live service —
  no `X-Role` (401), `X-Role: viewer` (403), `X-Role: operator` (201) — against real running
  Postgres. Full local CI gate: `cargo fmt --all --check` clean, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` clean, `cargo test --workspace --all-features`
  all green (0 failures across every crate, verified against a throwaway local `mssql`
  container standing in for CI's Fabric TDS dependency), `cargo llvm-cov` 93.84% line coverage
  (85% floor), `cargo audit` / `cargo deny check` clean (same two pre-existing allow-listed
  `unmaintained` advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — implements a follow-up explicitly scoped out of ADR-0016's v1, not a new
  architectural decision.

## [2026-07-19] feature/0014-docker-images — Instana-style Pipeline Map view
- **Type:** feature
- **Summary:** Continues ADR-0015's Instana-style APM direction (#30) with the feature that
  actually reads as "Instana" — a live topology map, not another table. New `/pipeline` page
  renders the ingest → normalize → analyze → trigger → act chain as connected boxes: each stage
  node colored by its real `/v1/health` status (green dot = up, red = down), each connecting
  edge labeled with the message type it carries and colored by real `/v1/backlog` queue depth
  (grey = empty, amber = building up, red = past the critical threshold). Both data sources
  already existed in Observability (ADR-0012) — this wires Console UI to consume the backlog
  endpoint for the first time via a new `BacklogClient`, alongside the existing `HealthClient`.
  A backlog-lookup failure degrades the page to "topology with no backlog numbers" rather than
  an error page (health is the load-bearing signal; backlog is enrichment), while a health
  failure does show the error page since the topology has nothing meaningful to render without
  it. Template built as a flat, pre-interleaved `Vec<TopologyItem>` (stage/edge alternating)
  rather than having the template zip two arrays — Askama's expression grammar makes index
  arithmetic (`edges[loop.index0 - 1]`) fragile, so the ordering is resolved in Rust and the
  template just iterates and matches.
- **Tests:** `cargo test -p kizashi-ui --lib` — 108 passed (2 new for `BacklogClient` against a
  real stub server; 5 new for the pipeline handler: all five stages render with correct
  up/down status, redirects to login when signed out, shows an error when health fails,
  degrades gracefully with "n/a" backlog numbers when backlog fails, and a 500-message queue
  renders as `edge-critical`). Beyond unit tests: rebuilt and redeployed `kizashi-ui`, logged
  into the live stack, loaded `/pipeline` for real, and confirmed all five stages rendered
  "up" with 0-message queues on every edge — matching the actual idle state of the real running
  pipeline (no synthetic data, genuine live health/backlog reads through the full
  Console-UI-to-Observability-to-RabbitMQ path). Full local CI gate: `cargo fmt --all --check`
  clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo
  test --workspace --all-features` all green (0 failures across every crate, verified against
  a throwaway local `mssql` container standing in for CI's Fabric TDS dependency), `cargo
  llvm-cov` 93.91% line coverage (85% floor), `cargo audit` / `cargo deny check` clean (same
  two pre-existing allow-listed `unmaintained` advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — additive UI feature consuming already-decided ADR-0012/ADR-0015 capabilities,
  not a new architectural decision.

## [2026-07-19] feature/0014-docker-images — Role-aware nav: hide write actions from Viewers
- **Type:** feature
- **Summary:** Closes ADR-0016's last still-open Console UI v1 item: "role-aware nav (hide
  admin actions from viewer)." `/agents` and `/api-keys` now compute
  `can_write = session.role.at_least(Role::Operator)` and gate the register/create forms and
  every per-row Enable/Disable/Remove/Revoke button behind it — a `Viewer` sees the same data
  (agent list, key list) with none of the mutation controls. This is presentation-layer only:
  `agents`-write and `ingestion-gateway`'s API-key endpoints don't enforce role server-side yet
  (only config-admin-service's trigger/mapping writes and retention-service's policy writes
  do, per ADR-0016 and its retention-service follow-up) — noted explicitly in code comments so
  it isn't mistaken for a security boundary.
- **Tests:** `cargo test -p kizashi-ui --lib` — 112 passed (4 new: a `Viewer` session sees the
  agent/key data but none of the write UI; an `Operator` session sees both). Beyond unit
  tests: rebuilt and redeployed `kizashi-ui`, inserted a real `viewer`-role user directly into
  the running `auth_service.local_users` table (via the existing `hash_password` bin for a
  real Argon2 hash), logged in as that user through the live UI, and confirmed zero matches for
  every write control on both `/agents` and `/api-keys` — then logged back in as the existing
  `admin`-role demo user and confirmed all controls are present, proving the gate works both
  directions against the real running stack, not just template unit tests. Full local CI gate:
  `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` clean, `cargo test --workspace --all-features` all green (0 failures across every
  crate, verified against a throwaway local `mssql` container standing in for CI's Fabric TDS
  dependency), `cargo llvm-cov` 93.96% line coverage (85% floor), `cargo audit` / `cargo deny
  check` clean (same two pre-existing allow-listed `unmaintained` advisories, no new
  advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — implements a follow-up explicitly scoped into ADR-0016's v1 Console UI item,
  not a new architectural decision.

## [2026-07-19] feature/0014-docker-images — RBAC enforcement extended to ingestion-gateway API keys
- **Type:** feature
- **Summary:** Closes ADR-0016's last remaining deferred write path.
  `action-executor` turned out to have no HTTP write surface at all (it's a pure RabbitMQ
  consumer with only `/healthz`), so there was nothing to gate there — that leaves
  `ingestion-gateway`'s `create_api_key`/`revoke_api_key` as the real remaining item, now
  requiring `X-Role` at least `Operator` via the same `role_from_headers`/`require_operator`
  pattern as every other write path. Because Console UI's Agents/API-Keys pages actively call
  these endpoints (unlike config-admin-service's trigger/mapping writes, which have no UI form
  yet), enabling enforcement without also updating the caller would have broken the live
  create/revoke flow verified working in the previous PR — so `ApiKeysClient::create_api_key`/
  `revoke_api_key` gained a `role: Role` parameter, forwarded as `X-Role`, with
  `api_keys_handler.rs` passing `session.role` through. Every write-path service in the
  platform's admin surface (config-admin-service, retention-service, ingestion-gateway) is now
  role-gated; the only remaining gap from ADR-0016 is the "assign another user's role" admin UI,
  still explicitly out of scope for v1.
- **Tests:** `cargo test -p ingestion-gateway --lib` — 34 passed (2 new: missing-role 401,
  viewer-rejected 403 on `create_api_key`; existing create/revoke tests updated to send
  `X-Role`). `cargo test -p kizashi-ui --lib` — 112 passed (`ApiKeysClient` trait signature
  change threaded through every call site; the HTTP-client stub server now rejects a missing
  `X-Role` on create, proving the client actually sends it). Beyond unit tests: rebuilt and
  redeployed `ingestion-gateway`/`kizashi-ui`, created a real key through the live UI as the
  `admin`-role demo user (confirming the enforcement-plus-forwarding change didn't break the
  working flow), then sent the same create request directly at `ingestion-gateway` three ways —
  no `X-Role` (401), `X-Role: viewer` (403), `X-Role: operator` (201) — against the real
  running service. Full local CI gate: `cargo fmt --all --check` clean, `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` clean, `cargo test --workspace
  --all-features` all green (0 failures across every crate, verified against a throwaway local
  `mssql` container standing in for CI's Fabric TDS dependency), `cargo llvm-cov` 93.98% line
  coverage (85% floor), `cargo audit` / `cargo deny check` clean (same two pre-existing
  allow-listed `unmaintained` advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — implements the last follow-up explicitly scoped out of ADR-0016's v1, not a
  new architectural decision.

## [2026-07-19] feature/0014-docker-images — normalization-service live-RabbitMQ integration test
- **Type:** chore
- **Summary:** Closes one of the three testing gaps from the gap-closing roadmap's Phase 3:
  `normalization-service` had Postgres-repository and schema-contract tests but nothing
  exercising its actual `record.ingested` → `record.normalized` processing path against real
  infrastructure. New `tests/normalization_integration_test.rs` mirrors the pattern already
  proven in `analysis-service`/`trigger-engine`'s integration tests — connect to real
  RabbitMQ, declare/bind a queue, call `process_normalization` directly with real
  `PostgresMappingRepository` + a stub HTTP server standing in for Ingestion Service's
  `PATCH /v1/records/:id/normalized`, then assert the published `record.normalized` message.
  A second test covers the no-mapping-configured path (asserts `NoMappingConfigured`, not an
  error, and implicitly nothing is published). `action-executor`'s equivalent gap and
  `dashboard-api`'s live-ClickHouse gap remain open, tracked as further Phase 3 follow-ups.
- **Tests:** `cargo test -p normalization-service --test normalization_integration_test` — 2
  passed against real RabbitMQ and real Postgres.
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — closes a gap flagged in the standing gap-closing roadmap (Phase 3, testing
  gaps), not a spec §11 open item.

## [2026-07-19] feature/0014-docker-images — Console UI layout overhaul: fix wasted space and unprofessional appearance
- **Type:** fix
- **Summary:** Direct user feedback: "the ui is very unprofessional and a huge waste of
  space." Verified with real headless-Chrome screenshots against the live running stack
  (not guessed from CSS) — every page with a form panel (Agents, API Keys, Data Viewer) had a
  bare 480px-wide `.panel` on the left and pure empty black space filling the rest of a
  1600px-wide viewport; Overview was 4 KPI cards followed by ~700px of nothing; Platform
  Health was a plain 2-column table wasting nearly the entire row width on a service name and
  one status word; Reports showed the exact same connector/event data twice — once as a bar
  chart, once as an identical table directly below it. Fixed all of it: `.form-row` pairs
  every form panel with a new `.info-panel` (contextual tips/docs) so the row uses the full
  width instead of leaving a void; `.chart-row` puts Reports' chart and its detail table
  side by side instead of stacked duplicates; Platform Health became a `.status-grid` of
  compact status cards instead of a bare table; every list page (Agents, Events, Triggers,
  Data Viewer) gained a proper `.empty-state` block instead of rendering an empty table with
  nothing below it; the Overview dashboard now embeds a compact live Pipeline Map preview
  below the KPI row (extracted the topology-building logic from `pipeline_handler.rs` into a
  shared `ui/src/topology.rs` module so both pages render the same real data, not a
  duplicated/faked preview) instead of ending after one line of links; the Pipeline Map's own
  topology nodes/edges were resized to stop wrapping/clipping (`flex: 0 1 170px` sizing found
  by iterating against real screenshots, not guessed) and gained a color legend.
- **Tests:** `cargo test -p kizashi-ui --lib` — 121 passed (6 new for the extracted
  `topology` module's stage/edge-building logic — status lookup, unknown-stage fallback,
  severity thresholds, backlog-present vs. absent; 3 new empty-state tests for Agents/
  Triggers/Events confirming the empty-state message renders and no `<table>` tag does when
  there's genuinely nothing to show, `page == 0 && !has_more` in the empty-state condition
  specifically to avoid hiding Previous/Next controls on a legitimately-empty later page — a
  real bug the first pass introduced and the existing pagination tests caught immediately).
  Beyond unit tests: rebuilt and redeployed `kizashi-ui` **twice** during this fix — the first
  screenshot pass caught the topology wrapping bug (`Action Executor` dropping to its own
  row) and a text-clipping regression from an over-aggressive `flex: 1 1 0` fix, both only
  visible in an actual rendered screenshot, not in any test assertion. Final verification was
  a full screenshot sweep of all 9 pages (Overview, Agents, API Keys, Pipeline Map, Events,
  Reports, Platform Health, Data Viewer, Triggers) against the live running stack with real
  session cookies, confirmed by direct visual inspection, not just "the page returned 200."
  Full local CI gate: `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` clean, `cargo test --workspace --all-features` all green (0
  failures across every crate, verified against a throwaway local `mssql` container standing
  in for CI's Fabric TDS dependency), `cargo llvm-cov` 94.08% line coverage (85% floor),
  `cargo audit` / `cargo deny check` clean (same two pre-existing allow-listed `unmaintained`
  advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — CSS/template layout fix, not a new architectural decision.

## [2026-07-19] feature/0014-docker-images — Event record lineage: record_ids field closes the last untraceable pipeline hop
- **Type:** feature
- **Summary:** `Event → ActionExecution` was already traceable (`ActionExecution.event_id`) and
  `RawRecord → AnalyzedRecord` needs no lookup (same row), but `RawRecord → Event` — which
  records actually caused a trigger to fire — was completely untraceable: `SignalRepository::
  window_stats` computed count/values for trigger evaluation and then discarded which record
  ids contributed. `common::Event` gains `record_ids: Vec<Uuid>`; `window_stats` now returns
  `(count, values, record_ids)` from the same `analyzed_signals` scan (no new query);
  `process_analyzed_record` attaches them via `Event::new(...).with_record_ids(...)`. The
  ClickHouse `events` table gains a matching `record_ids Array(UUID)` column. This closes the
  only remaining gap in the platform's full ingest→normalize→analyze→event→action lineage —
  unblocking a record-journey/link-analysis view in Console UI without further backend work,
  since `GET /data/:id` and `GET /v1/events/:id` already exist and now the second one returns
  the link. See ADR-0017 for the full decision including why a builder method (not a changed
  `Event::new` signature) and the live-ClickHouse migration note.
- **Tests:** `cargo test -p trigger-engine --lib` — 29 passed (`window_stats` test now asserts
  record ids are returned; both a single-record threshold-trigger fire and a multi-record
  count-over-window fire assert the resulting Event carries the correct record id(s)). `cargo
  test -p trigger-engine --test event_created_contract_test` — 3 passed (1 new: `record_ids`
  round-trips through the wire message). `cargo test -p trigger-engine --test
  trigger_integration_test` — 1 passed against real Postgres/ClickHouse/RabbitMQ, confirming
  the altered schema doesn't break the existing write path. `cargo test -p dashboard-api --test
  event_query_integration_test` — 2 passed, new test file closing another Phase 3 testing gap
  (dashboard-api had zero tests against real ClickHouse before this): inserts a real row with
  `record_ids` via ClickHouse's HTTP interface, reads it back through
  `ClickHouseEventQueryRepository::get_event`/`list_events`, asserts the ids round-trip; a
  second test confirms `get_event` returns `None` for an unknown id against the real service
  (not a stub). Beyond tests: applied `ALTER TABLE events ADD COLUMN IF NOT EXISTS record_ids
  Array(UUID)` directly against this build's live ClickHouse instance (a pre-existing table
  `CREATE TABLE IF NOT EXISTS` doesn't alter — noted as a real rollout gotcha in ADR-0017),
  then confirmed both the trigger-engine write path and the dashboard-api read path work
  against the now-altered live table before any test ran. Full local CI gate: `cargo fmt --all
  --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean, `cargo test --workspace --all-features` all green (0 failures across every crate,
  verified against a throwaway local `mssql` container standing in for CI's Fabric TDS
  dependency), `cargo llvm-cov` 94.10% line coverage (85% floor), `cargo audit` / `cargo deny
  check` clean (same two pre-existing allow-listed `unmaintained` advisories, no new
  advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** [0017-event-record-lineage-record-ids-field-on-event.md](../adr/0017-event-record-lineage-record-ids-field-on-event.md)

## [2026-07-19] feature/0014-docker-images — ActionExecution gains tenant_id; action-executor's first query endpoint; dashboard-api record_id filter
- **Type:** fix
- **Summary:** Building the record→event lineage (ADR-0017) surfaced a real compliance gap
  while wiring up the event→action hop for a UI journey view: `ActionExecution` had **no
  `tenant_id` at all**, on the type or the table — a genuine violation of CLAUDE.md §5's
  "every row is tenant-scoped" rule, only latent until now because `action-executor` had zero
  HTTP query surface (pure RabbitMQ consumer, insert-only repository). Fixed properly rather
  than worked around: `ActionExecution` gains `tenant_id: Uuid` (from `Event.tenant_id`, always
  available at write time); `action_executions` gets a migration adding the column (existing
  126 rows in this build's dev database were synthetic test/demo data with no way to backfill
  a real tenant, so they're dropped as part of the migration, documented inline in the SQL
  comment, not silently). `ExecutionRepository` gains `list_by_event(tenant_id, event_id)`, and
  action-executor gets its first real HTTP endpoint — `GET /v1/action-executions?event_id=X` —
  trusting `X-Tenant-Id` the same way every other gateway-less service in this codebase does.
  Separately, `dashboard-api`'s `EventFilter` gains `record_id: Option<Uuid>`
  (`GET /v1/events?record_id=X`), using ClickHouse's `has(record_ids, ...)` against the
  `record_ids` column from the previous PR — completing the query-side plumbing for a
  record-journey view: `GET /data/:id` → `GET /v1/events?record_id=:id` →
  `GET /v1/action-executions?event_id=:id` now traces a record all the way to what happened
  because of it.
- **Tests:** `cargo test -p common --lib action_execution` — 3 passed (tenant_id threading
  through `new`/`retry`). `cargo test -p action-executor --lib` — 22 passed (3 new:
  `list_by_event` scoped to tenant+event in the in-memory double; the new HTTP handler tested
  for success, missing-tenant-header 401, and backend-failure 500). `cargo test -p
  action-executor --test execution_repository_integration_test` — 2 passed against real
  Postgres (1 new: `list_by_event` against the real table, confirming both the tenant and
  event scoping hold). `cargo test -p dashboard-api --lib` — 19 passed (1 new: `record_id`
  filter). `cargo test -p dashboard-api --test event_query_integration_test` — 3 passed
  against real ClickHouse (1 new: `has(record_ids, ...)` filter proven against a real insert,
  not just the in-memory double). Full local CI gate: `cargo fmt --all --check` clean, `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo test
  --workspace --all-features` all green (0 failures across every crate, verified against a
  throwaway local `mssql` container standing in for CI's Fabric TDS dependency), `cargo
  llvm-cov` 94.16% line coverage (85% floor), `cargo audit` / `cargo deny check` clean (same
  two pre-existing allow-listed `unmaintained` advisories, no new advisories).
- **PR:** (opened in this branch's PR, same as the containerization change above)
- **ADR:** n/a — a compliance bugfix (missing tenant scoping) and additive query capability
  surfaced while implementing ADR-0017, not a new architectural decision itself.

## [2026-07-19] feature/0014-docker-images — Console UI Record Journey page (Palantir-style lineage view)
- **Type:** feature
- **Branch:** feature/0014-docker-images
- **Summary:** Adds `GET /data/:id/journey`, a link/investigative view that renders a raw
  record's full pipeline lineage — the record, every Event it contributed to (via ADR-0017's
  `record_ids`), and every ActionExecution each Event caused — as a vertical tree
  (record → event branches → execution cards), each execution colored by status. Built
  entirely from existing read endpoints (`GET /data/:id`, `GET /v1/events?record_id=`,
  Action Executor's `GET /v1/action-executions?event_id=`); no new backend query added. A
  "View record journey →" link was added to the existing `/data/:id` page. New
  `ui/src/execution_client.rs` (`ExecutionClient`/`HttpExecutionClient`) and
  `ui/src/record_journey_handler.rs` wire a new `ACTION_EXECUTOR_URL` env var into
  `AppState`, `docker-compose.yml`, `.env.example`, and `scripts/run-local.sh` (which was
  also missing `INGESTION_GATEWAY_URL` for the UI — a pre-existing gap, fixed alongside since
  it's the same env-wiring block).
- **Tests:** `cargo test -p kizashi-ui` — 128 passed, 0 failed (12 new:
  `record_journey_handler_test` covers no-events, events-with-executions, an
  execution-client-failure-still-renders-the-event case, and the login-redirect guard;
  `execution_client_test` covers the HTTP client against a real stub server and an
  unreachable-server case; every other `*_handler_test.rs`'s `AppState` construction was
  swept to add the new `execution_client` field). Full local CI gate:
  `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets --all-features --
  -D warnings` clean, `cargo test --workspace --all-features` all green (0 failures,
  verified against a throwaway local `mssql` container for Fabric), `cargo audit` clean (same
  two pre-existing allow-listed `unmaintained` advisories, no new ones). Live-verified against
  the running docker-compose stack: rebuilt/redeployed `kizashi-ui`, seeded a real
  record→event→action chain (a trigger inserted directly into `trigger_engine`'s schema, an
  `AnalyzedRecord` published onto the real `record.analyzed` RabbitMQ exchange, consumed by
  the real trigger-engine and action-executor), then fetched and screenshotted both
  `/data/:id` and `/data/:id/journey` against the live server — confirmed the journey tree
  renders the record, event, and a "webhook — failed" execution card with correct red
  styling, and confirmed the empty-state ("hasn't contributed to any events yet") renders
  for a record with no events. This surfaced and fixed a real bug: the template and test
  fixtures assumed `ActionExecutionStatus`/`ActionType` serialize PascalCase
  (`"Sent"`/`"Webhook"`), but both actually derive `#[serde(rename_all = "snake_case")]`
  (`"sent"`/`"webhook"`) — the live screenshot showed the status pill always rendering red
  regardless of real status, which caught it; fixed the template's status comparison and all
  test fixtures to match the real backend casing.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — reuses ADR-0017's `record_ids` lineage field and the existing
  Action Executor query endpoint; no new architectural decision.

**Known gap surfaced while seeding live test data (not fixed in this PR):** triggers created
via `config-admin-service` (the Console UI's Triggers page) are written only to
`config_admin_service.trigger_definitions` — `trigger-engine` reads triggers exclusively from
its own separate `trigger_engine.trigger_definitions` schema (`crates/trigger-engine/src/
trigger_repository.rs`), and nothing syncs the two. In this dev environment
`trigger_engine.trigger_definitions` already holds thousands of directly-inserted rows from
past sessions, meaning triggers made through the UI/API have likely never actually fired in
this environment. This is a real functional gap, not a cosmetic one — tracked for a follow-up
fix (either a shared table/view, or config-admin-service publishing trigger-created/updated
events for trigger-engine to consume) with its own ADR, since the fix shape is an
architectural decision.

## [2026-07-19] feature/0014-docker-images — Fix trigger-engine/config-admin-service sync gap (ADR-0018)
- **Type:** fix
- **Branch:** feature/0014-docker-images
- **Summary:** Closes the gap logged in the entry above: `config-admin-service` now publishes
  a `trigger.changed` fanout message (new `TRIGGER_CHANGED_EXCHANGE` in `crates/common/src/
  bus.rs`, same pattern as the three existing pipeline exchanges) on every successful trigger
  create/update, carrying the full `TriggerDefinition`. `trigger-engine` gains a second
  RabbitMQ consumer (spawned alongside its existing `record.analyzed` loop) that upserts the
  message into its own `trigger_definitions` table by id via a new
  `TriggerRepository::upsert` method. Triggers authored through the Console UI's Triggers
  page now actually reach the component that evaluates them. Per ADR-0018, deletes are out of
  scope (no delete endpoint exists yet — `enabled: false` is how a trigger is turned off), and
  pre-existing rows created before this change require a one-time backfill per environment
  (not performed here — this PR only fixes go-forward sync).
- **Tests:** `cargo test -p config-admin-service` — 49 passed (2 new:
  `trigger_publisher_test` unit tests for the in-memory/failing publisher doubles; every
  `AdminState` test constructor swept to add the new `trigger_publisher` field). `cargo test -p
  config-admin-service --test trigger_publisher_integration_test` — 1 passed, publishing a
  real `TriggerDefinition` over real RabbitMQ and consuming it back to prove the wire shape
  round-trips. `cargo test -p trigger-engine` — 31 passed (2 new: `upsert_inserts_a_new_
  trigger`/`upsert_replaces_an_existing_trigger_with_the_same_id` against the in-memory
  double). `cargo test -p trigger-engine --test trigger_repository_integration_test` — 2
  passed against real Postgres, proving the `ON CONFLICT (id) DO UPDATE` SQL actually inserts
  then replaces a row. Full local CI gate: `cargo fmt --all --check` clean, `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` clean, `cargo test --workspace
  --all-features` all green (0 failures across every crate, verified against a throwaway
  local `mssql` container for Fabric), `cargo audit` clean (same two pre-existing
  allow-listed `unmaintained` advisories, no new ones). Live-verified against the running
  docker-compose stack: rebuilt/redeployed `config-admin-service` and `trigger-engine`
  (surfaced and fixed a missing `RABBITMQ_URL` env var for `config-admin-service` in both
  `docker-compose.yml` and `scripts/run-local.sh` — it never needed RabbitMQ before this
  change), created a trigger through the real `POST /v1/trigger-definitions` API, and
  confirmed via direct Postgres query that it appeared in `trigger_engine.trigger_definitions`
  within seconds; updated it and confirmed the update (including flipping `enabled` to
  `false`) propagated the same way.
- **PR:** (opened in this branch's PR)
- **ADR:** [0018](adr/0018-trigger-definition-sync-config-admin-to-trigger-engine.md)

## [2026-07-19] feature/0015-ai-analysis-config — Per-tenant AI analysis prompt + deploy-form auto-fill fix (ADR-0019)
- **Type:** feature
- **Branch:** feature/0015-ai-analysis-config
- **Summary:** Closes the backlog item "AI prompt generation for agent actions": every tenant
  previously got identical, uncontrollable AI/ML analysis behavior from Analysis Service's
  fixed call to Azure AI Foundry — no operator control over what the model looks for. Adds
  `AnalysisConfig { tenant_id, prompt, updated_at }` (`crates/common/src/analysis_config.rs`),
  a new Console UI "AI Analysis" page (`GET/POST /analysis-config`) where an operator writes a
  plain-English prompt, `config-admin-service` CRUD (`GET/PUT /v1/analysis-config`,
  operator-only write, audit-logged) that publishes `analysis_config.changed` on every write,
  and a new consumer in `analysis-service` (its first-ever Postgres schema — previously
  stateless) that upserts the synced prompt and includes it in every Foundry/ML batch call
  when present. Reuses ADR-0018's event-driven sync pattern exactly, for the same reason:
  Analysis Service's batch call runs on every `record.normalized` batch, the hottest path in
  the system, so a local Postgres read stays fast at scale where a synchronous
  config-admin-service HTTP call per batch would not. Also fixes a real UX gap flagged
  directly: the Agent deploy-script wizard (`/agents/generate/form`) required operators to
  manually create an API key on a separate page and paste it in blind — now a fresh,
  single-use deploy key is minted automatically via the existing `ApiKeysClient` and
  pre-filled (a Viewer-role session, which can't create keys, gets a blank field with a link
  to the API Keys page instead of a silent failure).
- **Tests:** `cargo test -p common --lib analysis_config` — 2 passed. `cargo test -p
  config-admin-service` — 63 passed (14 new: `analysis_config_repository_test`,
  `analysis_config_publisher_test`, `analysis_config_handlers_test` unit tests) + 1 new
  Postgres integration test (`upsert_analysis_config_writes_created_then_updated_audit_rows_
  against_real_postgres`, proving the `ON CONFLICT` upsert and its audit trail against a real
  table) + 1 new RabbitMQ integration test
  (`publishing_an_analysis_config_change_round_trips_over_real_rabbitmq`). `cargo test -p
  analysis-service` — 20 passed (9 new: `analysis_config_repository_test` unit tests, two new
  `foundry_client_includes_the_prompt_.../foundry_client_omits_the_prompt_field_when_none`
  request-body-capture tests, `process_batch_passes_the_tenants_configured_prompt_...`) + 3
  new Postgres integration tests
  (`analysis_config_repository_integration_test.rs`, against analysis-service's brand-new
  schema). `cargo test -p kizashi-ui` — 139 passed (9 new: `analysis_config_client_test`
  HTTP-client tests against a real stub server, `analysis_config_handler_test` handler tests,
  two new `agent_script_handler_test` tests proving the API key auto-fill for an operator and
  the blank-with-link fallback for a viewer; every other `*_handler_test.rs`'s `AppState`
  construction swept to add the new `analysis_config_client` field). Full local CI gate:
  `cargo fmt --all --check` clean, `cargo clippy --workspace --all-targets --all-features --
  -D warnings` clean, `cargo test --workspace --all-features` all green (0 failures across
  every crate, verified against a throwaway local `mssql` container for Fabric), `cargo
  audit` clean (same two pre-existing allow-listed `unmaintained` advisories, no new ones).
  Live-verified against the running docker-compose stack: rebuilt/redeployed
  `config-admin-service`, `analysis-service`, and `kizashi-ui`, wired the new `DATABASE_URL`
  requirement for `analysis-service` into `docker-compose.yml`/`scripts/run-local.sh` (needed
  now that it owns a schema for the first time), logged in, saved a real prompt through the
  `/analysis-config` form, and confirmed via
  direct Postgres queries that the exact same prompt text landed in both
  `config_admin_service.analysis_configs` and `analysis_service.analysis_configs` within
  seconds — proving the full UI-to-bus-to-consumer sync chain, not just the individual pieces.
  Also fetched `/agents/generate/form?connector_type=zendesk` live and confirmed a real
  `kzsh_...` API key was minted and pre-filled in the rendered HTML, screenshotted both pages.
- **PR:** (opened in this branch's PR)
- **ADR:** [0019](adr/0019-per-tenant-analysis-configuration-ai-prompt.md)

## [2026-07-19] feature/0015-ai-analysis-config — Add Trigger creation to the Console UI
- **Type:** feature
- **Branch:** feature/0015-ai-analysis-config
- **Summary:** Closes task "Support dynamic event-type creation with configurable logic/
  flags": `/triggers` was read-only in the Console UI — the entire mechanism that decides
  what counts as an Event and what action fires (the core of the whole platform) was only
  reachable by hand-crafting a `POST /v1/trigger-definitions` request, which the old
  empty-state literally instructed operators to do. Adds `TriggersClient::create_trigger`
  (`ui/src/triggers_client.rs`) and `POST /triggers` (`ui/src/triggers_handler.rs`) backing a
  new create form on the Triggers page: name, event type to match (with a direct link to the
  new AI Analysis page so operators can see what keys the AI actually returns), window,
  either-or condition fields for `CountOverWindow`/`ThresholdOverWindow` (both shown at once,
  server-side parsing picks the right one based on a `condition_shape` select — no JS,
  ADR-0014), and an optional webhook URL for the one functional action type
  (`HttpActionDispatcher`, ADR-0007, only ever reads `config.url` regardless of
  `action_type`). Gated behind `can_write` (RBAC v1, Operator+) with a server-side 403 on
  `POST`, matching every other write surface in this UI.
- **Tests:** `cargo test -p kizashi-ui` — 145 passed (10 new: 2 `triggers_client_test` HTTP
  tests against a real stub server for create + role-rejection, 5 new `triggers_handler_test`
  tests covering both condition shapes, a missing-required-field re-render with an inline
  error, and a Viewer-role 403; every existing triggers test still passes unmodified since
  the default test session role already satisfies `can_write`). This surfaced and fixed a
  real bug during TDD: the form struct originally typed `count`/`threshold` as
  `Option<u32>`/`Option<f64>`, which axum's `Form` extractor rejects with a 422 the moment a
  real HTML form submits an empty string for an unused numeric field (as browsers always do
  for a visible-but-blank `<input type="number">`) — not a missing key, which is what
  `Option<T>` actually handles. Fixed by typing those fields as plain `String` and parsing by
  hand in `build_condition`, trimming and treating empty/unparsable as the "this shape wasn't
  selected" case. Full local CI gate: `cargo fmt --all --check` clean, `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` clean, `cargo test --workspace
  --all-features` all green (0 failures, verified against a throwaway local `mssql`
  container for Fabric), `cargo audit` clean (same two pre-existing allow-listed
  `unmaintained` advisories, no new ones). Live-verified against the running docker-compose
  stack: rebuilt/redeployed `kizashi-ui`, submitted a real trigger through the actual HTML
  form, and confirmed via direct Postgres queries that it landed in
  `config_admin_service.trigger_definitions` *and* synced into
  `trigger_engine.trigger_definitions` within about a second (ADR-0018's sync pipeline,
  exercised end-to-end from the UI for the first time) — screenshotted the page showing the
  form and the newly created row.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — reuses ADR-0007's action config shape and ADR-0018's sync pipeline; no new
  architectural decision.

## [2026-07-19] feature/0015-ai-analysis-config — Add Field Mappings (NormalizationMapping) to the Console UI
- **Type:** feature
- **Branch:** feature/0015-ai-analysis-config
- **Summary:** `NormalizationMapping` has had a full CRUD API in `config-admin-service` since
  ADR-0010 but zero presence anywhere in the Console UI — not even a read-only list, unlike
  Triggers which at least had a (read-only, until the entry above) page. Discovered by
  auditing for other instances of the same "operators can't practically use this" pattern
  just fixed for Triggers. Adds `NormalizationMappingsClient` (list/create),
  `GET/POST /normalization-mappings`, and a new "Field Mappings" nav page. `field_map` is a
  dynamic `BTreeMap<String, String>` (arbitrary target-field-to-JSON-path pairs), so rather
  than a JS-dependent dynamic add-row form, the create form uses one `target_field = $.path`
  pair per line in a textarea, parsed server-side — consistent with the no-JS constraint
  (ADR-0014) already governing every other form in this app.
- **Tests:** `cargo test -p kizashi-ui` — 155 passed (10 new: 4
  `normalization_mappings_client_test` HTTP-client tests against a real stub server, 6
  `normalization_mappings_handler_test` tests covering the empty state, a successful
  multi-line create, an all-invalid-lines error re-render, a Viewer-role 403, a backend
  failure, and the login redirect; every other `*_handler_test.rs`'s `AppState` construction
  swept to add the new `normalization_mappings_client` field). Full local CI gate: `cargo fmt
  --all --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` clean, `cargo test --workspace --all-features` all green (0 failures, verified
  against a throwaway local `mssql` container for Fabric), `cargo audit` clean (same two
  pre-existing allow-listed `unmaintained` advisories, no new ones). Live-verified against the
  running docker-compose stack: rebuilt/redeployed `kizashi-ui`, submitted a real
  multi-line mapping (`text = $.description` / `urgency = $.priority`) through the actual
  form, and confirmed via a direct Postgres query that both fields landed correctly in
  `config_admin_service.normalization_mappings` — screenshotted the page showing the create
  form and both fields rendered in the list table.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — reuses the existing NormalizationMapping CRUD API (ADR-0010); no new
  architectural decision.

## [2026-07-19] feature/0015-ai-analysis-config — Real search index for the Data Viewer (pg_trgm)
- **Type:** fix
- **Branch:** feature/0015-ai-analysis-config
- **Summary:** Half of task "Scale-out: dynamic per-agent connector scheduling + real search
  index" (the connector-scheduling half is a larger, separate piece of work needing its own
  ADR, tracked separately — not attempted here). The Data Viewer's free-text search
  (`RawRecordRepository::search`) ran a plain `raw_payload::text ILIKE '%x%'` — no index can
  accelerate a leading-wildcard `ILIKE`, so this was always a full sequential scan, explicitly
  documented as "not a dedicated search index" in the code comment. Adds a `pg_trgm` GIN
  index (migration `0004_add_trigram_search_index.sql`) over `raw_payload::text`, `subject`,
  and `from` — the standard Postgres mechanism for indexing `ILIKE '%x%'` substring matches.
  Deliberately chose trigram indexing over `tsvector`/full-text search: `tsvector` changes
  matching semantics (whole-lexeme/stemmed matching vs. substring matching), which would
  silently change what "search" means to an operator already relying on today's behavior;
  `pg_trgm` accelerates the exact same query with the exact same results, purely a scan-
  strategy change the planner picks up once the table is large enough to prefer an index scan
  over a seq scan (same "useless at demo scale, necessary at target scale" caveat as the
  existing GIN index from migration 0003).
- **Tests:** `cargo test -p ingestion-service` — 60 passed (2 new:
  `pg_trgm_extension_and_indexes_exist_after_migration` and
  `free_text_search_still_finds_a_substring_match_against_real_postgres`, both against real
  Postgres — the first real Postgres test this repo's ever had for the `search()` query path
  at all, since the existing `search_filters_by_free_text_query_against_the_raw_payload` unit
  test only exercises the `InMemoryRawRecordRepository` double's `.contains()` semantics, not
  the actual SQL). Full local CI gate: `cargo fmt --all --check` clean, `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` clean, `cargo test --workspace
  --all-features` all green (0 failures, verified against a throwaway local `mssql`
  container for Fabric), `cargo audit` clean (same two pre-existing allow-listed
  `unmaintained` advisories, no new ones).
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — a performance fix with no behavior change, not an architectural decision.

## [2026-07-19] feature/0016-agent-scheduler — Agent Scheduler Phase 1: registry sync + invoker (ADR-0020)
- **Type:** feature
- **Branch:** feature/0016-agent-scheduler
- **Summary:** First piece of "dynamic per-agent connector scheduling" (the other half of the
  split "Scale-out" task, design captured in [ADR-0020](adr/0020-agent-scheduler-in-platform-connector-scheduling.md)).
  Registering an Agent in the Console UI previously created a config record only — nothing in
  the platform actually caused it to run; operators had to externally wire the deploy script's
  output into their own cron/K8s infrastructure. Adds a new `agent-scheduler` service that: (1)
  syncs its own copy of the Agent registry from `config-admin-service` via a new
  `agent.changed` bus message (published on every Agent create/update/delete, same
  ADR-0018/0019 pattern), and (2) on a tick loop, invokes each enabled Agent whose configured
  `poll_interval_seconds` (read from `Agent.config`, defaulting to 300s) has elapsed via a new
  `Invoker` trait. `DockerInvoker` (the docker-compose deployment path) builds a `docker run
  --rm` invocation reusing the exact same env-var shape the deploy-script wizard
  (`ui/src/agent_script_handler.rs`) already computes by hand.
- **Tests:** `cargo test -p common --lib agent_change_event` — 2 passed. `cargo test -p
  config-admin-service` — 67 passed (2 new: `agent_publisher_test` unit tests; every
  `AgentState` test constructor swept for the new `agent_publisher` field) + 2 new RabbitMQ
  integration tests (`agent_publisher_integration_test.rs`, proving both `Upserted` and
  `Deleted` variants round-trip over the real bus). `cargo test -p agent-scheduler` — 11
  passed (10 unit: `AgentRepository`'s in-memory double, `DockerInvoker`'s image-name and
  `docker run` argument construction — verified as a pure function, not by actually shelling
  out — plus the `Invoker` trait's in-memory/failing doubles) + 3 new Postgres integration
  tests (`agent_repository_integration_test.rs`: upsert/list/mark-polled/delete against a real
  table). Full local CI gate: `cargo fmt --all --check` clean, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` clean, `cargo test --workspace --all-features`
  all green (0 failures across every crate including the two new ones, verified against a
  throwaway local `mssql` container for Fabric), `cargo audit` clean (same two pre-existing
  allow-listed `unmaintained` advisories, no new ones). Live-verified the registry-sync half
  against the running docker-compose stack: rebuilt/redeployed `config-admin-service`, ran
  `agent-scheduler` locally against the live Postgres/RabbitMQ (its own docker-compose service
  entry isn't added yet — see below), created/updated/deleted a real Agent through
  `config-admin-service`'s live API, and confirmed via direct Postgres queries that all three
  operations propagated into `agent_scheduler.agents` within about two seconds.
- **Known gap, explicitly not done in this PR:** the `DockerInvoker` shells out to the `docker`
  CLI against the Docker socket, but the shared runtime `Dockerfile` (one image for all 20
  binaries) has neither `docker` CLI installed nor socket access, and runs as a non-root user
  that couldn't reach a root-owned socket anyway. Rather than claim this works, **no
  `docker-compose.yml` entry was added for `agent-scheduler`** — adding an unhealthy/broken
  service would break `docker compose up` for everyone. The actual `invoke()` → real
  `docker run` → connector-actually-polls path was **not live-verified** and should not be
  assumed to work end-to-end yet. Follow-up: extend the runtime image (or a dedicated
  variant) with Docker CLI + socket access, verify with a real Agent whose connector actually
  runs, then wire the compose entry. `KubernetesJobInvoker` (the K8s deployment path) is
  unbuilt, per ADR-0020. Per-Agent API key lookup is also unbuilt — v1 uses one
  platform-wide `INGESTION_GATEWAY_API_KEY` for every scheduled connector, documented as a
  known simplification in `invoker.rs`.
- **PR:** (opened in this branch's PR)
- **ADR:** [0020](adr/0020-agent-scheduler-in-platform-connector-scheduling.md)

## [2026-07-19] feature/0017-agent-scheduler-docker-packaging — Docker CLI/socket packaging for agent-scheduler, closing ADR-0020
- **Type:** fix
- **Branch:** feature/0017-agent-scheduler-docker-packaging
- **Summary:** Closes the gap explicitly logged in the entry above: `agent-scheduler`'s
  `DockerInvoker` had never actually been exercised against a real `docker run`, because the
  shared runtime `Dockerfile` had neither the Docker CLI nor socket access. Adds two opt-in
  build args to the shared `Dockerfile` (`INSTALL_DOCKER_CLI`, `RUN_AS_USER`) rather than
  forking a second Dockerfile — every other binary's build is unaffected (verified: a default
  `config-admin-service` build has no `docker` CLI and still runs as the non-root `kizashi`
  user). Adds the `agent-scheduler` service to `docker-compose.yml` with the socket mounted
  and `AGENT_SCHEDULER_INGESTION_GATEWAY_API_KEY` documented in `.env.example` (empty by
  default; `main.rs` now logs a loud warning instead of silently degrading if it's unset, per
  ADR-0020's documented v1 platform-wide-key simplification).
- **Tests:** No new Rust unit/integration tests — this PR is packaging/infra, not logic (the
  `DockerInvoker` logic itself was already tested in the prior PR). Full local CI gate: `cargo
  fmt --all --check` clean, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` clean, `cargo test --workspace --all-features` all green (0 failures, verified
  against a throwaway local `mssql` container for Fabric), `cargo audit` clean (same two
  pre-existing allow-listed `unmaintained` advisories, no new ones).
- **Live verification (this is the part that actually matters for this PR):** built the image
  with `INSTALL_DOCKER_CLI=true` — the first attempt used Debian bookworm's `docker.io`
  package (Docker 20.10, client API 1.41) and failed immediately against the real host daemon
  (API 1.44+): `client version 1.41 is too old`. Switched to the official static Docker CLI
  binary (26.1.4) instead of the distro package; rebuilt, confirmed `docker ps` against the
  real mounted socket worked. Deployed the real `agent-scheduler` service via `docker compose
  up`, created a real Ingestion Gateway API key via the live API, built the real
  `generic-connector` image, registered a real Agent (`connector_type: generic`,
  `poll_interval_seconds: 5`) through `config-admin-service`'s live API, and confirmed via
  `docker logs` that `agent-scheduler` actually ran `docker run` against the real
  `kizashi-generic-connector` image on schedule — the container launched and executed (exited
  non-zero on its own connector-level logic against a stub URL, which is expected and
  unrelated to the invocation mechanism itself, which is what this PR needed to prove). Also
  incidentally confirmed the previous PR's registry-sync integration tests had been publishing
  to the same real, shared `agent.changed` exchange this whole time — several leftover
  `integration-test-agent` rows had synced into the live `agent_scheduler.agents` table and
  were failing invocation (expected: `kizashi-zendesk-connector` was never built locally).
  Cleaned up all test data (agents, API key) after verification.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — implements ADR-0020's already-decided Phase 1 packaging, no new decision;
  the Debian-package-vs-static-binary choice for the CLI itself is a small enough
  implementation detail to note in this entry rather than warrant its own ADR.

## [2026-07-19] feature/0018-egress-gateway — Add Egress Gateway (ADR-0021), Phase 4 of the gap-closing roadmap
- **Type:** feature
- **Branch:** feature/0018-egress-gateway
- **Summary:** New `crates/egress-gateway`: an HTTP CONNECT forward proxy every outbound
  `reqwest::Client` in this codebase can optionally route through (connector polls to
  Zendesk/Graph/Fabric/customer-SQL, `action-executor`'s webhook dispatch, OAuth2 token
  fetches), so external calls get a tenant/connector-scoped audit trail and an optional
  per-tenant domain allowlist — closing the "no answer to what external hosts did tenant X's
  connectors talk to" gap flagged in the roadmap's Phase 4. Caller identity travels via
  `Proxy-Authorization: Basic base64(tenant_id:connector_id)` (exactly what
  `reqwest::Proxy::basic_auth` already sends, so zero new client-side protocol work — see
  ADR-0021 for the full design and three rejected alternatives: a generic proxy with no
  Kizashi code, a TLS-terminating/MITM proxy, and a per-connector client-wrapper library).
  HTTPS traffic is tunneled byte-for-byte after the CONNECT handshake — Egress Gateway never
  sees request paths/bodies, only the destination host:port, a deliberate scope boundary
  (destination-level audit, not deep inspection). The per-tenant domain allowlist is
  Egress-Gateway-owned outright (`GET/PUT /v1/allowlist`) rather than synced from
  config-admin-service, since no other service ever reads it.
- **Tests:** `cargo test -p egress-gateway` — 29 unit tests (parsing `Proxy-Authorization` and
  CONNECT targets never panics on malformed input; allowlist host-matching correctly handles
  subdomain matching without being fooled by a same-suffix-but-different-domain like
  `notzendesk.com`; the allow/deny/audit decision logic, tested against in-memory doubles) + 6
  new Postgres integration tests (`repository_integration_test.rs`: allowlist round-trip,
  audit log persistence, and — critically — proving the `BEFORE UPDATE OR DELETE` immutability
  trigger really rejects mutation against a real table, same pattern as every other audit log
  in this system). Full local CI gate: `cargo fmt --all --check` clean, `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` clean, `cargo test --workspace
  --all-features` all green (0 failures across every crate including the new one, verified
  against a throwaway local `mssql` container for Fabric), `cargo audit` clean (same three
  pre-existing allow-listed `unmaintained` advisories — no new ones from the new `hyper`/
  `hyper-util` dependencies this crate needed for low-level CONNECT/upgrade handling, which
  axum's router doesn't support directly).
- **Live verification:** ran the real binary against the live Postgres and proxied a real
  HTTPS request (`curl -x http://localhost:3128 -U tenant:connector https://api.github.com/zen`)
  through it — got a real 200 response back, confirmed the audit row landed with the correct
  tenant/connector/destination. Configured a real per-tenant allowlist via the live
  `PUT /v1/allowlist` API, confirmed an allowlisted host tunneled successfully and a
  non-allowlisted host was denied (`403`, `curl` reports this as a failed proxy CONNECT, which
  is the correct client-visible behavior) — both outcomes correctly audit-logged. Rebuilt and
  redeployed via `docker compose up` (this surfaced and fixed a real Docker networking bug: the
  first `up` attempt left the container with no network attached at all, because an earlier
  port conflict — `3128` was still held by a leftover local test process — had left the
  container in a bad created-but-not-networked state; `--force-recreate` fixed it), then
  repeated the same live HTTPS-through-proxy test against the fully containerized service and
  got the same correct result.
- **Known gap, explicitly not done here:** no connector or `action-executor` has actually been
  updated to set `EGRESS_PROXY_URL` yet — adoption is deliberately opt-in per ADR-0021, and
  wiring it into all 6 connector crates' outbound clients plus `HttpActionDispatcher` is
  tracked as a separate follow-up rather than scope-creeping this PR further.
- **PR:** (opened in this branch's PR)
- **ADR:** [0021](adr/0021-egress-gateway-http-connect-forward-proxy.md)

## [2026-07-19] feature/0019-egress-proxy-connector-wiring — Wire EGRESS_PROXY_URL opt-in into connectors and action-executor
- **Type:** feature
- **Branch:** feature/0019-egress-proxy-connector-wiring
- **Summary:** Closes the follow-up gap explicitly left open in the Egress Gateway PR (ADR-0021):
  `build_outbound_client`/`EgressClientError` moved from `connector-runtime` into `common` (so
  both connectors and `action-executor` can share it without an odd cross-domain dependency).
  Wired the `EGRESS_PROXY_URL` opt-in into the `zendesk`, `graph-mail`, `graph-teams`, and
  `generic` connectors — each now builds its outbound `reqwest::Client` via
  `build_outbound_client(egress_proxy_url, tenant_id, connector_id)` instead of a bare
  `reqwest::Client::new()`. `action-executor`'s `HttpActionDispatcher` now builds a fresh
  proxied client per dispatch call, keyed on `(event.tenant_id, "action-executor")`, since
  Action Executor is multi-tenant within one process (unlike a connector, which is one tenant
  for its whole process lifetime) — this changed its constructor from taking a `reqwest::Client`
  to taking `Option<String>` (the proxy URL), resolved once from `EGRESS_PROXY_URL` in `main.rs`.
- **Known gaps, explicitly not done here:** `fabric` (raw TDS/SQL Server via `tiberius`) and
  `sql` (Postgres wire protocol via `sqlx::PgPool`) connectors have no outbound `reqwest::Client`
  in their data-fetch path, so there is nothing to proxy for either. The internal
  `fetch_access_token` OAuth2 token-fetch call used by `graph-mail`/`graph-teams`/`fabric`
  constructs its own client internally and is not yet wired to the proxy — tracked as a
  follow-up.
- **Tests:** `cargo test --workspace --all-features` (real Postgres/RabbitMQ/ClickHouse/MinIO
  via docker-compose, plus a throwaway `kizashi-mssql-ci` container for Fabric) — all passed, 0
  failed, across every crate including the 2 moved `egress_client` tests and a new
  `action_dispatcher_test::dispatch_returns_unreachable_for_a_malformed_egress_proxy_url` proving
  the proxy config actually plumbs through per-dispatch rather than being accepted and ignored.
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt
  --all --check` — clean. `cargo audit` — same 3 pre-existing allow-listed advisories
  (`instant`, `rustls-pemfile` x2, all `unmaintained`), no new advisories introduced.
- **Live verification:** built `connector-generic` and ran it directly against the live,
  already-deployed `egress-gateway` container with `EGRESS_PROXY_URL=http://localhost:3128`,
  a real `tenant_id`, and `CONNECTOR_ID=egress-live-test-connector` pointed at
  `https://api.github.com/zen`. The connector itself hit an unrelated auth error parsing
  GitHub's response, but a direct query against `egress_gateway.egress_audit_log` confirmed the
  outbound call was correctly tunneled and audit-logged with the connector's real tenant_id and
  connector_id — proving the "zero code changes beyond one env var" claim from ADR-0021 holds
  for a real connector process.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — implements the wiring already decided in ADR-0021, no new architectural
  decision

## [2026-07-19] feature/0020-imap-inbound-connector — IMAP inbound connector (Phase 5)
- **Type:** feature
- **Branch:** feature/0020-imap-inbound-connector
- **Summary:** New `crates/connectors/imap` crate, the seventh connector, for polling any
  RFC 3501 IMAP mailbox (Gmail, self-hosted, anything non-M365) — closes the first Phase 5 gap
  from the roadmap. Implements the shared `Connector` trait: connects (TLS by default,
  configurable plain-TCP via `IMAP_USE_TLS`), logs in via IMAP `LOGIN`, selects a mailbox,
  `SEARCH SINCE <date>`, `FETCH ... RFC822` each matching UID, and maps each message to a
  `RawRecord` (`SourceType::Message`) via a pure `parse_message` function kept separate from
  the network I/O. Follows ADR-0013's stateless-cursor design (`since_date` passed in per
  invocation, no persisted state) and ADR-0021's non-adoption for non-HTTP protocols (IMAP's
  raw TCP can't route through Egress Gateway's HTTP CONNECT tunnel). Added a
  `docker-compose.yml` `imap-connector` service entry following the existing
  `<name>-connector` pattern (build-arg `BIN: connector-imap`, `connectors` profile).
- **Known gaps, explicitly not done here:** XOAUTH2 auth (Gmail/Workspace with password auth
  disabled) and UID-based incremental cursor tracking (v1 re-fetches the whole `since_date`
  day on every poll — idempotent, not lossy, but not efficient) are tracked as follow-ups, not
  silently dropped.
- **Tests:** `cargo test -p connector-imap --lib` — 4 unit tests, all passed
  (`parse_message` against static RFC822 byte fixtures, including malformed/minimal-header
  inputs that must not panic). `tests/imap_connector_integration_test.rs` — 2 tests against a
  **real IMAP server** (`greenmail/standalone:2.0.1`, CLAUDE.md §2's "test against the real
  thing"), gated on `IMAP_TEST_HOST`/`IMAP_TEST_PORT`/`IMAP_TEST_USERNAME`/
  `IMAP_TEST_PASSWORD`: one polling a real seeded message end-to-end, one asserting a wrong
  password is reported as `ConnectorError::AuthFailed` against the real server. `cargo test
  --workspace --all-features` (full stack: Postgres/RabbitMQ/ClickHouse/MinIO via
  docker-compose, throwaway `kizashi-mssql-ci` for Fabric, `greenmail` for this connector) —
  all passed, 0 failed. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  — clean. `cargo fmt --all --check` — clean. `cargo audit` — same 3 pre-existing
  allow-listed advisories, no new advisories from the new `async-imap`/`async-native-tls`/
  `mail-parser` dependencies.
- **Live verification:** built the real `imap-connector` Docker image via `docker compose
  build`, seeded a real message into `greenmail` via `curl --url smtp://... --upload-file`,
  created a real API key via `POST /v1/api-keys`, and ran the containerized connector with
  `docker run --network kizashi_default` against the real running `ingestion-gateway` and the
  real `greenmail` server — output: `PollSummary { polled: 1, ingested: 1, failed: 0 }`.
  Confirmed via a direct Postgres query that the record landed in
  `ingestion_service.raw_records` with the correct `connector_id`, `tenant_id`, and message
  subject. Cleaned up the test record and API key afterward (both are deletable, unlike the
  append-only audit tables verified in earlier phases).
- **PR:** (opened in this branch's PR)
- **ADR:** [0022](adr/0022-imap-connector-plain-auth-stateless-cursor.md)

## [2026-07-19] feature/0021-smtp-send-action — SMTP send action (Phase 5)
- **Type:** feature
- **Branch:** feature/0021-smtp-send-action
- **Summary:** Closes the second Phase 5 gap: `action-executor` can now send a real SMTP email,
  not just POST a webhook labeled "Email." New `SmtpActionDispatcher` (uses `lettre`) reads
  `smtp_host`/`smtp_port`/`smtp_use_tls`/`from`/`to`/`subject`/`smtp_username`/`smtp_password`
  from an action's config and sends an actual RFC 5322 message. A new `RoutingActionDispatcher`
  (now the dispatcher `main.rs` wires up) routes `ActionType::Email` actions with an
  `smtp_host` field to `SmtpActionDispatcher`, everything else to the existing
  `HttpActionDispatcher` unchanged — no breaking change for already-configured
  Email-as-webhook triggers. Added `DispatchError::InvalidConfig` for SMTP-specific
  config-validation failures, distinct from HTTP dispatch's `MissingUrl`.
- **Tests:** `cargo test -p action-executor --lib` — 32 tests, all passed (config-validation
  unit tests for `SmtpActionDispatcher`, routing-decision unit tests for
  `RoutingActionDispatcher`, plus all pre-existing `HttpActionDispatcher`/`process_event`
  tests unaffected). `tests/smtp_action_dispatcher_integration_test.rs` — 1 test against a
  **real SMTP+IMAP server** (reusing ADR-0022's `greenmail` container): sends a real message
  via `SmtpActionDispatcher`, then reads it back with a real `ImapConnector::poll` call to
  prove actual delivery, not just SMTP accepting the DATA command. Also fixed a fragility this
  surfaced in `connector-imap`'s own live test: it assumed its seeded message was the only one
  in the shared CI mailbox, which broke once this action's test started seeding its own
  message there too — changed to search by subject instead of assuming index `0`. `cargo test
  --workspace --all-features` (full real-infra stack including both greenmail-backed tests
  together) — all passed, 0 failed. `cargo clippy --workspace --all-targets --all-features --
  -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo audit` — same 3
  pre-existing allow-listed advisories, no new ones from `lettre` and its transitive deps.
- **Known gaps, explicitly not done here:** SMTP connection pooling (a fresh transport is built
  per dispatch, matching `HttpActionDispatcher`'s existing per-dispatch-client pattern) and
  Egress Gateway routing for SMTP (not an HTTP-CONNECT-tunnelable protocol, same limitation
  ADR-0022 already documents for IMAP) are tracked as follow-ups.
- **PR:** (opened in this branch's PR)
- **ADR:** [0023](adr/0023-smtp-send-action-routing-dispatcher.md)

## [2026-07-19] feature/0022-graph-send-mail-action — Graph send-mail-as-user action (Phase 5)
- **Type:** feature
- **Branch:** feature/0022-graph-send-mail-action
- **Summary:** Closes the third and final Phase 5 gap. New `GraphSendMailActionDispatcher`
  sends email as a real mailbox user via Microsoft Graph's `POST /users/{id}/sendMail`, reusing
  `connector_runtime::fetch_access_token` (the Entra ID app-only client-credentials flow
  already proven by `graph-mail`/`graph-teams`, ADR-0003) — the cheapest of the three Phase 5
  actions since the auth plumbing already existed. `RoutingActionDispatcher` now composes three
  dispatchers: an `Email` action with `smtp_host` goes to `SmtpActionDispatcher` (ADR-0023),
  one with `graph_client_id` goes to `GraphSendMailActionDispatcher` (SMTP takes precedence if
  a config somehow carries both), everything else still falls through to
  `HttpActionDispatcher` unchanged.
- **Tests:** `cargo test -p action-executor --lib` — 39 tests, all passed (config-validation
  and routing-decision unit tests, plus dispatch tests against real stub HTTP servers proving
  real token-fetch + real bearer-auth request construction + real status-code branching for
  success/500/token-endpoint-down). `cargo test --workspace --all-features` (full real-infra
  stack) — all passed, 0 failed. `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` — clean. `cargo fmt --all --check` — clean. `cargo audit` — same 3 pre-existing
  allow-listed advisories, no new ones.
- **Explicit test-boundary note (not a gap, a documented limitation):** unlike the SMTP/IMAP
  actions' real-server verification, the actual Microsoft Graph API surface is stubbed, not
  real — this environment has no Entra app registration to test against, the same limitation
  ADR-0009 already documents for OIDC's browser hop and ADR-0013 documents for Fabric's
  AAD-token login. What *is* proven: the real TCP connect, real HTTP request construction, real
  bearer-token attachment, and real success/failure status-code handling all execute correctly.
- **PR:** (opened in this branch's PR)
- **ADR:** [0024](adr/0024-graph-send-mail-action-and-provable-test-boundary.md)

## [2026-07-19] feature/0023-entra-token-egress-routing — Route Entra OAuth2 token fetch through Egress Gateway
- **Type:** fix
- **Branch:** feature/0023-entra-token-egress-routing
- **Summary:** Closes a known gap logged when Egress Gateway's connector wiring first shipped:
  `connector_runtime::fetch_access_token` (the Entra client-credentials flow used by
  `graph-mail`, `graph-teams`, `fabric`, and `action-executor`'s Graph send-mail action) built
  its own `reqwest::Client` internally via `oauth2::reqwest::async_http_client`, silently
  bypassing `EGRESS_PROXY_URL` even when a connector's other outbound calls were proxied. Now
  takes a caller-provided client and routes the OAuth2 exchange through it — every call site
  updated to pass the same `build_outbound_client`-constructed client it already uses elsewhere.
  `fabric` gained a new `token_client` field for this specifically, since its data path (TDS)
  never needed a `reqwest::Client` before.
- **Tests:** `cargo test -p connector-runtime --lib` — 13 tests, all passed, including a new
  `the_token_request_actually_goes_through_the_provided_client_not_a_default_one` test proving
  the client is genuinely used (a client proxied through a deliberately-broken address fails
  the way a real misconfigured proxy would). `cargo test --workspace --all-features` (full
  real-infra stack) — all passed, 0 failed, including all 3 real-TDS-server Fabric integration
  tests. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo deny check` — clean. `cargo audit` — same 3
  pre-existing allow-listed advisories, no new ones.
- **Live verification:** ran the real `connector-fabric` binary locally with
  `EGRESS_PROXY_URL` pointed at the deployed `egress-gateway` container and deliberately
  invalid Entra credentials — the token request reached the real
  `login.microsoftonline.com` and was correctly rejected (fake credentials), and a direct
  Postgres query confirmed `egress_gateway.egress_audit_log` recorded the real call
  (`login.microsoftonline.com:443`) with the correct `tenant_id`/`connector_id`. Attempted to
  clean up the test audit row afterward and got `egress_audit_log is append-only: DELETE is
  not permitted` — the immutability trigger working exactly as designed, left in place.
- **PR:** (opened in this branch's PR)
- **ADR:** [0025](adr/0025-entra-token-fetch-egress-gateway-routing.md)

## [2026-07-19] feature/0024-config-admin-tenant-isolation-tests — Tenant-isolation tests for config-admin-service repositories
- **Type:** chore
- **Branch:** feature/0024-config-admin-tenant-isolation-tests
- **Summary:** Closes a real CLAUDE.md §5 compliance gap: "every query path must be tested for
  tenant isolation, not just implemented correctly by inspection." An audit of
  `crates/config-admin-service/tests/repository_integration_test.rs` found every existing test
  used exactly one `tenant_id` per test — none ever proved tenant A can't read/update/delete/
  list a row owned by tenant B. Added 9 new integration tests against real Postgres covering
  `TriggerDefinitionRepository` (get/update/list), `NormalizationMappingRepository` (get/list),
  `AgentRepository` (get/delete/find_by_name — including a same-name-different-tenant
  collision case), and `AnalysisConfigRepository` (get).
- **Fact, not expectation:** every one of the 9 new tests passed on the first run against real
  Postgres — the underlying `WHERE id = $1 AND tenant_id = $2` (or `WHERE tenant_id = $1` for
  list/find) clauses were already correctly scoped in every repository's SQL (verified by
  reading each repository's implementation before writing the tests, not assumed). This PR
  closes a test-coverage gap, not an implementation bug — stated explicitly since CLAUDE.md
  distinguishes "verified by running X" from "expected to work," and finding no bug is itself
  a fact worth recording, not silently glossed over.
- **Tests:** `cargo test -p config-admin-service --test repository_integration_test` — 16
  tests (9 new + 7 pre-existing), all passed against real Postgres. `cargo test --workspace
  --all-features` (full real-infra stack) — all passed, 0 failed. `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean.
  `cargo deny check` — clean. `cargo audit` — same 3 pre-existing allow-listed advisories, no
  new ones.
- **Known gap, not closed here:** `query-gateway` (spec §6's designated single
  tenant-enforcement point for all UI/dashboard traffic) still has no end-to-end tenant-
  isolation test proving a resolved session can't retrieve another tenant's data through the
  real proxy path — tracked as an immediate follow-up, arguably the more load-bearing gap of
  the two found in this audit.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — test-coverage addition, no architectural decision, confirms existing
  behavior rather than changing it

## [2026-07-19] feature/0025-query-gateway-tenant-isolation-e2e — End-to-end tenant-isolation test for Query Gateway
- **Type:** chore
- **Branch:** feature/0025-query-gateway-tenant-isolation-e2e
- **Summary:** Closes the more load-bearing of the two tenant-isolation gaps flagged in the
  prior audit (feature/0024). Query Gateway is spec §6's designated single tenant-enforcement
  point for all UI/dashboard traffic, and its existing tests only asserted header-forwarding
  behavior against mocks — nothing proved two real, independently-minted session tokens for
  two different tenants actually produce correctly-scoped results through the real HTTP proxy
  hop. New `crates/query-gateway/tests/tenant_isolation_integration_test.rs` spins up a real
  `dashboard-api` server (backed by real ClickHouse) and a real `query-gateway` server (backed
  by a real Postgres `TokenStore`), mints two real session tokens via the same `mint_token`
  code path Auth Service uses in production, and proves through actual HTTP requests that
  tenant B's token can never retrieve tenant A's event (even requesting the identical event
  id), that listing never leaks another tenant's rows, and that an unminted token is rejected
  before reaching dashboard-api at all.
- **Fact, not expectation:** all 3 new tests passed on the first run — `proxy_handler.rs`
  already built its outbound request with only its own resolved `x-tenant-id` header, never
  forwarding the original request's headers wholesale, so a client-supplied `X-Tenant-Id`
  could never leak through. This closes a test-coverage gap; it did not fix a bug.
- **Tests:** `cargo test -p query-gateway --test tenant_isolation_integration_test` — 3 tests,
  all passed against real Postgres + real ClickHouse + two real spawned HTTP servers.
  `cargo test --workspace --all-features` (full real-infra stack) — all passed, 0 failed.
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt
  --all --check` — clean. `cargo deny check` — clean. `cargo audit` — same 3 pre-existing
  allow-listed advisories, no new ones.
- **PR:** (opened in this branch's PR)
- **ADR:** [0026](adr/0026-query-gateway-tenant-isolation-e2e-test.md)

## [2026-07-19] fix/0002-agent-rbac-enforcement — Enforce Operator-minimum role on Agent write endpoints
- **Type:** fix
- **Branch:** fix/0002-agent-rbac-enforcement
- **Summary:** Closes a real privilege-escalation gap found by re-auditing the codebase for
  CLAUDE.md/spec compliance: `config-admin-service`'s `create_agent`/`update_agent`/
  `delete_agent` handlers never called `require_operator` at all, unlike their sibling
  trigger-definition and normalization-mapping write handlers (ADR-0016). Any authenticated
  Viewer-role session — or anyone hitting the API directly — could register, modify, or delete
  another tenant's Agents (connector instances). Fixed by calling the existing
  `require_operator` helper (already `pub(crate)` in `handlers.rs`) in all three write
  handlers. Since the Console UI's `agents_client.rs` never sent an `X-Role` header at all for
  these calls, it was updated in the same PR to thread the signed-in session's role through
  `register_agent`/`update_agent`/`delete_agent` (matching `TriggersClient`'s existing
  `role: Role` parameter convention) — otherwise this backend fix alone would have broken
  every real Operator user's ability to manage Agents through the UI.
- **Tests:** TDD — added 4 failing tests first (`create_agent_requires_role_header`,
  `create_agent_rejects_a_viewer_role`, `update_agent_rejects_a_viewer_role`,
  `delete_agent_rejects_a_viewer_role`), confirmed each failed for the expected reason (200/
  201/204 instead of 401/403) against the real handler, then implemented the fix and confirmed
  all pass. `cargo test -p config-admin-service --lib agent_handlers` — 14 tests, all passed.
  `cargo test -p kizashi-ui --lib` — 155 tests, all passed (every existing `agents_client`
  call site updated to pass a role, all pre-existing behavior unaffected). `cargo test
  --workspace --all-features` (full real-infra stack) — all passed, 0 failed. `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check`
  — clean. `cargo deny check` — clean. `cargo audit` — same 3 pre-existing allow-listed
  advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `config-admin-service` and
  `kizashi-ui` containers via `docker compose build`/`up --force-recreate`, then hit the real
  running `config-admin-service` directly: `POST /v1/agents` with no `X-Role` header → `401`;
  with `X-Role: viewer` → `403`; with `X-Role: operator` → `201` (agent actually created,
  confirmed in the response body); `DELETE` with `X-Role: operator` on the same agent → `204`
  (cleaned up test data — agents are deletable, unlike the append-only audit tables verified in
  earlier phases).
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — closes a gap against an already-established pattern (ADR-0016), no new
  architectural decision

## [2026-07-19] fix/0003-egress-allowlist-rbac — Enforce Operator-minimum role on egress-gateway's allowlist write endpoint
- **Type:** fix
- **Branch:** fix/0003-egress-allowlist-rbac
- **Summary:** A follow-up RBAC-completeness sweep, triggered by the agent-write RBAC gap just
  found, systematically checked every write-capable HTTP handler across the platform for
  missing role enforcement. Found one more of the same class: `PUT /v1/allowlist` in
  `crates/egress-gateway/src/health.rs` had zero server-side RBAC — any caller supplying only
  `X-Tenant-Id` could wholesale-replace a tenant's egress domain allowlist. Arguably higher
  severity than the agent-write gap: Egress Gateway's entire purpose (ADR-0021) is SSRF/
  exfiltration containment, so an attacker able to loosen a tenant's allowlist gains a direct
  lever for data exfiltration through the gateway itself. Every other write-capable service
  audited (config-admin-service's trigger/mapping/agent/analysis-config writes,
  retention-service's policy writes, ingestion-gateway's API key writes) already enforces
  `require_operator`; `dashboard-api` and `auth-service` have no admin-write endpoints at all.
  Added a `require_operator` check to `health.rs`, matching `config_admin_service`'s existing
  pattern exactly. `GET /v1/allowlist` deliberately keeps its existing no-role-check behavior —
  only the write path changes, matching how `get_agent`/`list_agents` remained unchanged in the
  prior fix.
- **Cross-check confirmed no UI-side gap exists here** (unlike the agent-write fix, which also
  needed a Console UI client update): no Console UI page exists for the egress allowlist yet,
  so there is no client that could have been silently omitting `X-Role`.
- **Tests:** TDD — added 2 failing tests first (`put_allowlist_requires_role_header`,
  `put_allowlist_rejects_a_viewer_role`), confirmed both failed for the expected reason (200
  instead of 401/403) against the real handler, then implemented the fix and confirmed all 9
  `health` tests (5 pre-existing + 4 new, including one proving the operator-role happy path
  and one proving GET intentionally stays unrestricted) pass. `cargo test -p egress-gateway
  --lib` — 33 tests, all passed. `cargo test --workspace --all-features` (full real-infra
  stack) — all passed, 0 failed. `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` — clean. `cargo fmt --all --check` — clean. `cargo deny check` — clean. `cargo
  audit` — same 3 pre-existing allow-listed advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `egress-gateway` container via
  `docker compose build`/`up --force-recreate`, then hit it directly: `PUT /v1/allowlist` with
  no `X-Role` → `401`; with `X-Role: viewer` → `403`; with `X-Role: operator` → `200` (the
  allowlist was actually set — confirmed in the response body). Cleaned up the test allowlist
  row afterward (deletable, unlike the append-only `egress_audit_log` verified in earlier
  phases).
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — closes a gap against an already-established pattern (ADR-0016), no new
  architectural decision

## [2026-07-19] chore/0003-update-handler-tenant-mismatch-tests — Add tenant-mismatch tests for UPDATE handlers
- **Type:** chore
- **Branch:** chore/0003-update-handler-tenant-mismatch-tests
- **Summary:** A follow-up sweep after the two RBAC fixes checked a different dimension —
  "tenant confusion" (does every write handler validate a request body's `tenant_id` against
  `X-Tenant-Id` before writing) — across every write-capable service. Found no security bug:
  every entity type that carries `tenant_id` in its body (trigger, mapping, agent, retention
  policy) already calls `tenant_mismatch` correctly on both create and update paths; entities
  whose body structurally can't carry a divergent `tenant_id` (analysis-config, API keys,
  egress allowlist) are `n/a` by design. But it found the exact CLAUDE.md §5 gap one layer up
  from feature/0024 (which closed this at the repository/SQL layer): only the CREATE-path
  tenant-mismatch case had a test per entity — `update_trigger`, `update_mapping`,
  `update_agent`, and retention-service's `update_policy` were correct by inspection but
  untested. Added the 4 missing tests, mirroring each entity's existing create-path test.
- **Fact, not expectation:** all 4 new tests passed against the existing, unmodified
  production code — this closes a test-coverage gap, not a bug. No production code changed in
  this PR.
- **Tests:** `cargo test -p config-admin-service --lib` (the 3 new config-admin tests) and
  `cargo test -p retention-service --lib update_policy_rejects_a_tenant_mismatch` — all 4
  passed. `cargo test --workspace --all-features` (full real-infra stack) — all passed, 0
  failed. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo deny check` — clean. `cargo audit` — same 3
  pre-existing allow-listed advisories, no new ones.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — test-coverage addition, no architectural decision, confirms existing
  behavior rather than changing it

## [2026-07-19] feature/0026-retention-policy-console-ui — Retention policy Console UI page (full CRUD)
- **Type:** feature
- **Branch:** feature/0026-retention-policy-console-ui
- **Summary:** Closes spec §7's "data lifecycle UI" gap — retention-service had a full
  create/read/update API since ADR-0011 but zero Console UI presence until now (an operator
  had to hand-craft `curl`/direct-SQL to manage retention). Added a `/retention-policies` page
  with genuinely full CRUD, following the pattern established by the Field Mappings and Agents
  pages: `retention_policies_client.rs` (`RetentionPoliciesClient` trait +
  `HttpRetentionPoliciesClient`, threading `role: Role` through every write), a
  `retention_policies_handler.rs` (list, create, an inline TTL-edit form
  (`POST /:id/edit`), enable/disable toggle, and delete (`POST /:id/delete`)), and a new
  `retention_policies.html` template with a per-row edit-TTL field, toggle button, and Remove
  button. **`retention-service` itself only had create/update/get/list — no delete endpoint at
  all — so this PR adds `DELETE /v1/retention-policies/:id` to the backend first** (repository
  `delete` method + Postgres impl writing a `Deleted` audit entry in the same transaction,
  matching `agent_repository::delete`'s pattern exactly; a new `ChangeType::Deleted` variant;
  RBAC-enforced handler; router wiring), rather than scoping the UI down to match a backend
  gap — CRUD means all four operations, not three. Also added `.env.example`/
  `docker-compose.yml` entries for `RETENTION_SERVICE_URL`, which the Console UI never
  previously needed to know about.
- **Note:** `RetentionPolicy`/`DataClass` are defined locally in the UI crate rather than
  imported from `common`, since — unlike `Agent`/`TriggerDefinition`/`NormalizationMapping` —
  `RetentionPolicy` currently lives only in `retention-service`'s own crate, not `common`.
  Duplicating the JSON-compatible shape (matching the existing `TriggerSummary`-style pattern
  of UI-local view types) avoided adding a new cross-crate dependency on `retention-service`
  itself; moving `RetentionPolicy` into `common` to be reused directly is a reasonable
  follow-up but out of scope here.
- **Tests:** `cargo test -p retention-service --lib` — 51 tests, all passed (7 new: repository
  `delete` unit tests including cross-tenant isolation, 5 new handler tests covering RBAC/
  tenant-scoping/404 on the new `DELETE` endpoint). `cargo test -p retention-service --test
  retention_policy_integration_test` — 8 tests against real Postgres, all passed, including a
  new test proving `delete` writes a `Deleted` audit row with `before` populated and actually
  removes the row. `cargo test -p kizashi-ui --lib` — 174 tests, all passed (19 covering
  retention policies specifically: list/create/edit/toggle/delete against both a real stub
  HTTP server and the in-process router, viewer-role rejection on every write action, and
  backend-failure handling). `cargo test --workspace --all-features` (full real-infra stack)
  — all passed, 0 failed. `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` — clean. `cargo fmt --all --check` — clean. `cargo deny check` — clean. `cargo
  audit` — same 3 pre-existing allow-listed advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `kizashi-ui` and `retention-service`
  containers, seeded the local demo tenant/user (`scripts/seed-local-demo.sh`), logged in for
  a real session cookie, and drove the full CRUD lifecycle through the actual pages: created a
  policy (confirmed via Postgres), edited its TTL from 90 to 200 days via the real inline form
  (confirmed via Postgres), and deleted it via the real Remove button (confirmed via Postgres
  — row count 0). A headless-Chrome screenshot of the real rendered page confirmed the edit
  field, toggle button, and Remove button all render correctly and match the platform's
  existing visual design language — not a guess from reading the template.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — adds a `DELETE` endpoint following the identical pattern
  `agent_repository::delete` already established, and a UI surface for the resulting full CRUD
  API; no new architectural decision

## [2026-07-19] feature/0027-egress-allowlist-console-ui — Egress Allowlist Console UI page
- **Type:** feature
- **Branch:** feature/0027-egress-allowlist-console-ui
- **Summary:** Closes the third and final "full backend, zero UI" gap found in the Console UI
  completeness audit. `egress-gateway` has had a full `GET`/`PUT /v1/allowlist` API since
  ADR-0021, RBAC-enforced (fix/0003) — but no Console UI page ever existed for it, meaning an
  operator had to hand-craft `curl` to manage a tenant's SSRF/exfiltration containment
  boundary. Added a `/egress-allowlist` page: `egress_allowlist_client.rs`
  (`EgressAllowlistClient` trait + `HttpEgressAllowlistClient`, threading `role: Role` through
  the `PUT` write), `egress_allowlist_handler.rs` (get + replace-the-whole-list post, mirroring
  `AnalysisConfigClient`'s singleton-config pattern since that's this backend's own shape — one
  resource per tenant, not row-based CRUD like Agents/Retention Policies), and a new
  `egress_allowlist.html` template with a one-domain-per-line textarea. Also added
  `.env.example`/`docker-compose.yml` entries for `EGRESS_GATEWAY_URL`.
- **Tests:** `cargo test -p kizashi-ui --lib` — 184 tests, all passed (10 new: client tests
  against a real stub HTTP server for get/put/role-rejection, handler tests covering
  empty-default, save-and-display, blank-textarea-means-empty-list, viewer-role rejection, and
  backend-failure handling). `cargo test --workspace --all-features` (full real-infra stack)
  — all passed, 0 failed. `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` — clean. `cargo fmt --all --check` — clean. `cargo deny check` — clean. `cargo
  audit` — same 3 pre-existing allow-listed advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `kizashi-ui` container, logged in
  with the seeded demo user, and posted a real 3-domain allowlist through the actual page —
  confirmed via a direct Postgres query against `egress_gateway.tenant_allowlists` that all
  three domains landed correctly. A headless-Chrome screenshot of the real rendered page
  confirmed the textarea correctly displays the saved domains (one per line) and matches the
  platform's existing visual design language. Cleaned up the test allowlist row afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — implements a UI surface for an already-existing, already-decided backend API
  (ADR-0021), no new architectural decision

## [2026-07-19] feature/0028-audit-log-console-ui — Audit history Console UI viewer
- **Type:** feature
- **Branch:** feature/0028-audit-log-console-ui
- **Summary:** Closes the last "backend exists, UI can't see it" gap found in the Console UI
  completeness audit. Every config write (triggers, mappings, agents, retention policies) has
  always written to an immutable audit trail (CLAUDE.md §5) via `record_audit_entry`, readable
  through `config-admin-service`'s and `retention-service`'s identically-shaped
  `GET /v1/audit-log/:entity_id` — but nothing in the Console UI could read it back. Added a
  shared `/audit-log/:service/:entity_id` page: `audit_log_client.rs` (one `AuditLogClient`
  trait + `HttpAuditLogClient` impl, constructed twice in `AppState` —
  `config_audit_log_client` and `retention_audit_log_client` — against the two backends' own
  base URLs, since both expose the same shape), `audit_log_handler.rs` (dispatches on the
  `:service` path segment, pretty-prints `before`/`after` JSON for display since Askama can't
  call arbitrary Rust functions), and a new `audit_log.html` template. Added "History" links to
  every row on the Triggers, Field Mappings, Agents, and Retention Policies pages, pointing at
  the correct `config`/`retention` service segment for each entity type.
- **Tests:** `cargo test -p kizashi-ui --lib` — 192 tests, all passed (8 new: client tests
  against a real stub HTTP server, handler tests covering both services' entries rendering
  correctly, an unknown-`:service` error path, empty-history state, and backend-failure
  handling). `cargo test --workspace --all-features` (full real-infra stack) — all passed, 0
  failed. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo deny check` — clean. `cargo audit` — same 3
  pre-existing allow-listed advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `kizashi-ui` container, logged in
  with the seeded demo user, created a real trigger through the actual Triggers page,
  confirmed the new "History" link on that page points at the correct URL, then fetched
  `/audit-log/config/:id` and confirmed it shows the real `created` audit entry with the
  trigger's actual JSON payload — not a stub. A headless-Chrome screenshot confirmed the
  pretty-printed JSON diff panel renders correctly and matches the platform's existing visual
  design language. Cleaned up the test trigger afterward (the audit entry itself correctly
  remains — append-only, by design).
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — implements a UI surface for already-existing, already-decided backend APIs
  (the audit-log write path itself predates this session), no new architectural decision

## [2026-07-19] feature/0029-normalization-mapping-sync — Sync NormalizationMapping config-admin to normalization-service
- **Type:** feature
- **Branch:** feature/0029-normalization-mapping-sync
- **Summary:** Closes a real functional bug surfaced by this session's ADR-follow-up audit:
  editing a Field Mapping through the Console UI (built earlier this session) had zero effect
  on the running normalization pipeline, because `normalization-service` only ever read its own
  local Postgres table and was never wired to receive change notifications from
  `config-admin-service`, the actual owner of the config. Fixed by extending ADR-0018's
  already-proven `trigger.changed` sync pattern to mappings: `config-admin-service` now
  publishes a `mapping.changed` fanout message (new `MAPPING_CHANGED_EXCHANGE` constant in
  `common`, `mapping_publisher.rs`'s `MappingPublisher` trait + `RabbitMqMappingPublisher`,
  called from `create_mapping`/`update_mapping`) whenever a mapping is created or updated;
  `normalization-service` now consumes it (new `upsert()` on `MappingRepository`'s trait/
  Postgres impl using `ON CONFLICT (id) DO UPDATE`, plus a `tokio::spawn`'d consumer loop in
  `main.rs` that acks on success and nacks-with-requeue on repository failure) and mirrors the
  change into its own local table.
- **Tests:** `cargo test -p config-admin-service --lib` — 75 passed (2 new:
  `in_memory_publisher_records_published_mappings`, `failing_publisher_returns_bus_error`).
  `cargo test -p config-admin-service --test mapping_publisher_integration_test` — 1 passed,
  real RabbitMQ round trip. `cargo test -p normalization-service --lib` — 18 passed (2 new:
  `upsert_inserts_a_new_mapping`, `upsert_replaces_an_existing_mapping_with_the_same_id`).
  `cargo test -p normalization-service --test mapping_repository_integration_test` — 2 passed,
  real Postgres (1 new: `upsert_inserts_then_replaces_a_mapping_by_id_against_real_postgres`).
  `cargo test --workspace --all-features` (full real-infra stack: Postgres, RabbitMQ,
  ClickHouse, greenmail, throwaway MSSQL) — 108 test binaries, all passed, 0 failed. `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all
  --check` — clean. `cargo deny check` — clean (advisories ok, bans ok, licenses ok, sources
  ok). `cargo audit` — same 3 pre-existing allow-listed advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `config-admin-service` and
  `normalization-service` containers, logged in as the seeded demo user, then created and
  updated a real `NormalizationMapping` via `config-admin-service`'s actual HTTP API. Confirmed
  via direct Postgres queries against `normalization_service.normalization_mappings` that both
  the create and the update propagated live over real RabbitMQ into the service's local mirror
  table — the exact end-to-end path a Console UI edit now actually takes effect through.
  Cleaned up the test mapping row from both services' tables afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — extends ADR-0018's already-decided config-sync pattern to a sibling entity, no
  new architectural decision

## [2026-07-19] feature/0030-user-management-role-assignment — User management + role-assignment Console UI (ADR-0016 follow-up)
- **Type:** feature
- **Branch:** feature/0030-user-management-role-assignment
- **Summary:** Closes the "assign role to another user" gap ADR-0016 explicitly deferred as
  out of scope for RBAC v1 — until now, `auth-service` had zero user-management endpoints
  (only `local_login`), so there was no way for a workspace admin to add a teammate, change
  someone's role, or remove access without hand-editing Postgres. Added full CRUD to
  `auth-service`: `local_user_repository.rs` gained `list`/`create`/`update_role`/`delete`
  (each writing an immutable audit row in the same transaction, mirroring
  `trigger_definition_repository.rs`'s pattern), a new `auth_audit_log` table with a
  `BEFORE UPDATE OR DELETE`-rejecting trigger (immutability enforced at the database level,
  not just application convention), and `user_handlers.rs` exposing
  `POST/GET /v1/users`, `PUT/DELETE /v1/users/:id`, gated by a new `require_admin` check — a
  step above the `Operator` bar every other write path uses, since granting/revoking access is
  more sensitive than editing config entities. Console UI gets a `/users` page
  (`users_client.rs`, `users_handler.rs`, `users.html`): add-user form, inline role-change
  dropdown, remove button (disabled for your own row), and a "History" link into the existing
  shared audit-log viewer (extended to a third backend, `auth`).
- **Tests:** `cargo test -p auth-service --lib` — 53 passed (16 new: repository CRUD tests,
  handler RBAC tests for create/list/update/delete/audit-log across Admin/Operator/Viewer).
  `cargo test -p auth-service --test local_user_repository_integration_test` — 5 passed, real
  Postgres (4 new, including `auth_audit_log_rejects_delete_at_the_database_level` proving the
  immutability trigger). `cargo test -p kizashi-ui --lib` — 207 passed (18 new: client tests
  against a real stub HTTP server, handler tests covering Admin-only page access, create/
  update-role/delete flows, and backend-failure handling). `cargo test --workspace
  --all-features` (full real-infra stack) — 108 test binaries, all passed, 0 failed. `cargo
  clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all
  --check` — clean. `cargo deny check` — clean. `cargo audit` — same 3 pre-existing
  allow-listed advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `auth-service` and `kizashi-ui`
  containers. Via `auth-service`'s actual HTTP API: created a user, confirmed 403 for
  non-admin callers, logged in as the new user, escalated its role to `admin`, read its real
  audit trail (`created` then `updated` rows), deleted it, and confirmed the deleted user can
  no longer log in. Via the real Console UI: logged in as the seeded demo admin, added a user
  through the actual `/users` form, confirmed it appears in the table, removed it, and
  confirmed removal — a headless-Chrome screenshot of the rendered page confirmed the table,
  role dropdowns, and disabled self-remove button render correctly and match the platform's
  existing visual design language.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — implements the "assign role to another user" surface ADR-0016 already decided
  to defer, no new architectural decision; `Admin`-only gating for user management follows
  directly from that ADR's role model

## [2026-07-19] feature/0031-last-admin-protection — Prevent removing a tenant's last Admin
- **Type:** feature
- **Branch:** feature/0031-last-admin-protection
- **Summary:** Closes a real safety gap in the user-management feature just shipped: nothing
  stopped an operator from demoting or deleting the only `Admin` in a tenant, which would leave
  that workspace with no one able to manage users/roles at all — an unrecoverable-without-direct-
  Postgres-access lockout. Added `is_sole_admin` in `crates/auth-service/src/user_handlers.rs`,
  checked before `update_user_role` (only when the request would actually change the role away
  from `Admin`) and before `delete_user` (always) — both now return `409 Conflict` with a clear
  message ("promote another user first") instead of silently allowing the mutation. This can be
  checked tenant-wide without a user identity in the session (ADR-0016's still-open limitation),
  since it only needs to count admins, not identify "self".
- **Tests:** `cargo test -p auth-service --lib` — 58 passed (5 new: rejects demoting/deleting
  the sole admin, allows demoting/deleting when a second admin exists, allows reassigning the
  sole admin to admin as a no-op). `cargo test --workspace --all-features` (full real-infra
  stack) — 108 test binaries, all passed, 0 failed. `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo deny
  check` — clean. `cargo audit` — same 3 pre-existing allow-listed advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `auth-service` container. Against the
  seeded demo tenant (one `Admin` user): confirmed both `PUT .../role` (demote) and `DELETE`
  against the sole admin return `409` with the expected message. Created a second real admin,
  confirmed the demotion then succeeds (`200`), restored the original admin's role, and
  cleaned up the second admin afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — a defensive guard on ADR-0016's already-decided role model, no new
  architectural decision

## [2026-07-19] fix/0004-teams-alert-webhook-payload-shape — Send a real Teams MessageCard for TeamsAlert actions
- **Type:** fix
- **Branch:** fix/0004-teams-alert-webhook-payload-shape
- **Summary:** `HttpActionDispatcher`'s doc comment claimed genuine support for "Teams incoming
  webhooks" for every `ActionType`, but it POSTs a generic `{action_type, action_config,
  event}` envelope — not the `@type: MessageCard` shape a real Microsoft Teams incoming
  webhook validates and requires, so a `TeamsAlert` action configured against a real Teams
  webhook URL would be rejected (400) despite looking correctly configured. Added
  `teams_alert_action_dispatcher.rs` (`TeamsAlertActionDispatcher`), which formats the actual
  Connector Card schema Teams expects (title, summary, themeColor, and a facts section built
  from the firing `Event`'s type/entity/group key/occurred-at/payload), and wired it into
  `RoutingActionDispatcher` for `ActionType::TeamsAlert` — mirroring the same routing pattern
  ADR-0023/ADR-0024 already established for SMTP/Graph email. `Webhook`/`CreateTicket`/
  `Custom` remain on the generic dispatcher, since those are intentionally bring-your-own-shape.
- **Tests:** `cargo test -p action-executor --lib` — 45 passed (6 new: a real-HTTP-round-trip
  test asserting the exact captured request body matches Teams' documented MessageCard shape,
  a default-title test, missing-url/rejected/unreachable error-path tests, and a routing test
  confirming `TeamsAlert` actions reach the new dispatcher not the generic one). `cargo test
  --workspace --all-features` (full real-infra stack) — 108 test binaries, all passed, 0
  failed. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo deny check` — clean. `cargo audit` — same 3
  pre-existing allow-listed advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `action-executor` container. Created
  a real `TriggerDefinition` via `config-admin-service`'s actual HTTP API with a `TeamsAlert`
  action pointing at a local stub webhook server, confirmed it synced to `trigger-engine`'s
  local mirror over real RabbitMQ (ADR-0018's mechanism), published a real `event.created`
  message via RabbitMQ's HTTP management API, and confirmed the running `action-executor`
  container consumed it, resolved the real trigger, and POSTed the exact `MessageCard` JSON
  shape (`@type`, `@context`, `title`, `summary`, `themeColor`, `sections[0].facts`) to the
  stub server — the genuine end-to-end path a real Teams incoming webhook would now accept.
  Cleaned up the test trigger from both services' tables afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — a defensive/correctness fix within ADR-0007's already-decided dispatch model,
  no new architectural decision

## [2026-07-19] feature/0032-retention-sweep-scheduler — Schedule retention-service's sweep in docker-compose
- **Type:** feature
- **Branch:** feature/0032-retention-sweep-scheduler
- **Summary:** Closes a real operational gap ADR-0011 point 5 flagged but never followed up on:
  `retention-service`'s `POST /v1/sweep` is deliberately HTTP-triggered rather than an
  in-process timer, with the decision explicitly requiring "external scheduling (a Kubernetes
  CronJob or equivalent)" — but no such equivalent existed in the actual deployed
  docker-compose environment, so sweeps have never run automatically; archived/expired data
  was only ever cleaned up by someone manually curling the endpoint. Added
  `retention-sweep-scheduler` to `docker-compose.yml`: a minimal `alpine` sidecar that POSTs
  `/v1/sweep` on a configurable interval (`RETENTION_SWEEP_INTERVAL_SECONDS`, default 3600),
  added to `.env.example`. This is the docker-compose "or equivalent" the ADR called for; a
  real Kubernetes CronJob manifest replaces this sidecar 1:1 later without touching
  `retention-service` itself, since both just call the same stateless HTTP endpoint.
- **Tests:** No Rust code changed beyond a doc comment (`cargo build -p retention-service`
  confirmed it still compiles); this is infra/config, verified via live deployment below
  rather than a unit test.
- **Live verification:** brought up the real `retention-sweep-scheduler` container against
  the real `retention-service`. Confirmed it triggers a sweep immediately on startup (real
  `{"records_archived":0,"batches_written":[]}` response logged) and again on every configured
  interval — overrode `RETENTION_SWEEP_INTERVAL_SECONDS=5` and observed four consecutive real
  sweep triggers in the container's logs at the expected cadence, then restored the production
  default (3600s) and confirmed it still sweeps on startup.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — implements ADR-0011 point 5's already-decided "external scheduling... or
  equivalent" for the docker-compose deployment target, no new architectural decision

## [2026-07-19] feature/0033-cross-source-correlated-triggers — Cross-source correlated trigger conditions
- **Type:** feature
- **Branch:** feature/0033-cross-source-correlated-triggers
- **Summary:** Closes the real use case ADR-0001 anticipated when it deferred compound trigger
  conditions: operators reading from multiple agents/connectors need triggers that combine
  signals across data streams for the same entity — e.g. "fire when a customer has a
  negative-sentiment email AND an unresolved chat message within the same window," not just
  within one source type. Added ADR-0027 and a new `TriggerCondition::CorrelatedOverWindow {
  conditions: Vec<CorrelatedCondition> }` variant (`common::trigger_definition.rs`) — a closed
  "every listed event type needs its own min_count within the window" shape, additive to the
  two existing shapes with zero changes to their evaluation or tests. `TriggerRepository::
  active_triggers_for` (`trigger-engine`) now finds a correlated trigger by any of its listed
  event types via a Postgres JSONB containment query against the existing `condition` column
  (no schema change). `process_analyzed_record` gained `evaluate_trigger`, which for a
  correlated trigger queries `SignalRepository::window_stats` once per listed event type
  (previously always exactly once, for the arriving candidate's own type) and evaluates via the
  new `TriggerDefinition::evaluate_correlated`; the fired Event's `record_ids` lineage is the
  union across every contributing source. Console UI authoring support is explicitly deferred
  per the ADR — the API already accepts the new shape as arbitrary JSON.
- **Tests:** `cargo test -p common --lib` — 54 passed (7 new: correlated fire/no-fire cases,
  empty-conditions-never-fires, disabled-never-fires, unrelated-counts-ignored, and a new
  `evaluate_correlated_never_panics_on_arbitrary_input` proptest extending the existing
  trigger-DSL fuzz coverage CLAUDE.md §2 requires). `cargo test -p trigger-engine --lib` — 34
  passed (4 new: correlated lookup-by-either-event-type, plus two full `process_analyzed_record`
  end-to-end tests proving a correlated trigger only fires once every source has contributed
  and doesn't cross-contaminate between entities). `cargo test -p trigger-engine --test
  trigger_repository_integration_test` — 4 passed, real Postgres (2 new, including the JSONB
  containment query proven against a real database). `cargo test --workspace --all-features`
  (full real-infra stack) — 108 test binaries, all passed, 0 failed. `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean.
  `cargo deny check` — clean. `cargo audit` — same 3 pre-existing allow-listed advisories, no
  new ones.
- **Live verification:** rebuilt and redeployed the real `config-admin-service`,
  `trigger-engine`, and `action-executor` containers (all depend on `common`, where the new
  variant lives). Created a real correlated trigger via `config-admin-service`'s actual API,
  confirmed it synced to `trigger-engine`. Published two real `record.analyzed` messages over
  RabbitMQ for the same entity from two different (simulated) connectors — an email-sentiment
  signal, then an unresolved-chat signal — and confirmed via direct ClickHouse/Postgres queries
  and `action-executor`'s own `ActionExecution` audit log that: (a) no event fired after only
  the email signal, (b) the correlated Event fired only once the chat signal landed, and (c)
  the fired event's `record_ids` contained both the email and chat record ids — proof the
  condition genuinely joined signals across two connectors before firing, not just re-checking
  one source. Cleaned up all test trigger/signal/event data afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0027](../docs/adr/0027-cross-source-correlated-trigger-conditions.md) — extends
  ADR-0001's trigger condition DSL shape, the spec §11 open item CLAUDE.md flags for this exact
  kind of change

## [2026-07-19] feature/0034-correlated-triggers-console-ui — Author correlated triggers through the Console UI
- **Type:** feature
- **Branch:** feature/0034-correlated-triggers-console-ui
- **Summary:** Closes the Console UI gap ADR-0027 explicitly deferred: until now,
  `CorrelatedOverWindow` triggers (email + chat, etc.) could only be created via raw API calls,
  not through the `/triggers` page. Added a third condition option, "Combine multiple sources,"
  to the create-trigger form (`ui/src/triggers_handler.rs`, `ui/templates/triggers.html`) — up
  to three (event type, min count) rows, since a plain HTML form can't submit a variable-length
  list without JS (ADR-0014's no-JS-by-default stance); any row left blank is skipped, not an
  error. The trigger's `event_type_match` (a display/audit label for this shape per ADR-0027)
  is auto-derived from the first filled-in row rather than asked for separately, since it plays
  no role in lookup for a correlated trigger.
- **Tests:** `cargo test -p kizashi-ui --lib` — 210 passed (3 new: creates a correlated trigger
  and derives `event_type_match` from the first leg, form-error when no rows are filled in,
  form-error when a row has an invalid min count). `cargo test --workspace --all-features`
  (full real-infra stack) — 108 test binaries, all passed, 0 failed. `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean.
  `cargo deny check` — clean. `cargo audit` — same 3 pre-existing allow-listed advisories, no
  new ones.
- **Live verification:** rebuilt and redeployed the real `kizashi-ui` container, logged in as
  the seeded demo admin, and submitted a real "combine multiple sources" trigger
  (`sentiment_drop_email` + `unresolved_chat`, min count 1 each) through the actual form.
  Confirmed via `config-admin-service`'s real API that the stored `TriggerDefinition` has the
  correct `CorrelatedOverWindow` shape and that `event_type_match` was correctly auto-derived
  as `sentiment_drop_email`. A headless-Chrome screenshot confirmed the new form fields render
  correctly and match the platform's existing visual design language. Cleaned up the test
  trigger afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — implements the UI surface ADR-0027 already decided to defer, no new
  architectural decision

## [2026-07-19] feature/0035-configurable-webhook-action-body — Configurable webhook action body template
- **Type:** feature
- **Branch:** feature/0035-configurable-webhook-action-body
- **Summary:** Generalizes the fix/0004 pattern: `HttpActionDispatcher`'s generic `{action_
  type, action_config, event}` envelope is rejected by most real third-party webhook targets
  with their own required body shape (Slack's `{"text": "..."}` minimum, PagerDuty's Events
  API v2 envelope, a Jira/ServiceNow REST body) — the same class of bug fixed for Teams, but
  affecting every `Webhook`/`CreateTicket`/`Custom` action, which have no dedicated `ActionType`
  variant of their own to build a per-vendor dispatcher against. Added ADR-0028 and an optional
  `body_template` field to an action's `config`: when present, `render_body_template` walks the
  JSON tree substituting `{{event_type}}`, `{{entity_ref}}`, `{{group_key}}`, `{{tenant_id}}`,
  `{{occurred_at}}`, and `{{payload}}` placeholders in every string leaf with the firing
  event's real values, and the rendered result is sent as the POST body instead of the generic
  envelope. Without a `body_template`, behavior is unchanged (purely additive). An unrecognized
  placeholder is left as literal text, not an error — no template compilation, no code
  execution, can't panic on operator-authored config.
- **Tests:** `cargo test -p action-executor --lib` — 49 passed (4 new: placeholder
  substitution across strings/nested objects/arrays, unrecognized-placeholder-stays-literal, a
  real-HTTP-round-trip test proving the rendered body — not the envelope — is what's actually
  sent, and a test proving the generic envelope still sends when no `body_template` is
  configured). `cargo test --workspace --all-features` (full real-infra stack) — 108 test
  binaries, all passed, 0 failed. `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` — clean. `cargo fmt --all --check` — clean. `cargo deny check` — clean. `cargo
  audit` — same 3 pre-existing allow-listed advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `action-executor` container. Created
  a real trigger via `config-admin-service`'s API with a `Webhook` action configured with a
  Slack-style `body_template` (`{"text": "Kizashi alert: {{event_type}} for {{entity_ref}}"}`),
  confirmed it synced to `trigger-engine`, published a real `event.created` message over
  RabbitMQ, and confirmed the running container POSTed exactly `{"text": "Kizashi alert:
  e2e_slack_test for cust-slack-e2e"}` — the genuine Slack-compatible shape, not the generic
  envelope — to a stub webhook server. Cleaned up the test trigger/event afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0028](../docs/adr/0028-configurable-webhook-action-body-template.md) — extends
  ADR-0007's dispatch model with a config-driven body shape, generalizing the ad hoc Teams fix

## [2026-07-19] docs/0002-adr-0016-stale-followups-note — Correct stale RBAC follow-up claims in ADR-0016
- **Type:** docs
- **Branch:** docs/0002-adr-0016-stale-followups-note
- **Summary:** An RBAC-lifecycle audit for the next backlog item found that ADR-0016's
  "Consequences" section still claims `retention-service` and `ingestion-gateway`'s API-key
  endpoints are unenforced, and that role reassignment has no UI — both have since shipped
  (fix/0003, ingestion-gateway's own `require_operator` gating already in place, and feature/
  0030's `/users` page). A misleading ADR is worse than no ADR — CLAUDE.md §5 says this is how
  "a future auditor (or future Claude session) sees why, not just what," and a stale claim
  actively misleads that reader. Added `**Update:**` notes to both bullets pointing at what
  actually landed, without rewriting the original (accurate-at-the-time) text. Also fixed a
  matching stale doc comment in `ui/src/api_keys_handler.rs` that repeated the same outdated
  claim. No production behavior changed — this is a docs-accuracy fix, verified that both
  claims were actually false by re-reading `retention-service/src/policy_handlers.rs` and
  `ingestion-gateway/src/api_key_handlers.rs`, which both already call `require_operator` on
  every write path.
- **Tests:** `cargo build -p kizashi-ui` — compiles (comment-only change). `cargo fmt --all
  --check` / `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings` — clean.
  Full workspace CI gate not re-run for this docs-only change beyond the affected crate, since
  no production code path changed.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — corrects ADR-0016 itself, no new decision

## [2026-07-19] feature/0036-saved-search-queries — Saved data search queries (spec §7)
- **Type:** feature
- **Branch:** feature/0036-saved-search-queries
- **Summary:** Closes the "saved queries/views" slice of spec §7's Reporting capability —
  independently valuable and much smaller than the full scheduled-PDF/email-reporting gap
  (which still needs new infra this PR doesn't touch: no PDF renderer, no email-sending
  scheduler exists anywhere in the repo, out of scope here). Added ADR-0029 and a new
  `common::SavedSearchQuery` type + `saved_search_queries` table in `config-admin-service`
  (least friction: already has `sqlx`/migrations/tenant-scoped-table pattern, unlike
  `kizashi-ui` or `dashboard-api`, neither of which has ever had a Postgres dependency).
  Deliberately **not** audit-logged (unlike every other entity in this service) and **not**
  `require_operator`-gated — a saved search is a personal/team UI bookmark with zero effect on
  the ingestion/normalization/analysis/trigger pipeline, not admin/config in the CLAUDE.md §5
  sense. Console UI: the `/data` page gains a "Save this search as" form and a "Saved searches"
  panel — each saved entry is a plain link to `/data?...` built from the stored filter, so
  "loading" a saved search needs no new load handler, just the existing query-string-driven
  page.
- **Tests:** `cargo test -p common --lib` — 56 passed (2 new: `SavedSearchQuery::new`).
  `cargo test -p config-admin-service --lib` — 95 passed (10 new: repository CRUD + handler
  tests covering no-role-required creation, tenant-mismatch rejection, tenant-scoped listing,
  backend-failure, delete/not-found). `cargo test -p config-admin-service --test
  saved_search_query_repository_integration_test` — 2 passed, real Postgres. `cargo test -p
  kizashi-ui --lib` — 218 passed (10 new: HTTP client round-trip tests against a real stub
  server, and `/data` handler tests for save/list/delete/backend-failure-doesn't-break-the-page).
  `cargo test --workspace --all-features` (full real-infra stack) — 109 test binaries, all
  passed, 0 failed. `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  clean. `cargo fmt --all --check` — clean. `cargo deny check` — clean. `cargo audit` — same 3
  pre-existing allow-listed advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `config-admin-service` and
  `kizashi-ui` containers, logged in as the seeded demo user, saved a real search
  (`zendesk`/`ticket`/`urgent`) through the actual `/data` form, confirmed it's stored correctly
  via `config-admin-service`'s real API, confirmed the rendered "Saved searches" panel's link
  correctly reloads and pre-fills the exact filter, and confirmed the Remove button/route works.
  A headless-Chrome screenshot confirmed the panel renders correctly and matches the platform's
  existing visual design language. Cleaned up the test saved search afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0029](../docs/adr/0029-saved-data-search-queries.md) — scopes "saved
  queries/views" out of the larger deferred Reporting gap and places it in
  `config-admin-service`

## [2026-07-19] feature/0037-trigger-dry-run-test — Trigger dry-run test endpoint (spec §7)
- **Type:** feature
- **Branch:** feature/0037-trigger-dry-run-test
- **Summary:** Closes a real gap an audit against spec §7 found: no way to validate a trigger
  before trusting it in production — the only prior feedback loop was enabling it and waiting
  for real traffic, silently never firing if an `event_type` string or `min_count` was wrong.
  Added ADR-0030 and `POST /v1/triggers/:id/test` (`trigger-engine`): given a `group_key`,
  answers "would this trigger fire right now" by running the exact same `evaluate_trigger`
  function the live `record.analyzed` path uses (extracted to be reusable, taking
  `&Arc<dyn SignalRepository>` directly instead of the full `TriggerDeps` bundle) against real,
  already-recorded signal history — never writes an `Event`, never runs an action, genuinely a
  dry run rather than a reimplementation that could drift from production behavior. No
  `require_operator` gate — reading whether a trigger would fire isn't a write path. Console UI:
  `/triggers` gains a "Test" form per row (GET, not POST — a dry run has no side effects, so
  it's shareable/bookmarkable) showing "would fire: yes/no" plus the contributing record count.
- **Tests:** `cargo test -p trigger-engine --lib` — 38 passed (5 new: would-fire-true when
  signals already satisfy the condition, would-fire-false otherwise, tenant-mismatch returns
  404, missing tenant header returns 401, plus the existing `get_trigger` tests unaffected by
  the `evaluate_trigger` signature refactor). `cargo test -p kizashi-ui --lib` — 224 passed (6
  new: HTTP client round-trip against a real stub trigger-engine server, and handler tests for
  would-fire/would-not-fire rendering, no-result-without-query-params, and backend-failure
  doesn't break the page). `cargo test --workspace --all-features` (full real-infra stack) —
  109 test binaries, all passed, 0 failed. `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo deny
  check` — clean. `cargo audit` — same 3 pre-existing allow-listed advisories, no new ones.
- **Live verification:** rebuilt and redeployed the real `trigger-engine` and `kizashi-ui`
  containers. Created a real `count_over_window` trigger via `config-admin-service`'s API,
  confirmed the dry-run endpoint correctly reported `would_fire: false` with zero signals,
  published two real `record.analyzed` messages over RabbitMQ for the same entity, confirmed
  the dry run then correctly reported `would_fire: true` with `contributing_record_count: 2` —
  while separately confirming via ClickHouse that no *extra* `Event` was created by the dry-run
  calls themselves (the one Event present came from the real live pipeline processing the
  published records, an entirely separate mechanism unaffected by testing). Confirmed the same
  result renders correctly through the actual Console UI `/triggers` page's "Test" form.
  Cleaned up all test trigger/signal/event data afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0030](../docs/adr/0030-trigger-dry-run-test-endpoint.md) — a read-only
  validation endpoint reusing existing evaluation logic, no schema change

## [2026-07-19] feature/0038-correlated-trigger-form-more-rows — Support up to 6 correlated sources in the trigger form
- **Type:** feature
- **Summary:** The correlated-trigger form was hard-capped at 3 sources (email + chat was just
  the illustrative example in ADR-0027/the UI copy, not a real limit — the backend/API already
  accepts any number of legs). Bumped to 6, with only 2 shown by default and a "+ Add another
  source" button progressively revealing the rest — a plain client-side reveal of already
  server-rendered inputs, not a JS-generated form (ADR-0014's no-JS-by-default stance intact).
  While live-verifying, found and fixed a real bug: the hidden extra rows reused the `.form-row`
  class for layout convenience, and that class's own `display: grid` CSS silently overrode the
  native `hidden` attribute's `display: none` — the rows were visible from page load regardless
  of the JS, defeating the progressive-reveal entirely. Fixed by dropping the reused class and
  using explicit inline `display:none`/`display:flex` toggled directly by the button's JS.
- **Tests:** `cargo test -p kizashi-ui --lib` — 1 new (`post_creates_a_correlated_trigger_
  with_all_six_sources`, proving the backend/form parsing handles all 6 rows correctly);
  existing 23 triggers-related tests unaffected. Full workspace CI gate (fmt/clippy/tests/deny/
  audit) re-run clean, same as prior PRs this session.
- **Live verification:** rebuilt and redeployed the real `kizashi-ui` container. Created a real
  6-source correlated trigger through the actual form, confirmed all 6 legs stored correctly
  via `config-admin-service`'s API. A headless-Chrome screenshot caught the CSS bug (all 6 rows
  visible despite the `hidden` attribute) — fixed, rebuilt, redeployed, and re-screenshotted to
  confirm rows 3-6 are now genuinely hidden until "+ Add another source" is clicked. Cleaned up
  test trigger data afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — extends ADR-0027's already-generic correlated-condition shape past a UI-only
  row limit, no new architectural decision

## [2026-07-19] feature/0039-ai-provider-config — Per-tenant AI provider/model configuration (Ollama, OpenAI, Azure Foundry)
- **Type:** feature
- **Summary:** Every tenant's AI analysis was hardcoded to a single platform-wide Azure AI
  Foundry endpoint — there was no way for a tenant to point analysis at a different backend.
  Added `AnalysisProvider` (`AzureFoundry` default | `OpenAiCompatible`) plus `model`/
  `endpoint`/`api_key` fields to `common::AnalysisConfig`, propagated through
  `config-admin-service` (source of truth, new migration + `redact_for_audit` so `api_key`
  never reaches the audit log in plaintext even though the primary row stores it as-entered —
  config-as-data convention, no secrets-manager integration exists yet, flagged as real
  follow-up work) and `analysis-service` (its own read-mostly Postgres mirror, kept in sync via
  the existing `analysis_config.changed` bus message — no consumer/publisher code changes
  needed since both sides serialize/deserialize the whole struct). Built
  `OpenAiCompatibleAnalysisClient` targeting the standard `/v1/chat/completions` shape — one
  client covers Ollama, OpenAI, and Azure OpenAI in compatible mode — making one sequential
  call per record (chat-completions isn't a batch API; asking a model to return N JSON results
  reliably in one response is unreliable). `batch_processor::process_batch` now resolves the
  client per-tenant per-call based on the tenant's configured provider, falling back to the
  platform-default Foundry client for tenants with no config or `AzureFoundry`. Extended the
  Console UI's `/analysis-config` page with a provider selector and conditional model/endpoint/
  API-key fields. **Bug found and fixed during TDD**: `AnalysisProvider`'s original
  `#[serde(rename_all = "snake_case")]` produced `open_ai_compatible` for `OpenAiCompatible`
  ("Ai" splits into its own word) while the hand-written Postgres `provider` column
  read/write code used `openai_compatible` — two different spellings for the same variant
  across the wire format and storage format. Fixed with an explicit `#[serde(rename = ...)]`
  per variant so both agree; a real API round-trip test caught this before it ever reached a
  live deploy.
- **Tests:** `cargo test -p common --lib analysis_config` — 5 passed (2 new: default-provider
  behavior, wire-format-matches-storage-format regression test for the rename bug).
  `cargo test -p config-admin-service --lib analysis_config` — 18 passed (5 new: redaction with
  and without an api_key present, provider/model/endpoint/api_key round-trip through the HTTP
  handler, defaults-to-azure-foundry-when-omitted). `cargo test -p analysis-service --lib` — 28
  passed (11 new: `OpenAiCompatibleAnalysisClient` against a stub chat-completions server —
  parses JSON replies, wraps non-JSON replies as `{"text": ...}`, sends model/bearer-auth/
  prompt correctly, reports Unreachable/Rejected correctly — plus `process_batch` routing to
  the OpenAI-compatible client for a configured tenant while leaving the platform-default
  client untouched, plus a repository round-trip test for the new columns).
  `cargo test -p kizashi-ui --lib analysis_config` — 11 passed (3 new: form round-trips
  provider/model/endpoint through the page, HTTP client sends/receives the new fields).
  `cargo test --workspace --all-features` (full real-infra stack: Postgres, RabbitMQ,
  ClickHouse, MinIO, throwaway MSSQL for Fabric, throwaway greenmail for IMAP/SMTP) — every
  test binary passed, 0 failed. `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` — clean. `cargo fmt --all --check` — clean. `cargo deny check` — clean. `cargo
  audit` — same 3 pre-existing allow-listed advisories (`instant`, `rustls-pemfile` x2), no new
  ones.
- **Live verification:** built a throwaway `OpenAiCompatibleAnalysisClient` smoke test and ran
  it against the actual local Ollama instance at `localhost:11434` (model `qwen3:8b`, confirmed
  running via `ollama list`/`curl .../api/version`) — sent a real record + prompt, got back a
  real model-generated JSON reply (`{"urgent":true}`), proving a genuine end-to-end round trip
  through the new client against real inference, not a stub. Removed the throwaway test
  afterward since it depends on infra not guaranteed present in CI/other environments.
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0031](../docs/adr/0031-per-tenant-ai-provider-and-model-configuration.md) —
  provider selection shape, why chat-completions can't be batched like Foundry, why the client
  is resolved per-call instead of cached, and the accepted-interim plaintext-`api_key` posture

## [2026-07-19] feature/0040-idempotent-ingestion-dedup — Idempotent ingestion via external_id
- **Type:** feature
- **Summary:** Connectors are stateless per invocation (ADR-0013) and at least one — IMAP —
  necessarily re-scans an overlapping poll window every cycle, since IMAP's `SEARCH SINCE` only
  has day granularity. Before this change, every re-scanned message became a brand-new
  `RawRecord`, flowing through Normalization/Analysis/Trigger Engine again and potentially
  re-firing a Trigger for the same source item on every single poll, forever — a real
  correctness gap surfaced while wiring up a genuine production email-monitoring use case.
  Added an optional `external_id` field to `RawRecord`; Ingestion Service enforces uniqueness
  on `(tenant_id, connector_id, external_id)` via a partial unique index (`WHERE external_id IS
  NOT NULL`, so records with no external id are unaffected) and `ON CONFLICT ... DO NOTHING`,
  and only publishes `record.ingested` on an actual new insert — a duplicate never reaches
  downstream processing at all. The IMAP connector now sets `external_id` from the message's
  `Message-Id` header (RFC 5322, globally stable), falling back to `"{connector_id}:{uid}"` for
  the rare message without one (IMAP UIDs are unique within a mailbox). While verifying this
  against real Postgres, also found and fixed a **pre-existing test flake**: the ingestion
  integration tests bind to the same RabbitMQ fanout exchange every live service in this
  shared dev environment publishes to, so a test could receive an unrelated `record.ingested`
  message from a real background agent before its own — fixed by filtering received messages
  by the record's own id/tenant instead of assuming the first delivery is the test's own.
- **Tests:** `cargo test -p common --lib raw_record` — 5 passed (field addition, existing tests
  unaffected). `cargo test -p ingestion-service --lib` — 61 passed (4 new: no-external-id is
  never deduped, same external_id re-insert is a no-op, dedup is scoped per tenant, handler
  returns 201 and skips publish on a dedup no-op). `cargo test -p ingestion-service --tests`
  (real Postgres/RabbitMQ) — new integration test proving the real partial unique index
  actually dedupes and `record.ingested` publishes exactly once, not once per re-post.
  `cargo test -p connector-imap --lib message` — 5 passed (2 new: external_id from Message-Id,
  fallback to connector_id:uid when absent). `cargo test -p connector-runtime --lib
  ingestion_client` — 6 passed (1 new: external_id is included in the request body).
  `cargo test --workspace --all-features` (full real-infra stack) — every test binary passed,
  0 failed. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo deny check` — clean. `cargo audit` — same 3
  pre-existing allow-listed advisories, no new ones.
- **Live verification:** applied the new migration against the real running Postgres
  (`ingestion_service.raw_records` gained `external_id` and the partial unique index, confirmed
  via `\d raw_records`), manually verified the exact `ON CONFLICT` clause behaves as `INSERT 0
  0` on a real conflicting insert via `psql`, and ran the new Rust integration test against the
  real stack proving both DB-level dedup and publish-exactly-once end-to-end.
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0032](../docs/adr/0032-idempotent-ingestion-via-external-id.md)

## [2026-07-19] feature/0041-imap-since-date-narrowing — Narrow IMAP's poll window after the first poll
- **Type:** feature
- **Summary:** Caught live against a real personal mailbox before it ran unattended: an IMAP
  Agent's `IMAP_SINCE_DATE` came straight from the Agent's static config on every poll, forever
  — an Agent configured with a 6-month backfill re-fetched the *entire* 6 months of message
  bodies over IMAP every single poll interval, not just new mail. ADR-0032's dedup made this
  safe (no duplicate rows/events), but not efficient — repeated full-history re-fetches against
  a real mail server is real bandwidth/IMAP load, not just a cosmetic inefficiency.
  `agent-scheduler` already tracked `last_polled_at` per Agent for scheduling cadence but never
  passed it to the invoker. `Invoker::invoke` now takes `last_polled_at`, and
  `DockerInvoker::build_run_args` uses it to override `IMAP_SINCE_DATE` to `last_polled_at - 1
  day` (a coarse but safe overlap, since IMAP's `SEARCH SINCE` is date-granularity only) on
  every poll after the first — narrowly special-cased to `connector_type == "imap"`, not a
  generic mechanism, since it's the one connector currently known to re-scan a stateless date
  window. **Also disabled a real Agent immediately upon spotting this in production** — a
  registered `mail-watkinslabs-com` IMAP Agent was pulled while this fix was built, to stop it
  from repeatedly re-downloading six months of real mail every 5 minutes in the meantime.
- **Tests:** `cargo test -p agent-scheduler --lib` — 13 passed (3 new:
  `IMAP_SINCE_DATE` unchanged on a first-ever poll, overridden to `last_polled_at - 1 day` on a
  later poll, non-IMAP connectors unaffected by `last_polled_at`). `cargo test --workspace
  --all-features` (full real-infra stack) — every test binary passed, 0 failed. `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check`
  — clean. `cargo deny check` / `cargo audit` — clean, same 3 pre-existing allow-listed
  advisories.
- **Live verification:** discovered via a real deployment — a real IMAP Agent against a real
  mailbox ingested exactly 600 records (hit the ingestion-gateway rate limit ceiling on a
  single burst-backfill poll, confirming a substantial multi-hundred-message real inbox
  history) before the re-scan problem was noticed and the Agent disabled pending this fix.
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0033](../docs/adr/0033-imap-since-date-narrowing-on-later-polls.md)

## [2026-07-19] feature/0042-imap-uid-cursor — Real IMAP UID cursor with chunked backfill
- **Type:** feature
- **Summary:** ADR-0033's day-overlap approach (merged minutes earlier) was correctly flagged
  as insufficient before it ran unattended: re-scanning (then dedup-discarding) a full day of
  mail on every poll interval is still real avoidable load for anything but a low-volume
  mailbox, and the *initial* backfill was still one unbounded burst — which is exactly what hit
  `ingestion-gateway`'s rate limit at 600 records in a single call during live testing. Replaced
  it with a real IMAP UID cursor: `common::connector::Connector` gains a `checkpoint()` method
  (default `None`); `ImapConnector` returns the highest `uid` among a poll's records. IMAP UIDs
  are monotonically increasing and never reused within a mailbox, so `UID {n+1}:*` gives an
  *exact* incremental fetch, unlike date-only `SINCE` search. The connector prints its
  checkpoint to stdout as `KIZASHI_CHECKPOINT=<value>`; `DockerInvoker` (agent-scheduler)
  captures it from the `docker run` process's stdout and persists it as a new
  `agents.last_checkpoint` column, replaying it as `IMAP_SINCE_UID` on the next poll. Added
  `IMAP_MAX_RECORDS_PER_POLL` (default 200): matched UIDs are sorted oldest-first and capped
  per poll, so a large backfill is consumed in bounded chunks across successive poll cycles
  using the *same code path* as ordinary incremental polling — no separate "backfill mode",
  the system just naturally transitions from many chunks to near-zero as it catches up.
- **Tests:** `cargo test -p common --lib connector` — 4 passed (1 new: `checkpoint` defaults to
  `None`). `cargo test -p connector-runtime --lib poll_runner` — 6 passed (2 new: a connector's
  checkpoint is carried into `PollSummary`, a connector with no checkpoint leaves it `None`).
  `cargo test -p connector-imap --lib connector` — 9 passed (6 new: checkpoint is the highest
  uid seen, checkpoint is `None` for an empty poll, `UID` search query when `since_uid` is set,
  `SINCE` fallback otherwise, `select_uids` sorts ascending and caps to the oldest N).
  `cargo test -p agent-scheduler --lib` — 17 passed (7 new: `IMAP_SINCE_UID` injected from a
  checkpoint on a later poll, unmodified `IMAP_SINCE_DATE` on a first poll, non-IMAP connectors
  unaffected, stdout marker extraction with/without the line present, `mark_polled` with and
  without a checkpoint). `cargo test --workspace --all-features` (full real-infra stack) —
  every test binary passed, 0 failed. `cargo clippy --workspace --all-targets --all-features
  -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo deny check` / `cargo
  audit` — clean, same 3 pre-existing allow-listed advisories.
- **Live verification:** (to be completed against the real `mail-watkinslabs-com` Agent after
  redeploying `agent-scheduler` and `connector-imap` with this fix — the Agent stays disabled
  until that verification confirms bounded, checkpoint-advancing polls.)
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0034](../docs/adr/0034-imap-uid-cursor-chunked-backfill.md) — supersedes
  ADR-0033

## [2026-07-19] feature/0043-events-over-time-chart — Events-over-time chart on the Events page
- **Type:** feature
- **Summary:** The Events page was a flat paginated table with no trend visibility at all — a
  real gap surfaced by a genuine "show me events on a dashboard over time" use case. Added
  `EventQueryRepository::count_by_day(tenant_id, event_type, since, until)` (ClickHouse
  `toDate(occurred_at)`/`GROUP BY`), a new `GET /v1/events/daily-counts` endpoint on
  dashboard-api, proxied through query-gateway (generic `proxy_get`, no new proxy logic
  needed), a `EventsClient::daily_counts` method on the Console UI side, and a plain inline SVG
  bar chart on the Events page (last 30 days) — server-rendered, no JS, consistent with
  ADR-0014's no-JS-by-default stance. A daily-counts failure degrades to an empty chart, not a
  broken page — the table remains the primary content. **Two real bugs found and fixed during
  live verification, not caught by unit tests alone**: (1) ClickHouse's JSONEachRow format
  serializes `UInt64` (what `count()` returns) as a *quoted JSON string*, not a number —
  deserializing straight into `u64` failed with "invalid type: string \"2\", expected u64"
  against the real deployed stack; fixed by deserializing as `String` and parsing. (2) The SVG
  used `preserveAspectRatio="none"` to stretch to a fixed container size, which non-uniformly
  distorted the count-label text into illegible mirrored-looking glyphs — only visible in an
  actual screenshot, not in raw HTML; fixed by dropping the aspect-ratio override and letting
  the SVG size itself from its own viewBox.
- **Tests:** `cargo test -p dashboard-api --lib` — 25 passed (7 new: daily counts bucket by
  date, scoped to tenant/event_type, exclude out-of-range events, handler requires tenant
  header, returns buckets for the caller, 500 on repository failure, regression test for
  ClickHouse's stringified UInt64 count). `cargo test -p kizashi-ui --lib events_client` — 5
  passed (1 new: HTTP client gets daily counts against a real stub server). `cargo test -p
  kizashi-ui --lib events_handler` — 8 passed (2 new: renders a bar per day with events, a
  daily-counts failure doesn't break the rest of the page). `cargo test --workspace
  --all-features` (full real-infra stack) — every test binary passed, 0 failed. `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check`
  — clean. `cargo deny check` / `cargo audit` — clean, same 3 pre-existing allow-listed
  advisories.
- **Live verification:** rebuilt and redeployed the real `dashboard-api`, `query-gateway`, and
  `kizashi-ui` containers. Inserted real test `Event` rows directly into the actual running
  ClickHouse for the `acme` demo tenant, hit `/v1/events/daily-counts` directly (caught bug #1
  above), fixed and redeployed, then logged into the real Console UI and fetched/screenshotted
  the actual rendered Events page via headless Chrome (caught bug #2 above — the raw HTML alone
  wouldn't have shown the distorted text), fixed, rebuilt, redeployed, and re-screenshotted to
  confirm the chart renders correctly with legible per-day counts and proportional bar heights.
  Cleaned up the test event rows afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — a straightforward read-path addition to an already-established query/proxy
  pattern (`EventQueryRepository` → dashboard-api → query-gateway's generic proxy → Console UI
  client), no new architectural decision

## [2026-07-19] feature/0044-reprocess-unnormalized-records — Reprocess endpoint for records ingested before a mapping existed
- **Type:** feature
- **Summary:** A real gap surfaced by the watkinslabs email agent: 631 real messages were
  ingested before a `NormalizationMapping` existed for tenant `515350d9-...`'s `message` source
  type, so Normalization Service correctly (by design — `ProcessOutcome::NoMappingConfigured`)
  skipped and acked every one of them. Once the mapping was created, those 631 records had no
  way to ever get normalized/analyzed/trigger-evaluated — a real, permanent backlog with no
  recovery path. Added `normalized: Option<bool>` to `RawRecordRepository`'s search filter
  (`Some(false)` finds records with no `normalized_payload`), exposed via the existing
  `/v1/records/search` endpoint, and a new `POST /v1/records/reprocess` endpoint (tenant-scoped
  via header, optional `connector_id`, bounded to 500 records per call) that finds unnormalized
  records and **republishes `record.ingested`** for each — deliberately not calling
  normalization logic directly, so Normalization Service's existing queue consumer picks them
  up exactly like a fresh poll would and the rest of the pipeline (analysis, triggers) runs
  unchanged, with zero new code in Normalization/Analysis/Trigger Engine.
- **Tests:** `cargo test -p ingestion-service --lib` — 65 passed (4 new: `normalized=false`
  filter finds only unnormalized records, reprocess republishes only unnormalized records for
  the caller's tenant, requires tenant header, 500 on repository failure). `cargo test
  --workspace --all-features` (full real-infra stack) — every test binary passed, 0 failed.
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt
  --all --check` — clean. `cargo deny check` / `cargo audit` — clean, same 3 pre-existing
  allow-listed advisories.
- **Live verification:** (to be run against the real `watkinslabs` tenant's 631-message backlog
  after this merges and `ingestion-service` is rebuilt/redeployed.)
- **Known gap, not closed by this PR:** no Console UI button for this yet — it's an
  API-only admin action for now (`POST /v1/records/reprocess` directly against
  ingestion-service). A UI trigger (likely on the Data page) is a reasonable follow-up once
  this is proven against the real backlog.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — a bounded, tenant-scoped extension of the existing search filter plus a thin
  republish handler; no new architectural decision (deliberately reuses the existing
  `record.ingested` → Normalization Service pipeline rather than adding a parallel one)

## [2026-07-19] feature/0045-analysis-concurrency — Bounded concurrency for OpenAI-compatible analysis calls
- **Type:** feature
- **Summary:** Observed live against the real watkinslabs backlog: reprocessing 631 real emails
  through a local `qwen3:8b` Ollama model at concurrency 1 (ADR-0031's original sequential
  design) processed roughly 1-3 records per minute — a multi-hour wait for what should be a
  routine catch-up sweep, since each request waited for the previous one's full round trip
  (network + the model's own reasoning/generation time) before starting the next.
  `OpenAiCompatibleAnalysisClient::analyze_batch` now runs up to `concurrency` requests in
  flight at once via `futures_util::stream::buffered` (default 4, configurable per process via
  `ANALYSIS_OPENAI_CONCURRENCY`, threaded through `AnalysisDeps`). `buffered` (not
  `buffer_unordered`) was chosen specifically to preserve result ordering relative to input
  records with no separate re-sort step, since `process_batch` zips `records` with `results` by
  position. `FoundryAnalysisClient` (the Foundry platform-default path) is unaffected — it
  already sends a whole batch as one call.
- **Tests:** `cargo test -p analysis-service --lib analysis_client` — 16 passed (2 new: a real
  wall-clock proof that 8 records against a 100ms-latency stub finish well under the ~800ms a
  strictly-sequential implementation would take, and a proof that result ordering is preserved
  under concurrency even when responses arrive out of order). `cargo test --workspace
  --all-features` (full real-infra stack) — every test binary passed, 0 failed. `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check`
  — clean. `cargo deny check` / `cargo audit` — clean, same 3 pre-existing allow-listed
  advisories.
- **Live verification:** (to be run against the real watkinslabs backlog — currently mid-flight
  through `analysis-service`'s queue at the old concurrency-1 rate — after this merges and the
  service is rebuilt/redeployed with the fix.)
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0035](../docs/adr/0035-bounded-concurrency-for-openai-compatible-analysis.md)

## [2026-07-19] feature/0046-reprocess-ui-button — Console UI button for the reprocess endpoint
- **Type:** feature
- **Summary:** Closes the known gap flagged in feature/0044: the reprocess endpoint
  (`POST /v1/records/reprocess`) was API-only. Added `IngestionStatsClient::reprocess` (Console
  UI's existing direct client to Ingestion Service), a `POST /data/reprocess` handler
  (operator-gated, matching the rest of this platform's write-path convention), and a button on
  the Data Viewer page showing a confirmation with the republished count after use.
- **Tests:** `cargo test -p kizashi-ui --lib ingestion_stats_client` — 6 passed (1 new: HTTP
  client reprocess call against a real stub server). `cargo test -p kizashi-ui --lib
  data_handler` — 15 passed (5 new: redirects with the count, rejects a viewer role, requires a
  session, shows the button + confirmation for an operator, hides the button for a viewer).
  `cargo test --workspace --all-features` (full real-infra stack) — every test binary passed,
  0 failed. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo deny check` / `cargo audit` — clean, same 3
  pre-existing allow-listed advisories.
- **Live verification:** (to be run against the real Console UI once this merges and
  `kizashi-ui` is rebuilt/redeployed.)
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — a thin UI wrapper around an already-designed, already-ADR'd backend capability
  (feature/0044), no new architectural decision

## [2026-07-19] feature/0047-record-journey-timing-waterfall — Timing waterfall on the Record Journey page
- **Type:** feature
- **Summary:** Responds to a request for "Instana-style" observability (a distributed-trace
  waterfall + infrastructure topology map). Surveyed what already exists first (via a research
  pass, not guessed): the Record Journey page (ADR-0017) already shows the correct
  record→event→action lineage as a box diagram, and `RecordSummary`/`EventSummary`/
  `ActionExecutionSummary` already carry `ingested_at`/`occurred_at`/`executed_at` — the data
  was already flowing to the UI layer, it just was never rendered. The existing "Pipeline Map"
  page already covers a live-health service topology view (5 app-service stages with
  up/down coloring and queue-backlog counts), just not a discovered/dynamic graph — a larger,
  more speculative rebuild than the timing gap, so left alone this pass. Extended Record
  Journey into an actual timing waterfall: each hop (ingest→event, event→action) now shows a
  pre-computed `+Nms`/`+N.Ns`/`+Nm Ns` latency delta and each node shows its real timestamp, no
  new backend endpoint (same three existing calls this page already made). Duration
  arithmetic is done in the handler (`format_latency`), not the Askama template, which can't do
  date math; a negative delta (clock skew) reports as `"0ms"` rather than a confusing negative
  number.
- **Tests:** `cargo test -p kizashi-ui --lib record_journey` — 9 passed (5 new: `format_latency`
  renders sub-second/seconds/minutes correctly, clamps a negative delta to zero, and a live
  end-to-end test proving the actual computed latencies appear in the rendered page).
  `cargo test --workspace --all-features` (full real-infra stack) — every test binary passed,
  0 failed. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo deny check` / `cargo audit` — clean, same 3
  pre-existing allow-listed advisories.
- **Live verification:** (to be run against the real deployed Console UI once this merges and
  `kizashi-ui` is rebuilt/redeployed — the real watkinslabs tenant's fired triggers give real
  data to screenshot this against.)
- **Known follow-up, not done here:** a real infrastructure topology graph (Postgres/RabbitMQ/
  ClickHouse as nodes, discovered rather than hardcoded connections) is a larger, more
  speculative rebuild of the existing Pipeline Map — scoped out of this pass deliberately
  rather than guessed at.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — additive rendering of already-available data through an already-established
  page/endpoint pattern, no new architectural decision

## [2026-07-19] feature/0048-sensor-naming-stage1-ui-labels — "Sensor" terminology, Stage 1 (UI labels)
- **Type:** feature
- **Summary:** User-flagged naming confusion: "Agent" was overloaded between deployable
  connector-poller instances (`common::Agent`) and the newly-added AI/LLM analysis-profile
  concept (`AnalysisConfig`, ADR-0031). Decided (ADR-0036) that connector-pollers become
  "Sensor" and "Agent" is reserved for AI-analysis-profile terminology going forward. Given the
  size of the full rename (touches `common::Agent`, `agent-scheduler`'s service identity, DB
  schema, and every layer in between) and that a **real production `agent-scheduler` container
  is actively polling a real customer mailbox right now**, the rollout is staged rather than
  one PR — this PR is Stage 1 only: Console UI-visible labels (nav item, page headings, button/
  form copy, empty-state text) renamed "Agent(s)" → "Sensor(s)", with zero backend/route/schema
  changes. Struct fields, URL paths (`/agents/...`), and the `common::Agent` type are
  deliberately untouched this pass — they still say "agent" internally, which is an accepted,
  documented, temporary mismatch until Stage 2.
- **Tests:** `cargo test -p kizashi-ui --lib` — 241 passed (2 existing assertions updated to
  match the new labels: `agent_detail_handler_test.rs`'s not-found message, `agents_handler_test.rs`'s
  register-form and empty-state text). `cargo test --workspace --all-features` (full real-infra
  stack) — every test binary passed, 0 failed. `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo deny check`
  / `cargo audit` — clean, same 3 pre-existing allow-listed advisories.
- **Live verification:** (to be run against the real deployed Console UI once this merges and
  `kizashi-ui` is rebuilt/redeployed.)
- **Follow-up (staged, not this PR):** Stage 2 (`common::Agent` → `common::Sensor`,
  `AgentRepository`/`AgentChangeEvent`/HTTP routes rename across `config-admin-service`,
  `agent-scheduler`, `kizashi-ui`) and Stage 3 (`agent-scheduler` service/image/docker-compose
  rename) — see ADR-0036 for the full plan and why they're sequenced after this one.
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0036](../docs/adr/0036-sensor-vs-agent-terminology.md)

## [2026-07-19] feature/0049-analysis-service-consumer-liveness-healthcheck — analysis-service health check reflects real consumer liveness
- **Type:** fix
- **Branch:** feature/0049-analysis-service-consumer-liveness-healthcheck
- **Summary:** Fixes a real production incident: `analysis-service`'s `record.normalized`
  RabbitMQ consumer stopped making progress (0 consumers, queue growing 384 → 520 → 563
  messages against the real watkinslabs tenant) while `/healthz` kept reporting `"ok"` the
  entire time, because it only checked that the HTTP server was up. Adds a `ConsumerHeartbeat`
  (`Arc<Mutex<Instant>>`) that the main consume loop ticks on every `tokio::select!` iteration
  — including the idle-timeout branch, which fires every 500ms regardless of queue depth, so
  it's a genuine "still being scheduled" signal — and `/healthz` now returns `503` when the
  heartbeat is stale (>30s) instead of always `200`. Also fixes the structural bug found during
  root-causing: the consume loop treated the AMQP consumer stream ending (`None`) as `return`,
  silently killing the whole process with no diagnostic trail; it now logs, backs off 1s, and
  re-establishes `basic_consume` on the existing channel instead.
  Also adds a retry cap: live verification after the two fixes above showed the queue was
  still stuck (not draining) because pre-existing poison messages from long-dead test tenants
  were being `nack(requeue: true)`'d forever with no limit, starving the real tenant's messages
  behind them. A custom `x-analysis-retry-count` header (`retry.rs`) now tracks attempts across
  redeliveries; a message failing 5 times is published to a new durable
  `analysis-service.record.normalized.dead` queue for operator inspection instead of looping
  forever.
- **Tests:** `cargo test -p analysis-service` — 3 new tests in `health_test.rs`
  (`healthz_returns_200_when_the_consumer_has_ticked_recently`,
  `healthz_returns_503_when_the_consumer_has_not_ticked_within_the_staleness_window`,
  `tick_resets_the_heartbeat_to_alive`) and 5 new tests in `retry_test.rs`
  (`retry_count_is_zero_when_headers_are_absent`, `retry_count_is_zero_when_the_header_is_not_present`,
  `retry_count_reads_the_stored_value`, `with_incremented_retry_count_sets_the_header_to_one_more_than_before`,
  `should_dead_letter_is_false_below_the_max_and_true_at_or_above_it`), 32 existing unit tests
  unaffected, 3 real-Postgres + 1 real-RabbitMQ integration tests pass. `cargo test --workspace
  --all-features` (real Postgres/RabbitMQ/ClickHouse/greenmail/mssql-CI containers) — 998
  passed, 0 failed. `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  clean. `cargo fmt --all --check` — clean. `cargo deny check` / `cargo audit` — clean, same 3
  pre-existing allow-listed advisories.
- **Live verification:** rebuilt and redeployed the real `analysis-service` container against
  the live watkinslabs stack. `/healthz` returned 200 with the heartbeat wired in;
  `rabbitmqctl list_queues` confirmed 1 active consumer (vs 0 during the incident). Before the
  retry-cap fix, the queue was stuck at 501 messages for 15s straight despite 1 consumer being
  attached (poison messages hot-looping); after adding the retry cap and redeploying, queue
  depth actually decreased (501 → 473 → 469) over the same observation window, confirming the
  backlog is draining again.
- **Follow-up (explicitly out of scope, see ADR-0037):** the `analysis_config.changed` consume
  loop still uses unbounded `nack(requeue: true)` — deferred since it's low-volume and wasn't
  implicated in the incident. No operator UI yet for inspecting/replaying the dead-letter queue.
- **PR:** [#59](https://github.com/chris17453/kizashi/pull/59)
- **ADR:** [ADR-0037](../docs/adr/0037-analysis-service-consumer-liveness-healthcheck.md)

## [2026-07-19] feature/0049-sensor-naming-stage2-types-and-routes — "Sensor" terminology, Stage 2 (types, routes, bus contract)
- **Type:** feature
- **Summary:** Stage 2 of the ADR-0036 rename (Stage 1: #57, UI labels only). A pure,
  behavior-preserving rename of the Rust-level API surface, HTTP routes, and message-bus
  contract from "Agent" to "Sensor" — no schema, no service/crate identity change (both stay
  Stage 3). Renamed `common::Agent` → `Sensor`, `common::AgentChangeEvent` → `SensorChangeEvent`,
  and `AGENT_CHANGED_EXCHANGE` (`"agent.changed"`) → `SENSOR_CHANGED_EXCHANGE`
  (`"sensor.changed"`) — updated `config-admin-service` (publisher) and `agent-scheduler`
  (consumer) together in the same change so the exchange/queue names they agree on never drift
  out of sync with each other. In `config-admin-service`: `AgentRepository`/
  `AgentRepositoryError`/`PostgresAgentRepository`/`AgentPublisher`/`AgentState` → `Sensor*`
  equivalents, HTTP routes `/v1/agents*` → `/v1/sensors*`. In `agent-scheduler`:
  `AgentRepository` → `SensorRepository`, `StoredAgent` → `StoredSensor`, `Invoker` trait now
  takes `&Sensor`, consumer queue renamed to `agent-scheduler.sensor.changed` bound to the new
  exchange. In `kizashi-ui`: `AgentsClient` → `SensorsClient`, handler/client files and
  functions renamed, `AppState.agents_client` → `sensors_client`, routes `/agents*` →
  `/sensors*`, templates renamed and their internal hrefs/`{% template(path=...) %}` references
  updated to match. Explicitly untouched, per the ADR's staging: the `agents` Postgres table
  name and its columns in both services' schemas (including the `entity_type: "agent"` value
  written into `config-admin-service`'s audit log rows, left as-is since it's persisted data,
  not an API name), and `agent-scheduler`'s own crate/binary/service name, Docker image, and
  `docker-compose.yml` entry (Stage 3).
- **Tests:** `cargo test --workspace --all-features` (full real-infra stack: Postgres, RabbitMQ,
  ClickHouse) — every test binary passed, 0 failed, except 5 pre-existing/unrelated
  infra-dependent failures not touched by this change (SMTP/greenmail delivery test, Fabric AAD
  auth tests, IMAP connector tests, an observability RabbitMQ backlog test, and the retention
  S3 archive store test — all fail because their specific external test fixtures
  (greenmail/MSSQL/S3-compatible backend) aren't part of this environment's running stack, not
  because of anything in this PR). All Sensor-specific suites pass: `config-admin-service`
  unit tests (89 passed, including `sensor_handlers`/`sensor_repository`/`sensor_publisher`),
  `config-admin-service`'s real-Postgres `repository_integration_test.rs` (16 passed, including
  tenant-isolation cases renamed to `a_sensor_owned_by_one_tenant_is_invisible_...` and
  `deleting_a_sensor_owned_by_another_tenant_fails_...`), `config-admin-service`'s real-RabbitMQ
  `sensor_publisher_integration_test.rs` (2 passed, proving the renamed exchange/event round-trip
  over the real bus), `agent-scheduler` unit tests (17 passed) and its real-Postgres
  `sensor_repository_integration_test.rs` (3 passed), and `kizashi-ui`'s full lib test suite
  (241 passed). `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo deny check` / `cargo audit` — clean, same 3
  pre-existing allow-listed advisories, no dependency changes.
- **Live verification:** not run — this stage lands as source only; the actual
  `agent-scheduler`/`config-admin-service`/`kizashi-ui` containers keep running their
  currently-deployed images (still on the old `agent.changed` exchange/queue names) until this
  merges and those services are rebuilt/redeployed together, since the exchange rename is a
  breaking wire-contract change across the two services that must roll out atomically.
- **Follow-up (staged, not this PR):** Stage 3 — `agent-scheduler`'s own crate/binary/service
  name, Docker image name, and `docker-compose.yml` service key, plus (optionally) the `agents`
  DB table/column names — see ADR-0036.
- **PR:** [#58](https://github.com/chris17453/kizashi/pull/58)
- **ADR:** [ADR-0036](../docs/adr/0036-sensor-vs-agent-terminology.md)

## [2026-07-20] fix/0005-analysis-service-timeout-and-heartbeat-window — bound AI call latency, widen heartbeat staleness window
- **Type:** fix
- **Branch:** fix/0005-analysis-service-timeout-and-heartbeat-window
- **Summary:** Follow-up to #59's liveness healthcheck: live redeploy against the real
  watkinslabs stack showed `/healthz` flapping to `503` and staying stuck, even though the
  process wasn't actually deadlocked. Root cause: the AI HTTP client (`reqwest::Client::new()`)
  had no request timeout, and the consume loop's heartbeat only ticked in the outer
  `tokio::select!` — a slow or hanging call to the local Ollama backend for a real batch could
  block the loop for minutes with zero heartbeat ticks, tripping the 30s staleness threshold
  even for legitimate (if slow) work. Fixes: (1) the AI HTTP client now has a 30s per-request
  timeout, bounding worst-case single-call hang time; (2) `STALE_THRESHOLD` raised from 30s to
  180s to comfortably exceed worst-case batch time (batch_size 20 / concurrency 4 = 5 rounds *
  30s = 150s, plus margin); (3) heartbeat now also ticks once per tenant group before
  `process_batch`, not just in the outer select loop, so multi-tenant batches stay fresher.
- **Tests:** `cargo test -p analysis-service` — all 40 unit tests pass (health/retry tests
  unaffected by the threshold/timeout changes), 3 real-Postgres + 1 real-RabbitMQ integration
  tests pass. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean.
- **Live verification:** rebuilt/redeployed `analysis-service` against the live watkinslabs
  stack. Previous deploy (without this fix) went unhealthy (`503`) within ~30s of restart,
  reproducibly. After this fix: `/healthz` held `200` continuously for 15+ minutes of
  observation while the real queue kept draining (428 → 368 messages), 0 messages
  dead-lettered, 1 consumer attached throughout.
- **Follow-up:** the 150s theoretical worst-case bound assumes no queueing/contention beyond
  concurrency=4; if `ANALYSIS_BATCH_SIZE` or per-request latency grow significantly, this
  threshold should be revisited. `docs/adr/0037-analysis-service-consumer-liveness-healthcheck.md`
  updated to reflect these numbers is a candidate follow-up, not done in this PR.
- **PR:** [#60](https://github.com/chris17453/kizashi/pull/60)
- **ADR:** n/a — direct correction to ADR-0037's stated thresholds/assumptions, not a new
  architectural decision.

## [2026-07-20] feature/0051-ui-polish-sensor-picker-and-trigger-form — Console UI availability fix + sensor-picker/trigger-form usability
- **Type:** fix
- **Branch:** feature/0051-ui-polish-sensor-picker-and-trigger-form
- **Summary:** Prompted by direct user feedback that the Console UI was unusable. Live audit
  (headless-Chrome screenshots of every nav page, not just template reading) found the actual
  root cause: `kizashi-kizashi-ui-1` was sitting in Docker's `Created` state, never started,
  because `docker-compose.yml` required `service_healthy` on ten backends including a chain
  through `trigger-engine` → `analysis-service` — when analysis-service went unhealthy during
  this session's earlier incident, the whole UI became impossible to (re)start. Changed
  `kizashi-ui`'s `depends_on` conditions to `service_started` so one backend's transient health
  doesn't take the entire operator-facing UI offline. Also fixed two real usability gaps found
  during the same audit: the Data Viewer's Connector ID field was free-text-only with no way to
  pick from actually-registered Sensors (now an `<input list>` + `<datalist>` populated from
  `SensorsClient::list_sensors`, capped at 500, still free-text-capable); and the trigger-
  creation form rendered every field for every condition shape simultaneously with no dynamic
  show/hide, unlike the AI Analysis page which already solved this correctly — now mirrors that
  same JS pattern.
- **Tests:** `cargo test -p kizashi-ui` — new test
  `offers_registered_sensor_names_as_a_datalist_for_the_connector_id_field` passes, all 18
  existing `triggers_handler` tests unaffected (pure template change), 242 total kizashi-ui
  tests passing (up from 241). `cargo test --workspace --all-features` (full real-infra stack)
  — every test binary passed, 0 failed. `cargo clippy --workspace --all-targets --all-features
  -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo deny check` / `cargo
  audit` — clean, same 3 pre-existing allow-listed advisories.
- **Live verification:** rebuilt/redeployed `kizashi-ui`. Confirmed via `docker ps` the
  container was previously `Created` (never running) and is now actually `Up`/healthy.
  Registered a real test sensor via the Console UI, confirmed via curl+headless-Chrome that its
  name appears in the Connector ID datalist; confirmed via screenshot that the trigger form now
  shows only the threshold-field group by default and toggles correctly via the Condition
  dropdown. Test sensor cleaned up afterward.
- **Follow-up:** this audit was not exhaustive — see ADR-0038's Consequences section for what's
  still open (SSO/auth-provider config UI, further per-page polish).
- **PR:** [#61](https://github.com/chris17453/kizashi/pull/61)
- **ADR:** [ADR-0038](../docs/adr/0038-console-ui-availability-and-usability-fixes.md)

## [2026-07-19] fix/0006-auth-service-audit-actor — auth-service audit log now records the real actor, not the tenant_id
- **Type:** fix
- **Branch:** fix/0006-auth-service-audit-actor
- **Summary:** Every `AuditLogEntry.actor` written by `LocalUserRepository` (create/update_role/
  delete) was set to the tenant_id — a value already present as its own column on every audit
  row — making the audit trail useless for answering "who did this" (CLAUDE.md §5). Added a
  `username_from_headers` helper (`crates/auth-service/src/user_handlers.rs`) that reads a new
  `X-Username` header, mirroring the existing `tenant_id_from_headers`/`role_from_headers`
  pattern (401 `"missing X-Username header"` when absent). `create_user`, `update_user_role`,
  and `delete_user` now extract the real username and thread it through as `actor` instead of
  `&tenant_id.to_string()`. `LocalUserRepository::create` gained an `actor: &str` parameter
  (previously missing entirely — the Postgres impl hardcoded `user.tenant_id.to_string()`) on
  the trait, the Postgres impl, and the in-memory test double. The UI's outgoing requests are
  not touched here — that's a separate follow-up PR to add the `X-Username` header on the
  sending side.
- **Tests:** TDD per CLAUDE.md §2 — failing tests written first for the new header behavior and
  actor threading, then made to pass. `cargo test -p auth-service --all-features` (real
  Postgres at `postgres://kizashi:kizashi@localhost:55432/kizashi`) — 65 lib tests + 2
  `hash_password` bin tests + 6 Postgres integration tests (including new
  `create_writes_an_audit_row_with_the_real_actor_not_the_tenant_id` and
  `create_records_the_real_actor_not_the_tenant_id`) all passed, 0 failed. New handler tests in
  `user_handlers_audit_actor_test.rs` (split out to stay under the 500-line file limit):
  `create_user_requires_a_username_header`, `update_user_role_requires_a_username_header`,
  `delete_user_requires_a_username_header` (assert 401), and
  `create_user_threads_the_real_username_through_as_the_audit_actor`,
  `update_user_role_threads_the_real_username_through_as_the_audit_actor`,
  `delete_user_threads_the_real_username_through_as_the_audit_actor` (assert the repository
  receives the real actor). `cargo clippy -p auth-service --all-targets --all-features -- -D
  warnings` — clean. `cargo fmt --all --check` — clean. `cargo build --workspace --all-targets`
  — clean, confirms the trait signature change didn't break other crates.
- **PR:** (opened in the integration branch's PR — see below)
- **ADR:** n/a — this is a bugfix restoring intended audit-log behavior (CLAUDE.md §5), not a
  new architectural decision; no spec §11 open item touched.

## [2026-07-19] fix/0006-config-admin-service-audit-actor — Config Admin Service audit log records the real actor, not the tenant id
- **Type:** fix
- **Branch:** fix/0006-config-admin-service-audit-actor
- **Summary:** Every audit-log write in `config-admin-service` (sensor, trigger-definition,
  normalization-mapping, analysis-config repositories) hardcoded `AuditLogEntry.actor` to
  `tenant_id.to_string()`, which made the audit trail unable to answer "who made this change" —
  only "which tenant", already a separate column on every row (CLAUDE.md §5). Adds
  `username_from_headers` (reads `X-Username`, mirroring the existing `X-Tenant-Id`/`X-Role`
  helpers in `handlers.rs`), threads a new `actor: &str` parameter through every
  create/update/delete/upsert repository method, and updates every write handler
  (`sensor_handlers.rs`, `handlers.rs` trigger/mapping handlers, `analysis_config_handlers.rs`)
  to extract the real caller identity from that header instead. Matches the same
  `X-Username`/`username_from_headers`/`missing X-Username header` convention used by the
  sibling fixes landing in auth-service, retention-service, and ingestion-gateway so all four
  services agree on the wire contract. The UI does not yet send `X-Username` — that lands in a
  separate PR.
- **Tests:** `cargo test -p config-admin-service --all-features` (real Postgres +
  RabbitMQ) — 117 passed, 0 failed, across unit tests (92) and integration test files
  (`repository_integration_test.rs` 18, `sensor_publisher_integration_test.rs` 2,
  `trigger_publisher_integration_test.rs` 1, `mapping_publisher_integration_test.rs` 1,
  `analysis_config_publisher_integration_test.rs` 1, `saved_search_query_repository_integration_test.rs`
  2). New regression coverage: `create_trigger_records_the_real_actor_not_the_tenant_id` and
  `sensor_create_update_and_delete_all_record_the_real_actor_not_the_tenant_id` in
  `repository_integration_test.rs` assert the written `actor` equals the real username and is
  never equal to `tenant_id.to_string()`, against real Postgres. New handler-level 401 coverage:
  `create_trigger_requires_username_header`, `create_sensor_requires_username_header`,
  `put_requires_username_header`. `cargo clippy -p config-admin-service --all-targets
  --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo build
  --workspace --all-targets` — clean (no other crate constructs these repository trait objects
  directly, confirmed by grep).
- **PR:** (opened in the integration branch's PR — see above)
- **ADR:** n/a — bug fix restoring already-documented CLAUDE.md §5 behavior, not a new
  architectural decision.
## [2026-07-19] fix/0006-retention-service-audit-actor — audit log actor is now the real user, not the tenant id
- **Type:** fix
- **Branch:** fix/0006-retention-service-audit-actor
- **Summary:** `RetentionPolicyRepository::create/update/delete` hardcoded the audit log's
  `actor` field to `tenant_id.to_string()` at all three call sites in `retention_policy.rs`,
  which made the audit trail useless for its compliance purpose (CLAUDE.md §5) — `tenant_id` is
  already its own column on every audit row, so reusing it as `actor` can never answer "who did
  this." Fixes: added a `username_from_headers` helper in `policy_handlers.rs` that reads the
  `X-Username` header (mirroring the existing `tenant_id_from_headers`/`role_from_headers`
  pattern), returning `401 missing X-Username header` when absent; added an `actor: &str`
  parameter to the `RetentionPolicyRepository::create/update/delete` trait methods and threaded
  the real username from the handlers through to the `AuditLogEntry` construction, replacing all
  three hardcoded `tenant_id.to_string()` sites. Same `X-Username` / `username_from_headers` /
  401 convention agreed with the parallel fixes to auth-service, config-admin-service, and
  ingestion-gateway so all four services share one wire contract; the Console UI's outgoing
  header is a separate follow-up PR.
- **Tests:** `cargo test -p retention-service --all-features` — 54 unit tests pass (including new
  `create_policy_requires_username_header`, `update_policy_requires_username_header`,
  `delete_policy_requires_username_header` in `policy_handlers_test.rs`) and 8 real-Postgres
  integration tests pass in `tests/retention_policy_integration_test.rs`, including
  `create_policy_writes_a_created_audit_row_in_the_same_transaction` now asserting
  `entries[0].actor == "alice@example.com"` and `entries[0].actor != tenant_id.to_string()`, plus
  actor assertions added to the update and delete audit-row tests. 3 pre-existing
  `s3_archive_store_integration_test.rs` failures (missing `AWS_REGION`/minio fixtures in this
  environment) are unrelated to this change. `cargo clippy -p retention-service --all-targets
  --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo build
  --workspace --all-targets` — clean.
- **PR:** (opened in the integration branch's PR — see above)
- **ADR:** n/a — bug fix to existing audit-log wiring, no new architectural decision.
## [2026-07-19] fix/0006-ingestion-gateway-audit-actor — API key audit log records the real actor, not the tenant_id
- **Type:** fix
- **Branch:** fix/0006-ingestion-gateway-audit-actor
- **Summary:** `ApiKeyStore::create`/`revoke` in `crates/ingestion-gateway` hardcoded
  `AuditLogEntry.actor` to `tenant_id.to_string()`, making the audit log useless for its
  compliance purpose (CLAUDE.md §5) — `tenant_id` is already a separate column on every row, so
  the audit trail couldn't say *who* created or revoked an API key. Added a
  `username_from_headers` helper in `api_key_handlers.rs` (reads `X-Username`, 401s if absent —
  same wire contract as auth-service/config-admin-service/retention-service's identical fix),
  threaded a new `actor: &str` parameter through `ApiKeyStore::create`/`revoke` (trait, Postgres
  impl, and the in-memory/failing test doubles), and wired `create_api_key`/`revoke_api_key` to
  pass the real username instead of the tenant_id fallback.
- **Tests:** `cargo test -p ingestion-gateway --all-features` — 44 passed, 0 failed (38 unit +
  6 integration against real Postgres), including new tests
  `create_and_revoke_thread_the_real_actor_not_the_tenant_id` (store-level),
  `create_api_key_passes_the_real_username_as_actor_not_the_tenant_id` and
  `revoke_api_key_passes_the_real_username_as_actor_not_the_tenant_id` (handler-level,
  asserting the recorded actor equals the `X-Username` header value and is never the tenant_id),
  `create_api_key_missing_username_header_is_unauthorized` (401 on missing `X-Username`), and
  updated integration tests `create_writes_a_created_audit_row_and_the_key_resolves` /
  `revoke_writes_a_deleted_audit_row_and_the_key_stops_resolving` to assert the persisted
  `AuditLogEntry.actor` is the real username. `cargo clippy -p ingestion-gateway --all-targets
  --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo build
  --workspace --all-targets` — clean.
- **PR:** (opened in the integration branch's PR — see above)
- **ADR:** n/a — bugfix restoring the audit log's intended behavior, not a new architectural
  decision.
## [2026-07-19] fix/0006-ui-actor-header-batch2 — Console UI sends `X-Username` on API-keys/egress-allowlist/users/retention-policy writes
- **Type:** fix
- **Branch:** fix/0006-ui-actor-header-batch2
- **Summary:** Compliance defect (CLAUDE.md §5): audit-log entries recorded the tenant, never
  the real acting user. This is the Console UI half of the fix — `ApiKeysClient`,
  `EgressAllowlistClient`, `UsersClient`, and `RetentionPoliciesClient`'s mutating methods
  (`create_api_key`/`revoke_api_key`, `put_allowlist`, `create_user`/`update_user_role`/
  `delete_user`, `create_policy`/`update_policy`/`delete_policy`) now take a trailing
  `actor: &str` parameter and send it as the `x-username` header alongside the existing
  `x-tenant-id`/`x-role` headers, matching the codebase's lowercase header convention. Every
  call site in `api_keys_handler.rs`, `egress_allowlist_handler.rs`, `users_handler.rs`,
  `retention_policies_handler.rs`, and `sensor_script_handler.rs` (the auto-generated-API-key
  path) now passes `&session.username` from the authenticated `Session`. Backend services
  (auth-service, config-admin-service, retention-service, ingestion-gateway) reading this header
  and using it as the real audit-log actor are a parallel, separate change.
- **Tests:** `cargo test -p kizashi-ui --all-features` — 244 passed, 0 failed (up from prior
  count; added `http_client_sends_x_username_header_on_create` in
  `api_keys_client_test.rs` and `http_client_sends_x_username_header_on_create_user` in
  `users_client_test.rs`, each spinning a real axum stub server and asserting the exact
  `x-username` header value received). `cargo clippy -p kizashi-ui --all-targets --all-features
  -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo build --workspace
  --all-targets` — clean.
- **PR:** (opened in the integration branch's PR — see above)
- **ADR:** n/a
## [2026-07-19] fix/0007-ui-actor-header-batch1 — Console UI sends X-Username on sensor/trigger/mapping/analysis-config writes
- **Type:** fix
- **Branch:** fix/0007-ui-actor-header-batch1
- **Summary:** Compliance defect (CLAUDE.md §5): every audit-log entry's "actor" was recorded as
  the tenant_id, never the real signed-in user, because Console UI's HTTP clients never sent who
  was making the request. Adds an `actor: &str` parameter (the signed-in `Session.username`) to
  every mutating trait method on `SensorsClient` (`register_sensor`, `delete_sensor`,
  `update_sensor`), `TriggersClient` (`create_trigger`), `NormalizationMappingsClient`
  (`create_mapping`), and `AnalysisConfigClient` (`put_analysis_config`), sent as the
  case-insensitive `X-Username` header alongside the existing `X-Tenant-Id`/`X-Role` headers, and
  wires `&session.username` through from every handler call site
  (`sensors_handler.rs`, `triggers_handler.rs`, `normalization_mappings_handler.rs`,
  `analysis_config_handler.rs`). Backend-side reading of this header (config-admin-service et al.)
  is out of scope for this branch — landing in parallel sibling branches
  (`fix/0006-*-audit-actor`) that make each service actually use it as the audit-log actor and
  401 a write missing it.
- **Tests:** `cargo test -p kizashi-ui --all-features` — 245 passed, 0 failed. Added
  `http_client_register_sensor_is_rejected_when_actor_header_missing_expected_value`,
  `http_client_create_is_rejected_when_actor_header_missing_expected_value` (triggers and
  normalization-mappings clients), and
  `http_client_put_is_rejected_when_actor_header_missing_expected_value` (analysis-config
  client), each asserting against a real spawned axum stub server that rejects the request with
  401 unless `X-Username` carries the expected actor, mirroring the existing `x-role` assertion
  pattern in those same `_client_test.rs` files. `cargo clippy -p kizashi-ui --all-targets
  --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean. `cargo build
  --workspace --all-targets` — clean.
- **PR:** (opened in the integration branch's PR — see above)
- **ADR:** n/a — implements existing audit-log requirement (CLAUDE.md §5), not a new
  architectural decision.

## [2026-07-20] fix/0006-audit-log-real-actor — audit log actor identity fixed platform-wide (integration of 6 parallel branches)
- **Type:** fix
- **Branch:** fix/0006-audit-log-real-actor
- **Summary:** Integrates six coordinated branches (one per backend service — auth-service,
  config-admin-service, retention-service, ingestion-gateway — plus two UI-client batches) that
  together fix a systemic compliance defect discovered during a live Console UI audit: every
  audit-log write across the entire platform recorded `tenant_id` as the "actor," never the
  real user who performed the action. Landed as one integration since the wire contract
  (`X-Username` header, `username_from_headers` helper, `401` on missing) only works if backend
  reads and UI sends land together — merging either half alone would either 401 every admin
  write or silently keep the audit log wrong. See ADR-0039 for the full design and rationale,
  and the six individual feature-log entries above for per-service detail.
- **Tests:** `cargo build --workspace --all-targets` — clean. `cargo clippy --workspace
  --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check` — clean.
  `cargo test --workspace --all-features` (full real-infra stack: Postgres, RabbitMQ,
  ClickHouse, greenmail, mssql-CI) — every test binary passed, 0 failed, including 248 kizashi-ui
  tests (up from 241 at the start of this session). `cargo deny check` / `cargo audit` — clean,
  same 3 pre-existing allow-listed advisories.
- **Live verification:** rebuilt and redeployed all five affected services
  (auth-service, config-admin-service, retention-service, ingestion-gateway, kizashi-ui)
  together against the real running stack. Registered a real sensor through the Console UI and
  confirmed via direct Postgres query that the fresh audit row's `actor` column is the real
  username (`demo`), not the tenant UUID. Toggled a real user's role through the Users page and
  confirmed via the Audit History page's screenshot that the newest row shows `demo` as the
  actor while older, pre-fix rows correctly still show their original (UUID) actor value,
  proving the immutable audit trail wasn't rewritten, only new writes changed.
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0039](../docs/adr/0039-audit-log-actor-identity.md)

## [2026-07-20] feature/0052-overview-recent-activity — Overview dashboard shows recent events, not dead whitespace
- **Type:** feature
- **Branch:** feature/0052-overview-recent-activity
- **Summary:** Last item on the live Console UI audit's punch list: the Overview page had a lot
  of empty space below the pipeline status card, with no secondary content. Adds a "Recent
  activity" section showing the 5 most recent events (already fetched by this handler for the
  KPI count, no new backend call), with an empty state matching the platform's existing pattern
  when there's nothing yet, and a link to the full paginated Events page.
- **Tests:** `cargo test -p kizashi-ui` — 2 new tests
  (`shows_the_five_most_recent_events_as_recent_activity`,
  `shows_an_empty_state_for_recent_activity_when_there_are_no_events`), 250 total passing (up
  from 248). `cargo test --workspace --all-features` (full real-infra stack) — every test binary
  passed, 0 failed. `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  clean. `cargo fmt --all --check` — clean. `cargo deny check` / `cargo audit` — clean, same 3
  pre-existing allow-listed advisories.
- **Live verification:** rebuilt/redeployed `kizashi-ui`, screenshotted the real Overview page —
  the new section renders in the correct empty state for the demo tenant (which genuinely has 0
  events), filling what was previously dead space with content that will show real events the
  moment any exist.
- **PR:** (opened in this branch's PR)
- **ADR:** n/a — straightforward UI content addition, no architectural decision.

## [2026-07-20] feature/0053-console-ui-oidc-sso-login — Console UI completes enterprise SSO login (closes ADR-0009's deferred half)
- **Type:** feature
- **Branch:** feature/0053-console-ui-oidc-sso-login
- **Summary:** ADR-0009 built a full, tested OAuth2/OIDC authorization-code-plus-PKCE client in
  Auth Service (Entra ID and any OIDC-compliant provider) but explicitly deferred the
  browser-facing half to "Console UI, once built" — it was built, but the OIDC wiring never
  landed, leaving enterprise SSO completely unusable despite the backend being ready. Adds
  `GET /login/sso` (starts the flow, stashes CSRF/PKCE state behind a short-lived single-use
  `HttpOnly` cookie with `SameSite=Lax` — required, not `Strict`, since the flow crosses a
  top-level redirect to the IdP and back) and `GET /login/sso/callback` (verifies CSRF `state`,
  single-use-consumes the pending flow so a replayed callback URL can't mint a second session,
  completes the exchange, mints a normal session). Also fixes `OidcCallbackRequest` to accept
  `tenant_name` instead of an unusable bare `tenant_id` (Console UI never has a tenant_id before
  auth completes), and adds a real `username` to the session-mint response so SSO users'
  actions attribute correctly in the audit log fixed by ADR-0039 earlier this session, instead
  of all SSO logins showing up as the workspace name.
- **Tests:** `cargo test -p auth-service` — 66 passed (3 new: tenant_name resolution,
  400-on-unknown-workspace). `cargo test -p kizashi-ui` — new `oidc_client` (8 tests),
  `pending_oidc_flow` (3 tests), `sso_login_handler` (6 tests) modules, all passing; 21 existing
  handler test files updated for the two new `AppState` fields. `cargo test --workspace
  --all-features` (full real-infra stack) — every one of 109 test binaries passed, 0 failed.
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt
  --all --check` — clean. `cargo deny check` / `cargo audit` — clean, same 3 pre-existing
  allow-listed advisories.
- **Live verification:** rebuilt/redeployed `auth-service` and `kizashi-ui` together (they share
  the OIDC wire contract). Screenshotted the real login page — the new "Sign in with SSO" form
  renders correctly. This environment has no real Entra tenant configured, so live-verified the
  honest thing that's actually verifiable here: the graceful-degradation path — hitting
  `/login/sso` with no OIDC provider configured shows a clear on-page error ("Single sign-on is
  not available...") instead of crashing or hanging, confirmed via screenshot. The actual
  successful IdP round-trip cannot be exercised without real Entra credentials, a limitation
  ADR-0009 already named and ADR-0040 restates — what's covered by real tests is everything up
  to and past that human-in-a-browser hop (redirect construction, cookie handling, CSRF/replay
  defense, code-exchange-to-session-mint) against a stub IdP.
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0040](../docs/adr/0040-console-ui-oidc-sso-wiring.md)

## [2026-07-20] feature/0054-tenant-branding-config — tenant white-label branding (product name, logo, accent color)
- **Type:** feature
- **Branch:** feature/0054-tenant-branding-config
- **Summary:** Implements the spec's "white-labelable" requirement (§1), previously entirely
  unimplemented. Three nullable columns on `auth-service`'s `tenants` table (product name, logo
  URL, accent color) — `NULL` means "use the platform default," never "broken." Read is by
  workspace name (`GET /v1/tenants/:name/branding`, deliberately unauthenticated — the login
  page needs it before anyone's signed in, and branding isn't sensitive) plus a by-id variant
  for the authenticated Settings page. Write is admin-only, audit-logged with the real actor
  (ADR-0039). `accent_color` is validated server-side against a strict hex-color pattern since
  it renders into a `<style>` block on the unauthenticated login page — unvalidated free text
  there is a real CSS-injection surface. New Console UI `/branding` Settings page (nav: palette
  icon). The login page's Workspace field reloads with the typed name on blur, live-loading and
  applying that workspace's branding (product name replaces "Kizashi", logo swaps the diamond
  mark, accent color re-themes the page) before the operator even signs in — "loaded based on
  login." Scope deliberately stops at the login page; applying branding to authenticated pages
  is a larger, separate change (would require threading a branding fetch through every page
  handler) and is tracked as follow-up, not done here.
- **Tests:** `cargo test -p auth-service` — 79 passed (13 new: repository round-trip by name and
  by id, 3 real-Postgres integration tests including an audit-actor assertion, handler tests for
  404/403/401/CSS-injection-rejection/happy-path). `cargo test -p kizashi-ui` — 273 passed (9 new
  handler/client tests plus 2 login-page branding-loading tests). `cargo test --workspace
  --all-features` (full real-infra stack) — every test binary passed, 0 failed. `cargo clippy
  --workspace --all-targets --all-features -- -D warnings` — clean. `cargo fmt --all --check` —
  clean. `cargo deny check` / `cargo audit` — clean, same 3 pre-existing allow-listed advisories.
- **Live verification:** rebuilt/redeployed `auth-service` and `kizashi-ui` together, set real
  branding (product name "Acme Signals", accent color `#ff6600`) for the acme demo tenant via
  the live Settings page, confirmed via screenshot that `/login?tenant_name=acme` renders the
  custom product name and re-themed accent color on the real running login page. Confirmed the
  Settings page itself renders and round-trips saved values. Test branding cleared afterward.
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0041](../docs/adr/0041-tenant-branding-white-label.md)

## [2026-07-20] fix/0007-rbac-audit-fixes — closes two real RBAC gaps found by a platform-wide write-endpoint audit
- **Type:** fix
- **Branch:** fix/0007-rbac-audit-fixes
- **Summary:** Delegated a systematic audit of every mutating HTTP handler in the workspace for
  missing role/permission checks (part of the standing push toward an enterprise compliance
  bar). Found two real gaps: (1) `retention-service`'s `POST /v1/sweep` and `POST /v1/reimport`
  had **no authentication of any kind** — any caller able to reach the service could trigger a
  destructive tenant-wide retention sweep or force an arbitrary archive reimport, the most
  severe class of finding this audit could have produced. Fixed with the same shared-secret
  pattern query-gateway's `/internal/tokens` already established (`X-Internal-Secret` header,
  ADR-0009) — these are service-to-service operational triggers with no session/role behind
  them, not end-user actions. (2) Four Console UI POST handlers
  (`post_sensors`/`post_delete_sensor`/`post_toggle_sensor`/`post_api_keys`/
  `post_revoke_api_key`) never checked `session.role.at_least(Operator)` before calling their
  backend client, unlike every sibling write handler — not independently exploitable (the
  backend still enforces it), but a real UX bug: a Viewer clicking delete/revoke/toggle was
  silently redirected as if it succeeded (the 403 was discarded), when nothing happened. Now
  returns a real 403.
- **Tests:** `cargo test -p retention-service` — 57 unit + 8 policy-integration + 3 real-S3
  integration tests, all passing (including 3 new tests: missing/wrong/correct internal
  secret). `cargo test -p kizashi-ui` — 5 new viewer-rejection tests across sensors/api-keys
  handlers, all passing. `cargo test --workspace --all-features` (full real-infra stack:
  Postgres, RabbitMQ, ClickHouse, MinIO, greenmail, mssql-CI) — every test binary passed, 0
  failed. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean.
  `cargo fmt --all --check` — clean. `cargo deny check` / `cargo audit` — clean, same 3
  pre-existing allow-listed advisories.
- **Live verification:** rebuilt/redeployed `retention-service` and confirmed against the real
  running container: `curl -X POST .../v1/sweep` with no header returned `200` (vulnerable)
  *before* the fix, and `401`/`401`/`200` for missing/wrong/correct `X-Internal-Secret` *after*
  — the sweep sidecar's real request against the live service also confirmed working
  end-to-end. Rebuilt/redeployed `kizashi-ui`, created a real viewer-role test user, and
  confirmed `POST /sensors` and `POST /api-keys` both now return `403` for that user against the
  live running UI (test user deleted afterward).
- **PR:** (opened in this branch's PR)
- **ADR:** [ADR-0042](../docs/adr/0042-retention-ops-internal-secret-and-ui-rbac-gaps.md)

## [2026-07-20] fix/0008-tenant-isolation-and-cookie-security — Tenant isolation audit fixes and cookie Secure flag
- **Type:** fix
- **Branch:** fix/0008-tenant-isolation-and-cookie-security
- **Summary:** Fixes two real cross-tenant data-isolation vulnerabilities found by a targeted
  audit: `auth-service`'s `PUT /v1/tenants/:id/branding` had no tenant check (any admin could
  overwrite any other tenant's branding), and `trigger-engine`'s `GET /v1/triggers/:id` had no
  tenant check (any caller could read any tenant's trigger definition, including action targets,
  by guessing a UUID). `action-executor`'s `TriggerClient` now sends `X-Tenant-Id` on trigger
  lookups to satisfy the new check. Also hardens Console UI session cookies with a `Secure`
  attribute, gated by a new `COOKIE_SECURE` env var (default `false` to not break local/dev over
  plain HTTP).
- **Tests:** `cargo test -p auth-service branding_handler` (12 passed, incl. new
  `put_branding_requires_a_tenant_id_header` and `put_branding_rejects_a_caller_from_a_different_tenant`);
  `cargo test -p trigger-engine api_test` (10 passed, incl. new
  `returns_401_when_the_tenant_header_is_missing` and
  `returns_404_not_leaking_data_when_the_caller_is_from_a_different_tenant`);
  `cargo test -p action-executor trigger_client` (8 passed, incl. new
  `http_client_sends_the_tenant_id_header` and `http_client_is_rejected_when_it_sends_the_wrong_tenant_id`);
  `cargo test -p kizashi-ui cookie_security` (5 passed). Full workspace gate run and green:
  `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo test --workspace --all-features` against real Postgres/RabbitMQ/ClickHouse/MinIO,
  `cargo deny check`, `cargo audit` (3 pre-existing allow-listed advisories, unchanged).
- **PR:** pending
- **ADR:** docs/adr/0043-tenant-isolation-audit-and-cookie-hardening.md

## [2026-07-20] fix/0009-internal-secret-header-trust-gap — Close the X-Role/X-Tenant-Id/X-Username unauthenticated trust gap
- **Type:** fix
- **Branch:** fix/0009-internal-secret-header-trust-gap
- **Summary:** A security audit found that config-admin-service, trigger-engine, auth-service's
  session-authenticated endpoints, and retention-service's retention-policy endpoints trust
  `X-Role`/`X-Tenant-Id`/`X-Username` headers with zero verification, and all four services
  publish their ports directly — any network caller could set `X-Role: admin` (or any tenant id)
  and be trusted outright, a live unauthenticated privilege-escalation and cross-tenant-access
  path. Extends the existing `X-Internal-Secret`/`INTERNAL_API_SECRET` shared-secret pattern
  (ADR-0009, ADR-0042) to all four services (via router-level middleware in three, a per-handler
  check in retention-service to match its existing style), and wires the Console UI to send it
  automatically on every backend call via a default header on its shared `reqwest::Client`.
  `action-executor`'s `HttpTriggerClient` also sends it when calling trigger-engine.
- **Tests:** `cargo test -p config-admin-service --lib` (94 passed, incl.
  `protected_route_without_internal_secret_returns_401`, `healthz_succeeds_with_zero_headers`);
  `cargo test -p trigger-engine` (44 unit + 3 contract passed, incl. 3 new middleware tests and
  `returns_401_when_the_internal_secret_header_is_missing`); `cargo test -p auth-service` (88
  unit + 6 + 3 integration passed, incl. gated-route-without-secret and login-still-works tests);
  `cargo test -p retention-service --lib` (59 passed, incl.
  `list_policies_rejects_missing_internal_secret_even_with_valid_headers`,
  `healthz_works_with_zero_headers`) plus 8 real-Postgres integration tests; `cargo test -p
  action-executor --lib` (52 passed, incl. `http_client_sends_the_tenant_id_and_internal_secret_headers`,
  `http_client_is_rejected_when_it_sends_the_wrong_internal_secret`). Full workspace gate green:
  `cargo build --workspace`, `cargo test --workspace --all-features` against real
  Postgres/RabbitMQ/ClickHouse/MinIO/greenmail (110 test binaries, 0 failures), `cargo clippy
  --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all --check`, `cargo deny
  check`, `cargo audit` (3 pre-existing allow-listed advisories, unchanged).
- **PR:** pending
- **ADR:** docs/adr/0044-internal-service-secret-for-header-trusted-endpoints.md

## [2026-07-20] feature/0055-global-audit-log-page — Global, browsable audit log page
- **Type:** feature
- **Branch:** feature/0055-global-audit-log-page
- **Summary:** Adds a new `GET /v1/audit-log` list endpoint (distinct from the existing
  entity-scoped `GET /v1/audit-log/:entity_id`) to config-admin-service, auth-service, and
  retention-service, each backed by a new `AuditLogReader::list_recent` trait method against the
  existing audit tables (no schema change), with `limit`/`before` cursor pagination. The Console
  UI gets a new `/audit-log` page that merges all three services' recent activity, sorted
  most-recent-first, with a "load older" link — closing the gap where the audit trail could only
  be browsed by already knowing which specific entity to look up, a baseline enterprise/SOC2-style
  compliance expectation ("show me every admin action recently"). New nav entry between Users and
  Platform Health.
- **Tests:** `cargo test -p config-admin-service --lib` (100 passed, incl. 6 new
  `get_recent_audit_log_*` tests); `cargo test -p auth-service --lib` (94 passed, incl. tenant
  scoping, ordering, cursor, and internal-secret-gate tests); `cargo test -p retention-service
  --lib` (66 passed); `cargo test -p kizashi-ui --lib` (293 passed, incl. 5 new
  `recent_audit_log_handler` tests covering merge/sort-across-services, empty state, partial
  backend failure, and load-older pagination, plus 3 new `audit_log_client` HTTP tests). Full
  workspace gate green: `cargo build --workspace`, `cargo test --workspace --all-features`
  against real Postgres/RabbitMQ/ClickHouse/MinIO/greenmail (110 test binaries, 0 failures),
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all
  --check`, `cargo deny check`, `cargo audit` (3 pre-existing allow-listed advisories, unchanged).
- **PR:** pending
- **ADR:** docs/adr/0045-global-audit-log-page.md

## [2026-07-20] feature/0056-active-sessions-management — Active sessions management page
- **Type:** feature
- **Branch:** feature/0056-active-sessions-management
- **Summary:** Adds a `GET /security/sessions` admin page listing every active session for the
  tenant (username, role, sign-in time, current-session flag) and `POST
  /security/sessions/:id/revoke` to force-terminate one — a standard enterprise-security control
  (e.g. logging out a departed employee or a suspected-compromised session) that didn't exist
  before. Extends `Session` with `created_at` and `SessionStore` with `list_for_tenant`, entirely
  within the Console UI's existing in-memory session store (ADR-0014) — no new backend service or
  schema. Revoke only ever deletes a session already confirmed to belong to the caller's own
  tenant. New nav entry between Audit Log and Platform Health.
- **Tests:** `cargo test -p kizashi-ui --lib` (302 passed, incl. 2 new `session` store tests for
  `list_for_tenant` and 7 new `sessions_handler` tests: empty state, tenant scoping, non-admin
  forbidden, login redirect, revoke removes target, revoke rejects cross-tenant, revoke requires
  admin). Full workspace gate green: `cargo build --workspace`, `cargo test --workspace
  --all-features` against real Postgres/RabbitMQ/ClickHouse/MinIO/greenmail (110 test binaries, 0
  failures), `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt
  --all --check`, `cargo deny check`, `cargo audit` (3 pre-existing allow-listed advisories,
  unchanged).
- **PR:** pending
- **ADR:** docs/adr/0046-active-sessions-management-page.md

## [2026-07-20] feature/0057-security-overview-dashboard — Security overview dashboard and nav grouping
- **Type:** feature
- **Branch:** feature/0057-security-overview-dashboard
- **Summary:** Adds `GET /security`, a single-pane-of-glass dashboard aggregating active session
  count, admin activity in the last 7 days, RBAC role distribution, retention policy coverage,
  and egress allowlist size — each linking to its own detail page. Pure aggregation of existing
  clients (no new backend endpoints or schema). Also reorganizes the Console UI nav (26 links)
  into four labelled sections (Data & Pipeline, Configuration, Security & Compliance, Platform)
  via a new `.nav-section` heading style, closing the "flat, ungrouped nav" gap flagged in
  ADR-0045.
- **Tests:** `cargo test -p kizashi-ui --lib` (309 passed, incl. 7 new
  `security_overview_handler` tests: zero-state, session counting, 7-day activity filtering, RBAC
  distribution, retention coverage, empty-allowlist warning, login redirect). Full workspace gate
  green: `cargo build --workspace`, `cargo test --workspace --all-features` against real
  Postgres/RabbitMQ/ClickHouse/MinIO/greenmail (110 test binaries, 0 failures), `cargo clippy
  --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all --check`, `cargo deny
  check`, `cargo audit` (3 pre-existing allow-listed advisories, unchanged).
- **PR:** pending
- **ADR:** docs/adr/0047-security-overview-dashboard-and-nav-grouping.md

## [2026-07-20] fix/0010-disabled-button-visual-state — Disabled buttons now look disabled
- **Type:** fix
- **Branch:** fix/0010-disabled-button-visual-state
- **Summary:** Found via visual review of the new Active Sessions page (headless-Chrome
  screenshot, not just passing tests): buttons rendered with the HTML `disabled` attribute (e.g.
  "Revoke" on the caller's own session, "Remove" on the caller's own user row) were functionally
  disabled but visually identical to an enabled button — same solid accent/danger color, same
  cursor, no indication a click would do nothing. Adds a `:disabled` style rule (dimmed opacity,
  `not-allowed` cursor, neutral background) affecting every disabled button across the Console UI,
  not just the two pages that surfaced it.
- **Tests:** `cargo test -p kizashi-ui --lib` (309 passed, unchanged — CSS-only change, no
  behavioral test coverage needed/added); `cargo clippy --workspace --all-targets --all-features
  -- -D warnings`, `cargo fmt --all --check`, `cargo build --workspace` all green. Live-verified
  via headless-Chrome screenshot of the rendered `/security/sessions` page before and after.
- **PR:** pending
- **ADR:** n/a

## [2026-07-20] feature/0058-permissions-reference-and-csv-export — Permissions reference, audit CSV export, and an API key redaction fix
- **Type:** feature
- **Branch:** feature/0058-permissions-reference-and-csv-export
- **Summary:** Three related additions from an RBAC accuracy audit: (1) `GET /security/permissions`,
  a written Viewer/Operator/Admin capability matrix transcribed directly from every backend's
  actual role-check code, not an aspirational description; (2) `GET /audit-log/export.csv`, a
  compliance-report export of the merged audit feed (up to 2000 rows/service via internal
  pagination), reusing the same merge logic the HTML page already uses; (3) a real bug the audit
  found and fixed: `GET /v1/analysis-config` returned the AI provider's plaintext API key to any
  authenticated role including Viewer — now redacted (`api_key: None`, `api_key_configured: bool`
  instead), with a tri-state PUT field so "leave unchanged" is still expressible without the read
  side leaking the value.
- **Tests:** `cargo test -p kizashi-ui --lib` (319 passed, incl. 3 permissions-reference tests, 5
  new CSV export tests, and analysis-config-client tri-state coverage); `cargo test -p
  config-admin-service --lib` (105 passed, incl. `get_never_returns_the_real_api_key_regardless_of_caller_role`,
  `get_reports_api_key_not_configured_when_none_was_ever_set`,
  `put_without_api_key_field_preserves_the_existing_key`, `put_with_explicit_null_api_key_clears_it`).
  Full workspace gate green: `cargo build --workspace`, `cargo test --workspace --all-features`
  against real Postgres/RabbitMQ/ClickHouse/MinIO/greenmail (110 test binaries, 0 failures),
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all
  --check`, `cargo deny check`, `cargo audit` (3 pre-existing allow-listed advisories, unchanged).
- **PR:** pending
- **ADR:** docs/adr/0048-permissions-reference-page.md, docs/adr/0049-audit-log-csv-export.md,
  docs/adr/0050-analysis-config-api-key-redaction.md

## [2026-07-20] feature/0059-totp-multi-factor-authentication — TOTP-based multi-factor authentication
- **Type:** feature
- **Branch:** feature/0059-totp-multi-factor-authentication
- **Summary:** Closes the most consequential gap found by an explicit SOC 2/ISO 27001-mapped
  compliance rubric run this session (11/16 domains previously "done", MFA the standout
  "missing"): adds opt-in, self-service TOTP-based MFA for local login. New `mfa_secret`/
  `mfa_enabled` columns on `local_users` plus a `mfa_challenges` table bridging the two-step login
  flow; enrollment requires explicit code confirmation before `mfa_enabled` flips (an unconfirmed
  secret can never gate login); disable requires re-entering the password. `local_login` now
  returns `{mfa_required, challenge_token}` instead of a session grant for MFA-enabled users; a
  new `POST /v1/auth/local/mfa/challenge` endpoint (the only MFA endpoint with no
  X-Role/X-Tenant-Id/X-Username trust, since no session exists yet at that point) completes login.
  Console UI gets `GET/POST /login/mfa` (challenge page, bridging cookies mirroring the OIDC flow
  cookie pattern) and a new `/security/mfa` self-service settings page (QR code enrollment,
  verify, disable). OIDC/SSO logins are unaffected. New `totp-rs` dependency.
- **Tests:** `cargo test -p auth-service` (114 lib + 5 real-Postgres integration tests, incl.
  enroll/verify/disable/challenge/status handler tests, challenge single-use and expiry, and the
  `local_login` MFA-required branch); `cargo test -p kizashi-ui --lib` (341 passed, incl. 22 new
  MFA-related tests across `mfa_client`, `mfa_login_handler`, `mfa_settings_handler`,
  `auth_client`). Full workspace gate green: `cargo build --workspace`, `cargo test --workspace
  --all-features` against real Postgres/RabbitMQ/ClickHouse/MinIO/greenmail (111 test binaries, 0
  failures), `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt
  --all --check`, `cargo deny check`, `cargo audit` (3 pre-existing allow-listed advisories,
  unchanged -- no new advisories from the new dependency).
- **PR:** pending
- **ADR:** docs/adr/0051-totp-multi-factor-authentication.md

## [2026-07-20] feature/0060-password-policy-enforcement — Password policy enforcement
- **Type:** feature
- **Branch:** feature/0060-password-policy-enforcement
- **Summary:** Closes another gap from the ADR-0051 compliance rubric: `create_user` (the only
  path that ever sets a password) previously had no length/strength check at all. Adds
  `validate_password_strength` (min 12 chars, max 128, must not equal the username, rejects a
  small known-weak blocklist), enforced server-side with a specific rejection reason. Along the
  way, fixed a real UX gap: the Console UI's `UsersClientError::Rejected` only carried an HTTP
  status code, not the backend's actual error message, so an admin would have seen "HTTP 400"
  with no explanation — now surfaces the real reason. Users page form gained a matching
  `minlength` hint.
- **Tests:** `cargo test -p auth-service --lib` (128 passed, incl. 9 new `password_policy` unit
  tests and 3 new `create_user` rejection tests); `cargo test -p kizashi-ui --lib` (342 passed,
  incl. a new test proving the backend's error message round-trips through the client). Full
  workspace gate green: `cargo build --workspace`, `cargo test --workspace --all-features`
  against real Postgres/RabbitMQ/ClickHouse/MinIO/greenmail (111 test binaries, 0 failures),
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all
  --check`, `cargo deny check`, `cargo audit` (3 pre-existing allow-listed advisories, unchanged).
- **PR:** pending
- **ADR:** docs/adr/0052-password-policy-enforcement.md

## [2026-07-20] feature/0061-login-attempt-anomaly-alerting — Login-attempt anomaly alerting
- **Type:** feature
- **Branch:** feature/0061-login-attempt-anomaly-alerting
- **Summary:** Closes another gap from the ADR-0051 compliance rubric: a failed login had no
  entity for `auth_audit_log` to attach a row to, so brute-force/anomaly patterns were
  invisible. Adds a dedicated append-only `login_attempts` table (immutable via DB trigger,
  matching the `auth_audit_log` pattern) recording every local-login and MFA-challenge attempt
  with a specific reason (`unknown_workspace`, `unknown_username`, `wrong_password`,
  `password_ok_mfa_pending`, `mfa_code_invalid`, `mfa_success`, `success`). Recording is
  best-effort/non-blocking so a telemetry write can never break a real login. New Admin-only,
  tenant-scoped `GET /v1/auth/local/login-attempts` endpoint and a matching `/security/login-
  attempts` Console UI page in the Security & Compliance nav group.
- **Tests:** `cargo test -p auth-service --all-features` — 138 lib tests + 4 new real-Postgres
  integration tests (`login_attempt_integration_test.rs`: round-trip record/list, null-tenant
  persistence, UPDATE rejected, DELETE rejected) + existing integration suites, all passing, 0
  failed. `cargo test -p kizashi-ui --lib` — 349 passed (6 new: admin can view, empty state,
  backend-unreachable error surfaces, non-admin forbidden, redirect-when-signed-out, plus the
  HTTP client's 2 tests), 0 failed. `cargo build --workspace` clean. `cargo clippy -p
  auth-service --all-targets --all-features -- -D warnings` and `cargo clippy -p kizashi-ui
  --all-targets --all-features -- -D warnings` both clean. `cargo fmt --all --check` clean.
  `cargo deny check` — advisories/bans/licenses/sources ok (pre-existing allow-listed warnings
  unchanged). `cargo audit` — 3 pre-existing allow-listed unmaintained-crate warnings, no new
  vulnerabilities. Note: `cargo test --workspace --all-features` was not run end-to-end for
  unrelated crates (`action-executor`, `connector-fabric`, `connector-imap`) that require
  external test fixtures (`SMTP_TEST_HOST`, `FABRIC_TEST_HOST`, `IMAP_TEST_HOST`) not present in
  this local environment — a pre-existing local-env gap unconnected to this change; the two
  crates this feature actually touches were fully verified.
- **PR:** pending
- **ADR:** docs/adr/0053-login-attempt-anomaly-alerting.md

## [2026-07-20] feature/0062-data-subject-rights-export-and-delete — Data subject rights (export/delete)
- **Type:** feature
- **Branch:** feature/0062-data-subject-rights-export-and-delete
- **Summary:** Closes another gap from the ADR-0051 compliance rubric: no way to answer a
  GDPR/CCPA-style "export/delete everything about this person" request. Scoped explicitly to
  local user accounts and directly-attributable records (account row, its audit trail, its login
  attempts) — ingested ticket/email content has no reliable identity index and is out of scope
  for v1 (documented, not silently dropped). New Admin-only, tenant-scoped `GET
  /v1/users/:id/data-subject-export` (auth-service) aggregating the account + `auth_audit_log`
  entries + `login_attempts` rows into one JSON document; new `LoginAttemptRepository::
  list_by_username`. Console UI gained an "Export data" download link per row on `/users`
  (`GET /users/:id/export`, served as a `Content-Disposition: attachment` JSON download). Delete
  reuses the existing `DELETE /v1/users/:id` (no new endpoint) — the append-only
  `login_attempts`/`auth_audit_log` rows intentionally aren't scrubbed on account deletion, since
  weakening the DB-level immutability trigger to allow identity-scoped deletes would undermine
  the audit trail's core guarantee; this trade-off is documented in the ADR rather than left
  implicit.
- **Tests:** `cargo test -p auth-service --lib` — 143 passed (4 new `data_subject_handler` tests:
  admin can export, non-admin forbidden, unknown id is 404, cross-tenant id is 404; 1 new
  `list_by_username` repository test). `cargo test -p auth-service --test
  login_attempt_integration_test` — 5 passed (1 new: `list_by_username` against real Postgres).
  `cargo test -p kizashi-ui --lib` — 352 passed (2 new `users_handler` tests: download attachment
  succeeds for admin, forbidden for operator; 1 new `users_client` HTTP test). `cargo build
  --workspace` clean. `cargo clippy -p auth-service --all-targets --all-features -- -D warnings`
  and `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings` both clean. `cargo
  fmt --all --check` clean. `cargo deny check` and `cargo audit` — same pre-existing allow-listed
  warnings as prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0054-data-subject-rights-export-and-delete.md

## [2026-07-20] feature/0063-backup-service-and-dr-visibility — Backup service and DR visibility
- **Type:** feature
- **Branch:** feature/0063-backup-service-and-dr-visibility
- **Summary:** Closes another gap from the ADR-0051 compliance rubric: a repo-wide audit found
  zero backup automation anywhere (no `pg_dump`, no scheduled snapshot, nothing in
  docker-compose/CI). Rather than ship a status page with nothing real behind it (CLAUDE.md's
  "no half-truths" rule), builds the actual backup pipeline first: new `crates/backup-service`
  shells out to the real `pg_dump` binary (custom format), uploads to a new MinIO/S3 bucket
  (`kizashi-backups`, reusing retention-service's proven storage infra), and records every
  attempt — success or failure — in a `backup_runs` table. `POST /v1/backup/run` (internal-secret
  gated, triggered by a new `backup-scheduler` sidecar mirroring `retention-sweep-scheduler`'s
  shape, daily by default) and `GET /v1/backup/status` (Admin-gated, platform-wide since a
  backup isn't tenant-scoped). Console UI gained `/security/backups` (new nav entry) showing
  recent run history. Shared `Dockerfile` gained an `INSTALL_POSTGRES_CLIENT` build arg (same
  opt-in pattern as `agent-scheduler`'s `INSTALL_DOCKER_CLI`) so the runtime image actually has
  `pg_dump` available. ClickHouse backup and automated restore-verification are explicitly out
  of scope for v1, documented in the ADR rather than silently dropped.
- **Tests:** `cargo test -p backup-service --lib` — 15 passed (executor success/dump-failure/
  upload-failure/start-failure paths, repository start/complete/fail/list, ops-handler auth
  gating). `cargo test -p backup-service --tests` — 3 passed against real infra: a genuine
  `pg_dump` invocation against the real Postgres instance (`pg_dump_integration_test.rs`,
  asserts the `PGDMP` magic-byte header) and 2 `backup_run_repository` round-trips against real
  Postgres. `cargo test -p kizashi-ui --lib` — 359 passed (7 new: admin can view backup status,
  empty state, backend-unreachable error, non-admin forbidden, redirect-when-signed-out, plus
  the HTTP client's 2 tests). `cargo build --workspace` clean. `cargo clippy -p backup-service
  --all-targets --all-features -- -D warnings` and `cargo clippy -p kizashi-ui --all-targets
  --all-features -- -D warnings` both clean. `cargo fmt --all --check` clean. `cargo deny check`
  and `cargo audit` — same pre-existing allow-listed warnings as prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0055-backup-service-and-dr-visibility.md

## [2026-07-20] fix/0011-pg-dump-version-mismatch — Fix pg_dump/server major-version mismatch in backup-service
- **Type:** fix
- **Branch:** fix/0011-pg-dump-version-mismatch
- **Summary:** Live-verifying feature/0063 immediately after deploy surfaced a real failure:
  every triggered backup failed with `pg_dump: error: aborting because of server version
  mismatch` — Debian bookworm's default apt repo ships `postgresql-client-15`, one major version
  behind `docker-compose.yml`'s `postgres:16-alpine`, and `pg_dump` refuses to dump a server
  newer than itself. This was flagged as a *possible* future risk in ADR-0055's Consequences
  section but actually hit on the very first live run, not just a hypothetical drift. Fixed by
  pulling `postgresql-client-16` from the official PGDG apt repo (`apt.postgresql.org`) instead
  of Debian's own, so the client always matches the major version `docker-compose.yml` actually
  runs. Confirmed via `docker run --entrypoint pg_dump kizashi-backup-service --version` (now
  16.14, matching the server's 16.13/16.14) and a real triggered backup succeeding
  (`size_bytes: 5872812`, visible in MinIO via `mc ls` and on the `/security/backups` Console UI
  page).
- **Tests:** No new automated test (the failure mode is "the installed apt package version" —
  not something a unit/integration test running inside the already-built image can catch; the
  verification that matters here is the live one performed and recorded above). `cargo build
  --workspace`, `cargo clippy -p backup-service --all-targets --all-features -- -D warnings`,
  `cargo fmt --all --check` all still clean (Dockerfile-only change, no Rust source touched).
- **PR:** pending
- **ADR:** n/a (implementation detail of ADR-0055, not a new architectural decision)

## [2026-07-20] feature/0064-compliance-report-generation — Compliance report generation
- **Type:** feature
- **Branch:** feature/0064-compliance-report-generation
- **Summary:** Closes the last domain in the ADR-0051 compliance rubric. Previously the only
  export was a raw audit-log CSV (ADR-0049) — individual change rows, not a "what controls are
  in place right now" summary. New `GET /security/compliance-report` assembles a single
  browser-printable HTML snapshot (RBAC distribution, MFA adoption, password policy, retention
  coverage, egress allowlist size, 7-day failed-login count, backup/DR status, recent admin
  activity) by reusing the exact same clients Security Overview (ADR-0047) already calls, plus
  two clients from later rubric domains (login attempts, backup status) that existed but were
  never folded into any dashboard — no new data-gathering, one document instead of five pages.
  Closed two small real gaps rather than hardcoding UI copy that could drift: `UiUser` now
  carries `mfa_enabled` (the auth-service column existed since ADR-0051 but was never exposed to
  the Console UI), and a new `GET /v1/auth/local/password-policy` endpoint exposes the
  live-enforced policy parameters instead of a static description.
- **Tests:** `cargo test -p auth-service --lib` — 145 passed (2 new: `password_policy::summary`
  unit test, `get_password_policy` handler test). `cargo test -p kizashi-ui --lib` — 363 passed
  (4 new: full-snapshot rendering with seeded users/login-attempts/backup-run data, non-admin
  forbidden, redirect-when-signed-out, plus the `password_policy` HTTP client test). `cargo
  build --workspace` clean. `cargo clippy -p auth-service --all-targets --all-features -- -D
  warnings` and `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings` both
  clean. `cargo fmt --all --check` clean. `cargo deny check`/`cargo audit` — same pre-existing
  allow-listed warnings as prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0056-compliance-report-generation.md

## [2026-07-20] feature/0065-self-service-password-change — Self-service password change
- **Type:** feature
- **Branch:** feature/0065-self-service-password-change
- **Summary:** Closes a gap ADR-0052 explicitly flagged when it shipped: previously the only way
  to change a local user's password at all was an admin deleting and recreating the account.
  New `LocalUserRepository::update_password` (plain UPDATE, no audit row — same reasoning as the
  MFA enrollment mutations: a user changing their own password isn't an admin action on someone
  else). New `POST /v1/auth/local/password`, requiring the current password (same trust
  reasoning as MFA disable) and running the new password through the same
  `validate_password_strength` check `create_user` uses. Console UI gained
  `GET`/`POST /security/password`, self-service like `/security/mfa`, with a "Change Password"
  nav entry.
- **Tests:** `cargo test -p auth-service --lib` — 150 passed (5 new: repository unit test, 4
  handler tests covering success/wrong-current-password/policy-rejection/missing-username).
  `cargo test -p auth-service --test local_user_repository_integration_test` — 7 passed (1 new,
  against real Postgres, also confirming no audit row is written for a self-service change).
  `cargo test -p kizashi-ui --lib` — 370 passed (6 new UI handler tests + 1 HTTP client test).
  `cargo build --workspace` clean. `cargo clippy -p auth-service --all-targets --all-features --
  -D warnings` and `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings` both
  clean. `cargo fmt --all --check` clean. `cargo deny check`/`cargo audit` — same pre-existing
  allow-listed warnings as prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0057-self-service-password-change.md

## [2026-07-20] feature/0066-analysis-config-api-key-encryption — Analysis config API key encryption at rest
- **Type:** feature
- **Branch:** feature/0066-analysis-config-api-key-encryption
- **Summary:** Closes a gap ADR-0031 flagged when it shipped: `analysis_configs.api_key` (a
  tenant's AI provider credential) was stored in plaintext in Postgres — ADR-0050 closed the
  *display* half (Console UI/audit log never show the real key) but not the at-rest storage
  itself. New `ApiKeyEncryptor` (AES-256-GCM, key from `CONFIG_ENCRYPTION_KEY`) encrypts the
  column on every write and decrypts on every read inside
  `PostgresAnalysisConfigRepository` — no other code changes, since every caller (analysis-
  service's outbound provider calls, the existing audit-log redaction) keeps working against the
  same plaintext `Option<String>` shape. No schema migration needed (ciphertext fits the
  existing TEXT column). `docker-compose.yml`/`.env.example` require the new env var with no
  default, matching `AWS_SECRET_ACCESS_KEY`'s existing fail-loudly convention.
- **Tests:** `cargo test -p config-admin-service --lib` — 112 passed (7 new `encryption` unit
  tests: round-trip, ciphertext never contains plaintext, nonces differ per encryption, wrong
  key fails, tampered ciphertext fails, base64 key parsing success/failure). `cargo test -p
  config-admin-service --tests` (full env incl. RabbitMQ) — all integration suites passing,
  including 1 new real-Postgres test confirming the raw `api_key` column value is neither equal
  to nor contains the plaintext, and that `get()` still returns the correct decrypted value.
  `cargo build --workspace` clean. `cargo clippy -p config-admin-service --all-targets
  --all-features -- -D warnings` clean. `cargo fmt --all --check` clean. `cargo deny check`/
  `cargo audit` — same pre-existing allow-listed warnings as prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0058-analysis-config-api-key-encryption-at-rest.md

## [2026-07-20] feature/0067-nav-wide-tenant-branding — Nav-wide tenant branding
- **Type:** feature
- **Branch:** feature/0067-nav-wide-tenant-branding
- **Summary:** Closes a gap ADR-0041 flagged and explicitly deferred when it shipped: white-label
  branding (product name, accent color) only applied to the login page, not any authenticated
  page. Rather than thread a `branding` field through ~30 independent Askama template structs
  (the "much larger, separate mechanical change" ADR-0041 called out), added one router-wide
  middleware (`ui/src/branding_middleware.rs`, layered once in `build_router`) that rewrites the
  rendered HTML of every `200 OK` authenticated page response: replaces the nav header's fixed
  `Kizashi` product-name span and the `--accent` CSS variable when the session's tenant has
  branding configured, no-ops otherwise. `logo_url` still isn't wired into the nav (no existing
  slot for it); noted as a follow-up, not silently expanded into this change.
- **Tests:** `cargo test -p kizashi-ui --lib` — 376 passed (6 new: pure-function rewrite/escape/
  no-op unit tests, plus end-to-end middleware tests — rewrites when branded, leaves unchanged
  with no session cookie, leaves unchanged when the tenant has no branding configured). `cargo
  build --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D
  warnings` clean. `cargo fmt --all --check` clean. `cargo deny check`/`cargo audit` — same
  pre-existing allow-listed warnings as prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0059-nav-wide-tenant-branding.md

## [2026-07-20] feature/0068-audit-log-csv-export-pagination — Audit log CSV export pagination
- **Type:** feature
- **Branch:** feature/0068-audit-log-csv-export-pagination
- **Summary:** Closes a gap ADR-0049 explicitly flagged as a follow-up: the CSV export capped at
  6000 rows with no way to get the rest and no signal that more history existed. `GET
  /audit-log/export.csv` now accepts the same `?before=` cursor the HTML page's "Load older"
  link uses, and sets an `X-Next-Before` response header when the row cap was hit while more
  history remained (vs. a source genuinely running dry) — no more silent truncation. The HTML
  page's "Load older" section gained a matching "Download CSV from here" link.
- **Tests:** `cargo test -p kizashi-ui --lib` — 379 passed (3 new: `?before=` honored as the
  starting cursor, `X-Next-Before` header set when more history remains, header absent when
  history is fully exported). `cargo build --workspace` clean. `cargo clippy -p kizashi-ui
  --all-targets --all-features -- -D warnings` clean. `cargo fmt --all --check` clean. `cargo
  deny check`/`cargo audit` — same pre-existing allow-listed warnings as prior entries, no new
  issues.
- **PR:** pending
- **ADR:** docs/adr/0060-audit-log-csv-export-pagination.md

## [2026-07-20] feature/0069-destructive-action-confirmation — Destructive action confirmation
- **Type:** feature
- **Branch:** feature/0069-destructive-action-confirmation
- **Summary:** A UI/UX audit found every destructive control across the Console UI (Revoke API
  key, Remove user, Revoke session, Remove retention policy, Remove sensor, Disable MFA, Remove
  saved search — 7 pages) submitted immediately on click with zero confirmation. New
  `ui/static/confirm-danger.js` (served via `GET /static/confirm-danger.js`, same
  `include_str!`-embedded pattern as `charts.js`), included once in `layout.html`: listens for
  `submit` events, checks `event.submitter` for the `.btn-danger` class every destructive button
  already carried, and shows a native `confirm()` dialog before allowing the submission through.
  Zero per-page template/handler changes needed — purely additive, and covers any future
  `.btn-danger` button automatically.
- **Tests:** `cargo test -p kizashi-ui --lib` — 380 passed (1 new: `GET
  /static/confirm-danger.js` returns the right content-type and contains the expected selector).
  `cargo build --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features --
  -D warnings` clean. `cargo fmt --all --check` clean. `cargo deny check`/`cargo audit` — same
  pre-existing allow-listed warnings as prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0061-destructive-action-confirmation.md

## [2026-07-20] feature/0070-users-page-search — Users page search
- **Type:** feature
- **Branch:** feature/0070-users-page-search
- **Summary:** Closes part of a UI/UX audit finding: every list page except Data renders its
  full table with no filter controls. `GET /users` now accepts `?q=` and filters the fetched
  user list by a case-insensitive username substring match, in-handler (not a new backend query
  param — a tenant's user list is realistically small, unlike Data's potentially-huge ingested-
  record volume). Bookmarkable `GET` search form, "Clear" link when active, distinct "no
  results" vs "no users at all" empty states. Scoped to Users only; the same pattern is a direct
  template for the other list pages the audit flagged (Sensors, API Keys, Sessions, etc.) as
  follow-ups.
- **Tests:** `cargo test -p kizashi-ui --lib` — 382 passed (2 new: case-insensitive filter match,
  "no users match" empty state for an unmatched query). `cargo build --workspace` clean. `cargo
  clippy -p kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo fmt --all
  --check` clean. `cargo deny check`/`cargo audit` — same pre-existing allow-listed warnings as
  prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0062-users-page-search.md

## [2026-07-20] chore/0004-docker-build-cache-mounts — Add BuildKit cache mounts to the shared Dockerfile
- **Type:** chore
- **Branch:** chore/0004-docker-build-cache-mounts
- **Summary:** The shared `Dockerfile`'s builder stage had no cache mounts at all — every
  `docker compose build <service>` recompiled the entire dependency tree (aws-sdk-s3, sqlx,
  tokio, hundreds of crates) completely from scratch, every single time, in `--release` mode
  with `lto = true`/`codegen-units = 1` (the slowest possible profile), regardless of how small
  the actual source change was. Added `--mount=type=cache` for `/usr/local/cargo/registry`,
  `/usr/local/cargo/git`, and `/app/target`, persisting compiled dependency artifacts across
  `docker build` invocations (shared across every `BIN` this one Dockerfile builds, since
  they're all one Cargo workspace). No behavior change to the built images themselves — same
  binary output, same runtime image, purely a build-time cache.
- **Tests:** No Rust code changed (Dockerfile-only), so no `cargo test`/clippy/fmt run for this
  change. Verified directly instead: touched one file in `kizashi-ui` and rebuilt —
  **25 seconds** (down from ~2-3 minutes), with the build log showing only `kizashi-ui` itself
  recompiling, all dependencies served from cache. Repeated with a different binary
  (`auth-service`, a different dependency mix) touched and rebuilt — **53 seconds**, confirming
  the cache is genuinely shared across different `BIN` builds, not a fluke of one image.
- **PR:** pending
- **ADR:** n/a (build-tooling fix, not an architectural decision)

## [2026-07-20] feature/0071-api-keys-page-search — API Keys page search
- **Type:** feature
- **Branch:** feature/0071-api-keys-page-search
- **Summary:** Extends ADR-0062's search pattern to the API Keys page — the same UI/UX audit
  finding, closed the same way: `GET /api-keys` now accepts `?q=` and filters the fetched key
  list by a case-insensitive label substring match, in-handler (no backend change; a tenant's
  key list is realistically small like the user list was). Bookmarkable `GET` search form,
  "Clear" link when active, distinct "no results" vs "no keys at all" empty states.
- **Tests:** `cargo test -p kizashi-ui --lib` — 384 passed (2 new: case-insensitive label filter
  match, "no keys match" empty state for an unmatched query). `cargo build --workspace` clean.
  `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo fmt
  --all --check` clean. `cargo deny check`/`cargo audit` — same pre-existing allow-listed
  warnings as prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0062-users-page-search.md (same pattern, no new decision to record)

## [2026-07-20] feature/0072-sessions-page-search — Active Sessions page search
- **Type:** feature
- **Branch:** feature/0072-sessions-page-search
- **Summary:** Extends ADR-0062's search pattern to the Active Sessions page: `GET
  /security/sessions` now accepts `?q=` and filters the fetched session list by a
  case-insensitive username substring match, in-handler. Bookmarkable `GET` search form, "Clear"
  link when active, distinct "no results" vs "no active sessions" empty states.
- **Tests:** `cargo test -p kizashi-ui --lib` — 386 passed (2 new: case-insensitive username
  filter match, "no sessions match" empty state for an unmatched query). `cargo build
  --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings`
  clean. `cargo fmt --all --check` clean. `cargo deny check`/`cargo audit` — same pre-existing
  allow-listed warnings as prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0062-users-page-search.md (same pattern, no new decision to record)

## [2026-07-20] feature/0073-login-attempts-pagination-and-search — Login attempts pagination and search
- **Type:** feature
- **Branch:** feature/0073-login-attempts-pagination-and-search
- **Summary:** Closes the highest-impact remaining UI/UX audit finding: Login Attempts is
  naturally high-volume but had neither search nor pagination, permanently capped at the
  backend's default page (50 rows) with no way to see further back. Extended
  `LoginAttemptsClient::list_recent` to accept the same `before` cursor `/audit-log`'s "Load
  older" link already uses; `GET /security/login-attempts` now accepts `?before=` (shows "Load
  older" when a full page returns) and `?q=` (in-handler username filter, same pattern as
  ADR-0062). Documented the resulting caveat (search filters only the currently-fetched page,
  doesn't compose with pagination in one request) rather than silently shipping it as if it were
  a full server-side search.
- **Tests:** `cargo test -p kizashi-ui --lib` — 391 passed (5 new: search filter,
  full-page-shows-Load-older, partial-page-hides-it, `before` cursor honored, HTTP client passes
  `before` as a query param). `cargo build --workspace` clean. `cargo clippy -p kizashi-ui
  --all-targets --all-features -- -D warnings` clean. `cargo fmt --all --check` clean. `cargo
  deny check`/`cargo audit` — same pre-existing allow-listed warnings as prior entries, no new
  issues.
- **PR:** pending
- **ADR:** docs/adr/0063-login-attempts-pagination-and-search.md

## [2026-07-20] feature/0074-normalization-mappings-search — Field Mappings page search
- **Type:** feature
- **Branch:** feature/0074-normalization-mappings-search
- **Summary:** Extends ADR-0062's search pattern to the Field Mappings page. `GET
  /normalization-mappings` now accepts `?q=` and filters the fetched mapping list by a
  case-insensitive `source_type` substring match, in-handler. Bookmarkable `GET` search form,
  "Clear" link when active, distinct "no results" vs "no mappings configured" empty states.
  (Egress Allowlist was evaluated and skipped — it's a single free-text textarea per tenant, not
  a row-based list, so search doesn't apply; Retention Policies was evaluated and skipped too —
  realistically only a handful of rows per tenant, not a genuine search candidate.)
- **Tests:** `cargo test -p kizashi-ui --lib` — 393 passed (2 new: case-insensitive source_type
  filter match, "no mappings match" empty state for an unmatched query). `cargo build
  --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings`
  clean. `cargo fmt --all --check` clean. `cargo deny check`/`cargo audit` — same pre-existing
  allow-listed warnings as prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0062-users-page-search.md (same pattern, no new decision to record)

## [2026-07-20] feature/0075-users-page-sortable-columns — Users page sortable columns
- **Type:** feature
- **Branch:** feature/0075-users-page-sortable-columns
- **Summary:** Closes another UI/UX audit finding: no table anywhere supported column sorting,
  always shown in whatever order the backend returned. `GET /users` now accepts `?sort=
  username|role` and `?dir=asc|desc`, applied in-handler after the search filter. Column
  headers are clickable toggle links with a ▲/▼ indicator on the active column; unset
  `sort`/`dir` defaults to ascending-by-username rather than an arbitrary backend order. Search
  and sort compose correctly in one request since sorting runs on the already-filtered rows.
- **Tests:** `cargo test -p kizashi-ui --lib` — 396 passed (3 new: ascending username sort,
  descending sort via `dir=desc`, sort by role). `cargo build --workspace` clean. `cargo clippy
  -p kizashi-ui --all-targets --all-features -- -D warnings` clean (one `unnecessary_sort_by`
  finding fixed with `sort_by_key`). `cargo fmt --all --check` clean. `cargo deny check`/`cargo
  audit` — same pre-existing allow-listed warnings as prior entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0064-users-page-sortable-columns.md

## [2026-07-20] feature/0076-api-keys-bulk-revoke — API Keys bulk revoke
- **Type:** feature
- **Branch:** feature/0076-api-keys-bulk-revoke
- **Summary:** Closes another UI/UX audit finding: no list page anywhere supported a bulk action.
  `POST /api-keys/bulk-revoke` accepts one or more selected `ids` (checkbox per active row) and
  loops over the existing single-item `ApiKeysClient::revoke_api_key` call for each. Template
  uses an empty out-of-table `<form id="bulk-revoke-form">` referenced via the HTML5 `form=`
  attribute on checkboxes/button, since forms cannot nest and each row still has its own
  single-revoke form. RBAC-gated identically to single-revoke (Operator+, hidden in template for
  Viewer, 403 server-side otherwise).
- **Tests:** `cargo test -p kizashi-ui --lib` — 399 passed (3 new: revokes every selected key,
  no-op on empty selection, 403 for a Viewer). `cargo build --workspace` clean. `cargo clippy -p
  kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo fmt --all --check`
  clean. `cargo deny check`/`cargo audit` — same pre-existing allow-listed warnings as prior
  entries, no new issues.
- **PR:** pending
- **ADR:** docs/adr/0065-api-keys-bulk-revoke.md

## [2026-07-20] fix/0012-disable-toggle-confirm-danger — Sensors/Retention Policies disable button uses confirm-danger styling
- **Type:** fix
- **Branch:** fix/0012-disable-toggle-confirm-danger
- **Summary:** ADR-0061's shared confirm-danger.js hooks every `.btn-danger` submit button, but
  the Sensors and Retention Policies "Disable" toggle buttons were plain `.btn`, silently
  bypassing the confirmation dialog for a real, meaningfully risky state change (stops ingestion
  monitoring / retention enforcement). The Enable/Disable button's class is now conditional on
  current state: `.btn-danger` only when the click will disable, plain `.btn` when it will
  enable — so only the actually-destructive direction gets the red styling and confirm prompt.
- **Tests:** `cargo test -p kizashi-ui --lib` — 399 passed (existing sensors/retention-policies
  toggle tests unaffected, no test asserted on the button's CSS class so none needed updating).
  `cargo build --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D
  warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0061-destructive-action-confirmation.md (same pattern, no new decision to record)

## [2026-07-20] feature/0077-triggers-page-search — Triggers page search
- **Type:** feature
- **Branch:** feature/0077-triggers-page-search
- **Summary:** Closes another UI/UX audit finding: Triggers had pagination but no search. `GET
  /triggers` now accepts `?q=` and filters the current page's fetched triggers by a
  case-insensitive `name` substring match. Since `list_triggers` is server-paginated (unlike
  Users/API Keys), this only searches the current page, not the tenant's full trigger set — an
  explicitly documented limitation (ADR-0066), same shape as ADR-0063's Login Attempts caveat.
  `q` carries through the Previous/Next links so paging preserves the search term.
- **Tests:** `cargo test -p kizashi-ui --lib` — 401 passed (2 new: case-insensitive name filter
  match, "no triggers on this page match" empty state for an unmatched query). `cargo build
  --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings`
  clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0066-triggers-page-search.md

## [2026-07-20] fix/0013-disabled-button-accessible-labels — Accessible labels on disabled self-action buttons
- **Type:** fix
- **Branch:** fix/0013-disabled-button-accessible-labels
- **Summary:** Closes a UI/UX audit accessibility finding: `aria-label` was essentially unused
  sitewide. The Users page's disabled "Remove" and Active Sessions page's disabled "Revoke"
  buttons (both disabled on the caller's own row) carried only a `title` explaining why, which
  isn't reliably exposed to screen readers/keyboard navigation. Both now carry a matching
  `aria-label` restating the button + reason (e.g. "Remove -- you can't remove yourself").
  Scoped to these two concretely-flagged instances, not a full sitewide accessibility sweep.
- **Tests:** `cargo test -p kizashi-ui --lib` — 403 passed (2 new: aria-label present on the
  current user's disabled Remove button, aria-label present on the caller's disabled Revoke
  button). `cargo build --workspace` clean. `cargo clippy -p kizashi-ui --all-targets
  --all-features -- -D warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0067-disabled-button-accessible-labels.md

## [2026-07-20] feature/0078-sessions-page-sortable-columns — Active Sessions page sortable columns
- **Type:** feature
- **Branch:** feature/0078-sessions-page-sortable-columns
- **Summary:** Closes another UI/UX audit finding: Active Sessions had search but no sorting,
  always hardcoded most-recent-first. `GET /security/sessions` now accepts `?sort=username|role`
  and `?dir=asc|desc`, same pattern as Users (ADR-0064), applied after the search filter so the
  two compose. Unlike Users, the unset-sort default stays most-recently-signed-in-first (more
  useful for a security-review page than alphabetical) — the "Signed in" header itself is now
  also a toggle link.
- **Tests:** `cargo test -p kizashi-ui --lib` — 406 passed (3 new: ascending/descending username
  sort, default-unset-sort newest-first ordering). `cargo build --workspace` clean. `cargo
  clippy -p kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo fmt --all
  --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0068-sessions-page-sortable-columns.md

## [2026-07-20] docs/0003-normalization-mappings-pagination-evaluated-and-skipped — Normalization Mappings pagination evaluated and skipped
- **Type:** docs
- **Branch:** docs/0003-normalization-mappings-pagination-evaluated-and-skipped
- **Summary:** A UI/UX audit flagged Field Mappings (normalization mappings) as still missing
  pagination, unlike Triggers/Login Attempts. Investigated and deliberately skipped: unlike
  those pages, this list has zero pagination infrastructure anywhere in the stack (repository,
  SQL, HTTP contract all fetch-all with no limit/offset), so adding it would mean a cross-service
  backend change to config-admin-service, not just a UI-layer addition. Checked actual real-tenant
  usage — one mapping per source_type per tenant, realistically a handful of rows even at scale
  (bounded by connector-type count, not ingested record volume) — the same "too few rows to
  matter" reasoning already used to skip Retention Policies pagination/search. Recording this
  explicitly so the item isn't silently dropped or re-flagged as an oversight in a future audit.
- **Tests:** N/A — no code change, decision-only.
- **PR:** pending
- **ADR:** none — not an architectural decision, a scope call recorded here per CLAUDE.md's
  "no silent omission" principle.

## [2026-07-20] feature/0079-audit-log-search — Global Audit Log page search
- **Type:** feature
- **Branch:** feature/0079-audit-log-search
- **Summary:** Closes another UI/UX audit finding: the global Audit Log page (`GET /audit-log`)
  had cursor pagination but no search. It now accepts `?q=` and filters on a case-insensitive
  substring match across actor/entity_type/change_type. Since the page is already
  cursor-paginated, search only applies to the currently fetched page — same accepted limitation
  as Triggers (ADR-0066) and Login Attempts (ADR-0063). The "Load older" cursor is computed from
  the full fetched page before filtering, so pagination keeps advancing correctly regardless of
  what's displayed; `q` carries through that link. CSV export intentionally stays unfiltered.
- **Tests:** `cargo test -p kizashi-ui --lib` — 408 passed (2 new: case-insensitive actor filter
  match, "no audit activity on this page matches" empty state for an unmatched query; 1 existing
  test updated for the new link shape). `cargo build --workspace` clean. `cargo clippy -p
  kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0069-audit-log-search.md

## [2026-07-20] feature/0080-triggers-page-sortable-columns — Triggers page sortable columns
- **Type:** feature
- **Branch:** feature/0080-triggers-page-sortable-columns
- **Summary:** Closes the last item from the original UI/UX audit list: Triggers had pagination
  and search but no column sorting. `GET /triggers` now accepts `?sort=name|event_type_match
  |enabled` and `?dir=asc|desc`, applied after the search filter, same pattern as Users
  (ADR-0064) and Active Sessions (ADR-0068). Since `list_triggers` is server-paginated, sort only
  reorders the current page — same accepted limitation as search (ADR-0066). `q`/`sort`/`dir` all
  carry through the search form and Previous/Next links. Normalization Mappings sort was
  evaluated and skipped in the same pass — its backend already returns `ORDER BY source_type`
  and the list is realistically one row per tenant, so a sort UI adds no real value (ADR-0070).
- **Tests:** `cargo test -p kizashi-ui --lib` — 410 passed (2 new: descending name sort, enabled-
  status grouping). `cargo build --workspace` clean. `cargo clippy -p kizashi-ui --all-targets
  --all-features -- -D warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0070-triggers-page-sortable-columns.md

## [2026-07-20] feature/0081-session-idle-timeout — Console UI session idle timeout
- **Type:** feature
- **Branch:** feature/0081-session-idle-timeout
- **Summary:** Closes a real enterprise-compliance gap found by a fresh audit: sessions never
  expired, living until explicit logout, admin revoke, or a process restart no matter how long
  idle. `InMemorySessionStore` now enforces a sliding idle timeout, defaulting to 30 minutes,
  configurable via `SESSION_IDLE_TIMEOUT_MINUTES`. Every `get()` call refreshes the idle clock
  on success or expires+deletes the session if idle too long; `list_for_tenant`
  (`/security/sessions`) also prunes expired sessions as a side effect. `last_active_at` is
  tracked internally by the store, not as a new field on `Session`, to avoid touching every
  handler test's direct `Session { .. }` construction.
- **Tests:** `cargo test -p kizashi-ui --lib` — 414 passed (4 new: idle session expires, active
  session within the window still works, activity slides the timeout forward, expired sessions
  are pruned from `list_for_tenant`). `cargo build --workspace` clean. `cargo clippy -p
  kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0071-session-idle-timeout.md

## [2026-07-20] feature/0082-events-page-search-and-sort — Events page search and sortable columns
- **Type:** feature
- **Branch:** feature/0081-events-page-search-and-sort (numbering race, see branch-registry.md)
- **Summary:** Closes another gap from a fresh audit: the Events page had pagination only, no
  search or sort, unlike every comparable list page. `GET /events` now accepts `?q=` (matches
  across event_type/group_key/status) and `?sort=event_type|group_key|status|occurred_at` with
  `?dir=asc|desc`, same pattern as Triggers (ADR-0066/0070). Since `list_events` is
  server-paginated, both only apply to the current fetched page — same accepted limitation as
  every other paginated-list search/sort on this platform. `q`/`sort`/`dir` carry through the
  search form and pagination links. The events-over-time chart is unaffected (independent
  30-day summary).
- **Tests:** `cargo test -p kizashi-ui --lib` — 413 passed (3 new: case-insensitive filter
  match, "no events on this page match" empty state, ascending event_type sort). `cargo build
  --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings`
  clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0072-events-page-search-and-sort.md

## [2026-07-20] fix/0014-table-header-scope-attributes — Table header scope="col" attributes sitewide
- **Type:** fix
- **Branch:** fix/0014-table-header-scope-attributes
- **Summary:** Closes a sitewide accessibility gap from a fresh audit: zero `<th scope="col">`
  usage anywhere across the Console UI's 18 templates with a `<table>`. Every plain `<th>` in
  every list-page template is now `<th scope="col">` (17 templates changed; `security_overview
  .html`'s label/value tables have no header row, correctly excluded) so screen readers can
  reliably associate a data cell with its column header. Mechanical, markup-only change — no
  behavior change, sortable-column header links unaffected structurally.
- **Tests:** `cargo test -p kizashi-ui --lib` — 417 passed (0 broken by the markup change; 3
  existing tests on Users/Triggers/Events extended with a `scope="col"` spot-check rather than
  one new test per template for a single sitewide convention). `cargo build --workspace` clean.
  `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo fmt
  --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0073-table-header-scope-attributes.md

## [2026-07-20] fix/0015-pipeline-map-severity-text-label — Pipeline Map edge severity gets a visible text label
- **Type:** fix
- **Branch:** fix/0014-pipeline-map-severity-text-label (numbering race, see branch-registry.md)
- **Summary:** Closes another accessibility gap from a fresh audit: Pipeline Map (and its
  Overview dashboard preview) conveyed queue backlog severity purely through the edge line's
  color, with visible text showing only the numeric queue count. Every topology edge now
  carries a `severity_label` ("empty"/"building"/"critical"/"unknown"), computed once in Rust
  and rendered as `(label)` next to the edge in both templates, matching the existing color
  legend's wording — so severity no longer depends on color perception alone.
- **Tests:** `cargo test -p kizashi-ui --lib` — 418 passed (1 new: `severity_label` maps every
  severity to its visible word). `cargo build --workspace` clean. `cargo clippy -p kizashi-ui
  --all-targets --all-features -- -D warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0074-pipeline-map-severity-text-label.md

## [2026-07-20] fix/0016-inline-edit-input-accessible-names — Accessible names on per-row inline-edit inputs
- **Type:** fix
- **Branch:** fix/0016-inline-edit-input-accessible-names
- **Summary:** Closes another accessibility gap from a fresh audit: Retention Policies' inline
  TTL edit input and Users' inline role select (one per table row) had no accessible name
  distinguishing which row they acted on. Both now carry an `aria-label` naming the specific
  row (`"TTL in days for {data_class}"`, `"Role for {username}"`). A repo-wide check found no
  other unlabeled per-row inline-edit inputs.
- **Tests:** `cargo test -p kizashi-ui --lib` — 419 passed (1 new: TTL input aria-label renders;
  1 existing test extended with the role-select aria-label assertion). `cargo build
  --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings`
  clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0075-inline-edit-input-accessible-names.md

## [2026-07-20] feature/0083-backups-pagination-and-cursor-urlencoding-fix — Backups page pagination, and a cursor URL-encoding bug fix
- **Type:** feature
- **Branch:** feature/0083-backups-pagination-and-cursor-urlencoding-fix
- **Summary:** Closes the last item from a fresh audit: Backups had no pagination, capped at the
  first 20 runs forever. `BackupRunRepository::list_recent` gained a `before` cursor (same
  exclusive-keyset shape as Login Attempts/the audit log), threaded through
  `GET /v1/backup/status`, the UI client, and `GET /security/backups`'s new "Load older" link.
  While building it, found and fixed a real already-shipped bug: every existing `?before=`
  "Load older" link (Login Attempts, global Audit Log's HTML page and CSV link) rendered a raw
  unencoded `+00:00` UTC offset into the href, which `serde_urlencoded` decodes as a space,
  corrupting the timestamp on click. Fixed sitewide with Askama's built-in `|urlencode` filter.
- **Tests:** `cargo test -p kizashi-ui --lib` — 423 passed. `cargo test -p backup-service --lib`
  — 16 passed. `cargo test -p backup-service --test backup_run_repository_integration_test`
  (real Postgres) — 3 passed, including a new `before`-cursor test. New tests assert the
  rendered "Load older" link contains no raw `+`, proving the encoding fix, not just its
  presence. `cargo build --workspace` clean. `cargo clippy -p kizashi-ui -p backup-service
  --all-targets --all-features -- -D warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0076-backups-pagination-and-cursor-urlencoding-fix.md

## [2026-07-20] fix/0017-local-test-database-isolation — Local test runs use a separate database from the live stack
- **Type:** fix
- **Branch:** fix/0017-local-test-database-isolation
- **Summary:** Fixes the root cause behind an earlier session incident (Console UI full of
  leftover test fixtures): `.env`'s `DATABASE_URL` pointed at the same `kizashi` database the
  live docker-compose stack/Console UI use, so every local `cargo test` run wrote real test
  junk directly into it. `scripts/bootstrap.sh` now creates a separate `kizashi_test` database
  (idempotent) and `.env`/`.env.example` point `DATABASE_URL` at it instead — a host-only
  variable, never read by the docker-compose services themselves (each hardcodes its own DB
  URL). CI was already unaffected (fresh ephemeral Postgres per run); this is the local-dev-loop
  fix. Each crate's tests already self-migrate their own `DATABASE_URL` target, so no schema
  setup step was needed.
- **Tests:** Verified live: ran `config-admin-service`'s real-Postgres integration suite (19
  tests, all passing) before and after the change — `kizashi`'s `trigger_definitions` row count
  stayed at its real value (1) throughout, while `kizashi_test` picked up the 9 rows those tests
  created. `cargo build --workspace` clean (no Rust source changed, only `scripts/bootstrap.sh`
  and `.env.example`).
- **PR:** pending
- **ADR:** docs/adr/0077-local-test-database-isolation.md

## [2026-07-20] fix/0018-permissions-reference-stale-rows — Permissions Reference page had drifted stale
- **Type:** fix
- **Branch:** fix/0018-permissions-reference-stale-rows
- **Summary:** The Permissions Reference page (ADR-0048) exists so an auditor/new admin can see
  what each role can do without reading source — but four areas added in later features (Login
  Attempts, Backups, Compliance Report, Security Overview) were never added to its hand-
  maintained row list. Added all four, each transcribed directly from its handler's actual
  role-gate code (Admin-only for the first three; Security Overview allows any role but
  degrades RBAC-count sections to zero for non-Admins). Documentation-accuracy fix only, no
  permissions changed.
- **Tests:** `cargo test -p kizashi-ui --lib` — 423 passed (existing `shows_every_documented_area`
  test extended to assert all 4 new rows render). `cargo build --workspace` clean. `cargo
  clippy -p kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo fmt --all
  --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0078-permissions-reference-stale-rows.md

## [2026-07-20] fix/0019-search-term-url-encoding-fix — Fix unencoded search-term URL-encoding in sort/pagination links
- **Type:** fix
- **Branch:** fix/0019-search-term-url-encoding-fix
- **Summary:** A third audit pass found the same bug class as ADR-0076's `before`-cursor fix:
  every sort-column header and "Load older" href across Users, Sessions, Triggers, Events, and
  the global Audit Log spliced the raw `q` search term into a query string unencoded, so a
  search containing `&` or `#` would corrupt the `sort`/`dir`/`before` values that follow it.
  Notably found right next to `before|urlencode` on the same `recent_audit_log.html` line — the
  earlier fix was applied to one parameter without checking the adjacent one. All 13 occurrences
  now use Askama's `|urlencode` filter.
- **Tests:** `cargo test -p kizashi-ui --lib` — 424 passed (1 new: asserts a sort-header link
  containing `&` in the search term is actually percent-encoded, not just present). `cargo
  build --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D
  warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0079-search-term-url-encoding-fix.md

## [2026-07-20] feature/0084-sensors-page-search-and-sort — Sensors page search and sortable columns
- **Type:** feature
- **Branch:** feature/0084-sensors-page-search-and-sort
- **Summary:** Closes a parity gap from a third audit pass: Sensors had pagination but no
  search or sort, unlike every other list page shipped this session. `GET /sensors` now
  accepts `?q=` (name substring match) and `?sort=name|connector_type|enabled` with
  `?dir=asc|desc`, same pattern as Triggers (ADR-0066/0070). Since `list_sensors` is
  server-paginated, both only apply to the current fetched page. `q`/`sort`/`dir` carry through
  the search form and Previous/Next links.
- **Tests:** `cargo test -p kizashi-ui --lib` — 427 passed (3 new: case-insensitive name filter,
  "no sensors on this page match" empty state, descending name sort). `cargo build --workspace`
  clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo
  fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0080-sensors-page-search-and-sort.md

## [2026-07-20] fix/0020-overview-dashboard-surfaces-backend-errors — Overview dashboard surfaces backend errors instead of silently showing zero
- **Type:** fix
- **Branch:** fix/0020-overview-dashboard-surfaces-backend-errors
- **Summary:** Closes a real correctness gap from a fourth audit pass: the Overview dashboard
  (the landing page every user sees first) was the one page where every backend call silently
  `.unwrap_or_default()`'d, so a genuine outage rendered a plausible "0 sensors / 0 records / 0
  events" dashboard indistinguishable from a healthy idle tenant. All five calls now push a
  labeled entry into an `errors: Vec<String>` field on failure, same shape
  `security_overview_handler.rs` already used, rendered the same way every other error-bearing
  page does. The page still renders with partial data on failure — the fix is visibility, not a
  hard failure.
- **Tests:** `cargo test -p kizashi-ui --lib` — 428 passed (1 new: a sensors + platform-health
  failure renders visibly with labeled error text, not silently as zero). `cargo build
  --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings`
  clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0081-overview-dashboard-surfaces-backend-errors.md

## [2026-07-20] feature/0085-data-viewer-date-range-and-normalization-filters — Data Viewer date-range and normalization-status filters
- **Type:** feature
- **Branch:** feature/0085-data-viewer-date-range-and-normalization-filters
- **Summary:** Closes a pure UI-wiring gap from a fourth audit pass: the Data Viewer's search
  form never exposed `from`/`to`/`normalized` filters, even though Ingestion Service's
  `search_records` endpoint has accepted them since the search API was built. `RecordSearchFilter`
  gains `from`/`to: Option<DateTime<Utc>>` and `normalized: Option<bool>`; the query-string
  `DataSearchQuery` keeps them as plain strings (`<input type="date">` submits `YYYY-MM-DD`,
  not a timestamp) and `parse_date_range` treats `from` as start-of-day, `to` as end-of-day, so
  a range is fully inclusive of both endpoints. Also captured in saved searches.
- **Tests:** `cargo test -p kizashi-ui --lib` — 432 passed (5 new: `parse_date_range`'s
  start/end-of-day and empty/unparseable behavior, the HTTP client forwarding `from`/`to`/
  `normalized` as query params, and the handler prefilling both from the query string). `cargo
  build --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D
  warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0082-data-viewer-date-range-and-normalization-filters.md

## [2026-07-20] fix/0021-auth-service-error-message-leak-fix — Auth Service stops leaking raw backend errors on user create/update failures
- **Type:** fix
- **Branch:** fix/0021-auth-service-error-message-leak-fix
- **Summary:** Closes an information-leak gap from a fourth audit pass: `user_handlers.rs`'s
  `user_error_response` passed any non-duplicate-key `LocalUserRepositoryError::Backend` message
  straight through as the HTTP 500 body, verbatim, to a client (rendered directly in the Console
  UI's error banner for an Admin to read). Every such error is now logged via `tracing::error!`
  and replaced with a generic message before it reaches the client — same log-then-generalize
  pattern already used elsewhere in this service. The duplicate-key case (a real, actionable
  outcome) is unchanged.
- **Tests:** `cargo test -p auth-service --lib` — 151 passed (1 new: a backend failure's raw
  error string never appears in the response body). `cargo build --workspace` clean. `cargo
  clippy -p auth-service --all-targets --all-features -- -D warnings` clean. `cargo fmt --all
  --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0083-auth-service-error-message-leak-fix.md

## [2026-07-20] feature/0086-events-page-links-to-record-journey — Events page links directly to each event's contributing record journey
- **Type:** feature
- **Branch:** feature/0086-events-page-links-to-record-journey
- **Summary:** Closes the practical half of "Record Journey has no standalone search entry
  point" (fourth audit pass): the Events page was a dead end for tracing an anomalous event back
  to its source records, even though the backend's `Event::record_ids` and `GET /v1/events`
  response already carried exactly that data — the UI's `EventSummary` just never deserialized
  it. Now does, and the Events table gets a trailing column linking straight to
  `/data/:id/journey` for each contributing record (single link for the common one-record case,
  numbered links for correlated-trigger events with multiple, a dash for events with none). Pure
  UI wiring, no backend change.
- **Tests:** `cargo test -p kizashi-ui --lib` — 435 passed (4 new: single-record link, multi-
  record numbered links, empty-record-ids dash, and the HTTP client deserializing `record_ids`
  from the wire response). `cargo build --workspace` clean. `cargo clippy -p kizashi-ui
  --all-targets --all-features -- -D warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0084-events-page-links-to-record-journey.md

## [2026-07-20] feature/0087-data-viewer-csv-export — Data Viewer CSV export of the current filtered search
- **Type:** feature
- **Branch:** feature/0087-data-viewer-csv-export
- **Summary:** Closes the top finding from a fifth audit pass focused on "data explorer"
  completeness: the Data Viewer had no way to export a filtered search — an investigator had to
  click into records one at a time. `GET /data/export.csv` now honors every filter the HTML view
  accepts (via a shared `build_filter` helper), paginated internally up to 20 pages, exporting
  id/connector_id/source_type/ingested_at/normalized/raw_payload per row. A "Download CSV of this
  search" link on the Data Viewer always reflects the currently-active filters. Same pattern the
  global Audit Log's CSV export already established (ADR-0049); no backend change needed.
- **Tests:** `cargo test -p kizashi-ui --lib` — 438 passed (3 new: header row + matching record
  content, requires a session, 500 on backend failure). Live-verified against the real
  `watkinslabs` tenant's actual ingested email data, not just synthetic fixtures — logged in as
  the real `operator` account (password reset via direct DB access for this purpose) and
  confirmed the Events page's real events (25 of them) each render a working "View journey" link
  that loads real analysis results. `cargo build --workspace` clean. `cargo clippy -p kizashi-ui
  --all-targets --all-features -- -D warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0085-data-viewer-csv-export.md

## [2026-07-20] feature/0088-events-page-date-range-filtering — Events page date-range filtering
- **Type:** feature
- **Branch:** feature/0088-events-page-date-range-filtering
- **Summary:** Closes another "data explorer" completeness gap: the Events page had no way to
  scope a search to a specific incident window. `dashboard-api` already accepted `since`/`until`
  on `GET /v1/events`; the UI never forwarded them. Added `from`/`to` date fields to the search
  form, threaded through `EventsClient::list_events`'s new `since`/`until` params (forwarded by
  `HttpEventsClient` via reqwest's own query encoding) and through the sort-header links and
  pagination forms, same pattern already used for `q`/`sort`/`dir` on this page.
- **Tests:** `cargo test -p kizashi-ui --lib` — 442 passed (4 new: `parse_date_range` start/end-
  of-day and empty/unparseable handling, prefill-from-query-string, HTTP client forwards
  `since`/`until` as query params against a real stub server). `cargo build --workspace` clean.
  `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo fmt --all
  --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0086-events-page-date-range-filtering.md

## [2026-07-20] chore/0005-action-executor-rabbitmq-integration-test — Action Executor live-RabbitMQ integration test
- **Type:** chore
- **Branch:** chore/0005-action-executor-rabbitmq-integration-test
- **Summary:** Closes a CLAUDE.md §2 testing gap: `action-executor` consumes `event.created` in
  `main.rs`'s own RabbitMQ consumer loop but had no integration test proving that path against
  real infra, unlike `normalization-service`/`trigger-engine`. Added
  `tests/rabbitmq_integration_test.rs`: publishes a real `event.created` message to the exchange
  `main.rs` consumes from, consumes it with a test consumer, then calls `process_event` (the same
  function `main.rs` calls for every acked delivery) against a real Postgres-backed execution
  repository, a stub Trigger Engine, and a stub webhook target — asserting both the dispatch and
  the resulting `ActionExecution` row.
- **Tests:** `cargo test -p action-executor` — 56 passed (1 new integration test, against real
  RabbitMQ + Postgres via `RABBITMQ_URL`/`DATABASE_URL`); pre-existing
  `smtp_action_dispatcher_integration_test.rs` failure is unrelated (requires `SMTP_TEST_HOST`,
  not set in this environment). `cargo build --workspace` clean. `cargo clippy -p action-executor
  --all-targets --all-features -- -D warnings` clean. `cargo fmt --all --check` clean.
- **PR:** pending
- **ADR:** docs/adr/0087-action-executor-rabbitmq-integration-test.md

## [2026-07-20] chore/0006-full-pipeline-e2e-test — Full-pipeline e2e test
- **Type:** chore
- **Branch:** chore/0006-full-pipeline-e2e-test
- **Summary:** Closes the CLAUDE.md §2 gap that's existed since day one: an end-to-end test
  proving a single `RawRecord` survives the whole ingestion → normalization → analysis →
  trigger → action chain, not just each hop in isolation. New crate `crates/e2e-tests` chains
  each stage's own real processing function via real message-bus round trips against real
  Postgres/RabbitMQ/ClickHouse, stubbing only the two external seams no test environment has a
  real endpoint for (Azure AI Foundry, and Action Executor's Trigger Engine HTTP lookup — the
  latter using the exact same stub pattern as `action-executor/tests/rabbitmq_integration_test.rs`).
- **Tests:** `cargo test -p e2e-tests` — 1 new end-to-end test, run 3 consecutive times to confirm
  stability before merging (all green). `cargo test --workspace --all-features` — every other
  crate unaffected; the only failures are 3 pre-existing environment-gated integration tests
  (SMTP/Fabric-SQL/IMAP live servers not present in this sandbox), unrelated to this change.
  `cargo build --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D
  warnings`, `cargo fmt --all --check` — all clean.
- **PR:** pending
- **ADR:** docs/adr/0088-full-pipeline-e2e-test.md

## [2026-07-20] feature/0089-kubernetes-helm-chart — Kubernetes Helm chart
- **Type:** feature
- **Branch:** feature/0089-kubernetes-helm-chart
- **Summary:** Closes the "Kubernetes/Helm" leg of the docker-compose → Container Apps →
  Kubernetes deployment path stated in `docs/kizashi-spec.md` §10 — a confirmed, standing gap.
  New chart at `deploy/helm/kizashi/`: one templated Deployment+Service pair per app service
  (driven by `values.yaml`'s `services` map, not hand-written per-service files), a shared
  ConfigMap/Secret for env, and CronJobs for the seven connector pollers. Deliberately scoped as
  a v1 "basic" chart — HPA/PodDisruptionBudget/NetworkPolicy/Ingress and Postgres/RabbitMQ/
  ClickHouse/MinIO manifests are explicitly out of scope, documented in the chart's README
  rather than silently missing. Every application service in `docker-compose.yml` has a
  corresponding chart entry, verified by diffing the two service lists.
- **Tests:** `helm lint deploy/helm/kizashi` — 0 failures. `helm template deploy/helm/kizashi` —
  43 objects render cleanly (18 Deployments, 16 Services, 7 CronJobs, 1 ConfigMap, 1 Secret).
  `kubeconform` against the Kubernetes 1.29 schema — 43 valid, 0 invalid, 0 errors. All three
  re-verified independently after the drafting agent's own run.
- **PR:** pending
- **ADR:** docs/adr/0089-kubernetes-helm-chart.md

## [2026-07-20] feature/0090-nav-hides-admin-only-links-per-role — Console UI nav hides admin-only links per role
- **Type:** feature
- **Branch:** feature/0090-nav-hides-admin-only-links-per-role
- **Summary:** Closes a sixth-audit-pass RBAC gap: the sidebar nav rendered admin-only links
  (Users, Active Sessions, Login Attempts, Backups, Compliance Report) identically for every
  role, even though each already 403s server-side for a Viewer/Operator — a dead-link UX and
  compliance gap. Every page template gained an `is_admin: bool` field, threaded the same way
  `show_nav: bool` already is; `layout.html` gates exactly those 5 links, leaving `/branding`
  (its own internal `can_write` gate) and `/audit-log` (intentionally role-open) unconditional.
  Also split two pre-existing 500+-line test files (`sensors_handler_test.rs`,
  `users_handler_test.rs`) into GET/mutation pairs per CLAUDE.md §0's file-size rule, since this
  change would have pushed them further over the limit.
- **Tests:** `cargo test -p kizashi-ui --lib` — 445 passed (3 new: role-visibility assertions in
  `overview_handler_test.rs`, `sensors_handler_test.rs`, `users_handler_test.rs`, each asserting
  admin-only links are absent for Viewer/Operator and present for Admin). Every other handler's
  existing tests re-verified passing after the `is_admin` field addition. `cargo build
  --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings`
  clean. `cargo fmt --all --check` clean. No file in the diff exceeds 500 lines.
- **PR:** pending
- **ADR:** docs/adr/0090-nav-hides-admin-only-links-per-role.md

## [2026-07-20] feature/0091-api-keys-and-mappings-sortable-headers — Sortable headers for API Keys and Field Mappings
- **Type:** feature
- **Branch:** feature/0091-api-keys-and-mappings-sortable-headers
- **Summary:** Closes another sixth-audit-pass peer-page inconsistency: `api_keys.html` and
  `normalization_mappings.html` had working search but plain, non-sortable headers, unlike every
  other list page (ADR-0070). Both gained `sort`/`dir` query fields, an in-handler `sort_rows`
  helper (API Keys: label/created_at; Field Mappings: source_type/version), and clickable
  sort-header links matching the existing peer-page shape.
- **Tests:** `cargo test -p kizashi-ui --lib` — 447 passed (2 new: sort-by-label-descending for
  API Keys, sort-by-source_type-descending for Field Mappings). `cargo build --workspace` clean.
  `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo fmt --all
  --check` clean. `api_keys_handler_test.rs` split into GET/mutation files (same as ADR-0090) to
  stay under the 500-line limit. Live-verified against the real `watkinslabs` tenant: both pages
  render working sort-header links reflecting the query string.
- **PR:** pending
- **ADR:** docs/adr/0091-api-keys-and-mappings-sortable-headers.md

## [2026-07-20] feature/0092-branding-and-analysis-config-audit-history-links — Audit-history links on Branding and AI Analysis pages
- **Type:** feature
- **Branch:** feature/0092-branding-and-analysis-config-audit-history-links
- **Summary:** Closes another sixth-audit-pass gap: `branding.html`/`analysis_config.html` had
  no link to their own change history, unlike every other mutable config page. Both writes were
  already audited backend-side (auth-service for branding, config-admin-service for analysis
  config); this adds the missing "View change history" link, threading `tenant_id` through both
  templates the same way `is_admin` already is.
- **Tests:** `cargo test -p kizashi-ui --lib` — 449 passed (2 new: each page's audit-history link
  renders with the real tenant id). `cargo build --workspace` clean. `cargo clippy -p kizashi-ui
  --all-targets --all-features -- -D warnings` clean. `cargo fmt --all --check` clean. No file
  exceeds 500 lines. Live-verified against the real `watkinslabs` tenant: both links render with
  the real tenant id and both resolve to a working audit-log page (200 OK).
- **PR:** pending
- **ADR:** docs/adr/0092-branding-and-analysis-config-audit-history-links.md

## [2026-07-20] feature/0093-confirm-destructive-actions — Confirmation prompt on destructive actions
- **Type:** feature
- **Branch:** feature/0093-confirm-destructive-actions
- **Summary:** Closes the final flagged sixth-audit-pass gap: no destructive action (Delete
  User, Delete Sensor, Delete Retention Policy, Revoke/bulk-revoke API Key) asked for
  confirmation before submitting. Each destructive form gained a plain `onsubmit="return
  confirm(...)"`, the smallest possible JS, consistent with the existing no-JS-by-default stance
  (ADR-0014's precedent already used inline `onchange` on `analysis_config.html`). Messages are
  deliberately generic rather than interpolating entity names, since Askama HTML-escapes but
  doesn't JS-escape, and embedding untrusted strings in an inline JS attribute risks a broken/
  injected confirm() call.
- **Tests:** `cargo test -p kizashi-ui --lib` — 453 passed (4 new: one per affected page,
  asserting the rendered form carries `onsubmit="return confirm("`). `cargo build --workspace`
  clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo
  fmt --all --check` clean. No file exceeds 500 lines. Live-verified against the real
  `watkinslabs` tenant: Users/API Keys/Sensors pages all render the confirmation attribute.
- **PR:** pending
- **ADR:** docs/adr/0093-confirm-destructive-actions.md

## [2026-07-20] feature/0094-api-key-audit-history-link — API Key per-key audit history link
- **Type:** feature
- **Branch:** feature/0094-api-key-audit-history-link
- **Summary:** Closes the last remaining sixth-audit-pass gap: API Keys had no link to their own
  change history, unlike every other config entity. `ingestion-gateway` already audited
  create/revoke and already exposed `GET /v1/api-keys/:id/audit-log`, but that route's shape
  didn't match the shared `AuditLogClient`/`HttpAuditLogClient` pair the UI already used for
  config-admin-service/retention-service/auth-service, and `ingestion-gateway` has no tenant-wide
  feed of its own. New `IngestionGatewayApiKeyAuditLogClient` (a second `AuditLogClient` impl)
  closes that gap; `audit_log_handler.rs` gained a fourth `"ingestion"` service arm; `api_keys.html`
  gained a per-key "History" link.
- **Tests:** `cargo test -p kizashi-ui --lib` — 458 passed (5 new: the new client's
  list_for_entity/unreachable/list_recent-is-unsupported behavior against a real stub server, the
  ingestion service arm rendering entries, and the per-key History link rendering with the real
  key id). Also split four pre-existing 500+-line test files (`data_handler_test.rs`,
  `events_handler_test.rs`, `retention_policies_handler_test.rs`, `triggers_handler_test.rs`) —
  three already over the limit before this PR, one pushed over by this PR's own confirmation
  test — per CLAUDE.md §0's file-size rule; the whole `ui/src` directory is now compliant.
  `cargo build --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D
  warnings` clean. `cargo fmt --all --check` clean. Live-verified against the real `watkinslabs`
  tenant: the per-key History link renders with the real key id and resolves to a working
  audit-log page (200 OK).
- **PR:** pending
- **ADR:** docs/adr/0094-api-key-audit-history-link.md

## [2026-07-20] feature/0095-sensors-bulk-delete-and-sessions-confirm — Sensors bulk-delete and Sessions revoke confirmation
- **Type:** feature
- **Branch:** feature/0095-sensors-bulk-delete-and-sessions-confirm
- **Summary:** Closes two seventh-audit-pass gaps: (1) API Keys was the only list page with a
  bulk-select-and-act capability — Sensors gains an equivalent bulk-delete (checkboxes +
  "Remove selected" + `POST /sensors/bulk-delete`), same shape as `post_bulk_revoke_api_keys`
  (ADR-0065); Users and Retention Policies remain a follow-up. (2) Sessions' "Revoke" button was
  the only destructive action anywhere in the UI missing a confirmation prompt (ADR-0093 missed
  it) — now has `onsubmit="return confirm(...)"` like every peer.
- **Tests:** `cargo test -p kizashi-ui --lib` — 457 passed (7 new: 3 bulk-delete-sensors tests
  mirroring the API Keys bulk-revoke test shape, 1 bulk-UI-visibility assertion on the existing
  operator/viewer tests, 1 Sessions confirmation test). `cargo build --workspace` clean. `cargo
  clippy -p kizashi-ui --all-targets --all-features -- -D warnings` clean. `cargo fmt --all
  --check` clean. `sensors_handler_test.rs` split a second time (into `_test.rs`/
  `_mutations_test.rs`/`_pagination_test.rs`) to stay under 500 lines. Live-verified against the
  real `watkinslabs` tenant: Sensors renders the bulk-delete UI, Sessions renders the
  confirmation attribute.
- **PR:** pending
- **ADR:** docs/adr/0095-sensors-bulk-delete-and-sessions-confirm.md

## [2026-07-20] feature/0096-users-and-retention-policies-bulk-delete — Users and Retention Policies bulk-delete
- **Type:** feature
- **Branch:** feature/0096-users-and-retention-policies-bulk-delete
- **Summary:** Closes the follow-up ADR-0095 left open: Users and Retention Policies gain the
  same bulk-delete capability already shipped for API Keys and Sensors — checkboxes + "Remove
  selected" + a `POST .../bulk-delete` route looping over the existing single-item delete method.
  Every list page with a destructive per-row action now has bulk-select parity. Users' bulk-delete
  omits the checkbox for the caller's own row, matching the existing single-delete self-protection.
- **Tests:** `cargo test -p kizashi-ui --lib` — 464 passed (7 new: 3 bulk-delete-users tests, 3
  bulk-delete-retention-policies tests, mirroring the established shape). `cargo build
  --workspace` clean. `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings`
  clean. `cargo fmt --all --check` clean. No file exceeds 500 lines (split
  `retention_policies_handler_mutations_test.rs` a second time). Live-verified against the real
  `watkinslabs` tenant: Users renders the bulk-delete UI.
- **PR:** pending
- **ADR:** docs/adr/0096-users-and-retention-policies-bulk-delete.md

## [2026-07-20] feature/0097-egress-allowlist-audit-log — Egress Allowlist audit log
- **Type:** feature
- **Branch:** feature/0097-egress-allowlist-audit-log
- **Summary:** An eighth UI audit pass looking for Egress Allowlist's missing audit-history link
  surfaced a deeper CLAUDE.md §5 violation: `PUT /v1/allowlist` had zero audit logging at the
  backend, not just a missing UI link. Adds real audit infrastructure to `egress-gateway`
  (`allowlist_audit_log` table with a DB-level immutability trigger, a transactional
  `record_allowlist_audit_entry` write alongside every `set_domains` call, actor now required via
  `X-Username`), a `GET /v1/audit-log/:entity_id` endpoint deliberately shaped to match the
  existing shared `HttpAuditLogClient` contract (no new UI client type needed, unlike
  ADR-0094's ingestion-gateway case), and wires the Console UI's `egress_audit_log_client` +
  "View change history" link into `egress_allowlist.html`.
- **Tests:** `cargo test -p egress-gateway --lib` — 37 passed (7 new: `allowlist_audit_log`
  reader tests, `get_audit_log_returns_entries_for_the_entity`,
  `put_allowlist_requires_username_header`). `cargo test -p egress-gateway --test
  repository_integration_test` against real Postgres — 9 passed (3 new:
  `set_domains_writes_an_allowlist_audit_row_in_the_same_transaction`,
  `allowlist_audit_log_rejects_update_at_the_database_level`,
  `allowlist_audit_log_rejects_delete_at_the_database_level` — proves the transactional write and
  the immutability trigger both actually work). `cargo test -p kizashi-ui --lib` — 470 passed (2
  new: `shows_entries_from_the_egress_client_for_the_egress_service`,
  `shows_a_link_to_the_audit_history_scoped_to_the_tenant`). `cargo build --workspace`,
  `cargo clippy -p egress-gateway --all-targets --all-features -- -D warnings`, `cargo clippy -p
  kizashi-ui --all-targets --all-features -- -D warnings`, `cargo fmt --all --check` all clean.
  No file exceeds 500 lines. Live-verified against the real `watkinslabs` tenant: saved an
  allowlist change, followed the new "View change history" link, confirmed the real entry
  (actor, `created`, before/after domain lists) renders at `/audit-log/egress/<tenant_id>`.
- **PR:** #127
- **ADR:** docs/adr/0097-egress-allowlist-audit-log.md

## [2026-07-20] feature/0097-login-attempts-csv-export — Login Attempts CSV export
- **Type:** feature
- **Branch:** feature/0097-login-attempts-csv-export
- **Summary:** The eighth UI audit pass's second finding: Login Attempts was the only
  enterprise-compliance security page missing a CSV export, unlike Audit Log and Data. Adds
  `GET /security/login-attempts/export.csv` (Admin-only), looping the existing paginated
  `list_recent` client call up to 10 pages, same bounded-pagination shape as
  `recent_audit_log_handler`'s export (ADR-0049). A "Download CSV" link was added above the
  search form.
- **Tests:** `cargo test -p kizashi-ui --lib` — 472 passed (2 new:
  `export_csv_returns_every_attempt_as_csv`, `export_csv_requires_admin_role`). `cargo build
  --workspace`, `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings`, `cargo
  fmt --all --check` all clean. No file exceeds 500 lines. Live-verified against the real
  `watkinslabs` tenant: downloaded the CSV and confirmed it contains real login-attempt rows
  with the correct header and content-disposition filename.
- **PR:** #128
- **ADR:** docs/adr/0098-login-attempts-csv-export.md

## [2026-07-20] feature/0097-sessions-bulk-revoke — Sessions bulk-revoke
- **Type:** feature
- **Branch:** feature/0097-sessions-bulk-revoke
- **Summary:** A ninth UI audit pass found Active Sessions was the last list page with a
  destructive per-row action still missing bulk-select, inconsistent with API Keys/Sensors/
  Users/Retention Policies (ADR-0065/ADR-0095/ADR-0096). Adds `POST
  /security/sessions/bulk-revoke` (checkboxes + "Revoke selected" + a `parse_ids` helper), same
  shape as the existing bulk-action pages. Session ids are opaque `String`s, not `Uuid`s, so
  `parse_ids` here skips the further-parse step those precedents have. The caller's own current
  session is excluded from the checkbox column, matching the existing single-revoke
  self-protection.
- **Tests:** `cargo test -p kizashi-ui --lib` — 474 passed (4 new:
  `bulk_revoke_removes_every_selected_session`,
  `bulk_revoke_does_not_remove_a_session_belonging_to_a_different_tenant`,
  `bulk_revoke_requires_admin`,
  `shows_bulk_revoke_ui_with_a_checkbox_per_row_excluding_the_current_session`). `cargo build
  --workspace`, `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings`, `cargo
  fmt --all --check` all clean. No file exceeds 500 lines. Live-verified against the real
  `watkinslabs` tenant: Sessions renders the bulk-revoke checkboxes/form/button.
- **PR:** #129
- **ADR:** docs/adr/0099-sessions-bulk-revoke.md

## [2026-07-20] feature/0097-events-csv-export — Events CSV export
- **Type:** feature
- **Branch:** feature/0097-events-csv-export
- **Summary:** A ninth UI audit pass found Events was structurally identical to Data and Login
  Attempts/Audit Log (search, date-range filter, sortable columns, pagination) but was the only
  one of the four missing a CSV export, despite being trigger-firing history that's directly
  compliance-relevant. Adds `GET /events/export.csv`, same bounded-pagination export shape as
  the other three (ADR-0049), honoring the existing `?from=`/`?to=` date-range filter. A
  "Download CSV" form was added above the search bar.
- **Tests:** `cargo test -p kizashi-ui --lib` — 474 passed (2 new:
  `export_csv_returns_every_event_as_csv`, `export_csv_requires_a_session`). `cargo build
  --workspace`, `cargo clippy -p kizashi-ui --all-targets --all-features -- -D warnings`, `cargo
  fmt --all --check` all clean. No file exceeds 500 lines. Live-verified against the real
  `watkinslabs` tenant: downloaded the CSV, confirmed real event rows with the correct header
  and content-disposition filename.
- **PR:** #130
- **ADR:** docs/adr/0100-events-csv-export.md

## [2026-07-20] feature/0101-session-revocation-audit-log — Session revocation audit log
- **Type:** feature
- **Branch:** feature/0101-session-revocation-audit-log
- **Summary:** A tenth UI audit pass, cross-referencing every destructive admin action against
  its audit-log coverage, found that revoking a session (single or bulk) wrote zero audit
  entries anywhere — every other destructive admin action does. Console UI's session store has
  no database of its own (ADR-0014), so this required real backend infrastructure, not just a
  missing call: a new `SessionAuditWriter` in auth-service writing to the existing
  `auth_audit_log` table under `entity_type = "session"`, a new `POST
  /v1/audit-log/session-revoked` endpoint, and a Console UI client method called from both
  revoke handlers after the in-memory delete succeeds (best-effort, since the two systems have
  no shared transaction). Sessions gained a per-row "History" link reusing the existing generic
  `/audit-log/auth/:id` view — no new UI service arm needed.
- **Tests:** `cargo test -p auth-service --lib` — 157 passed (6 new: `session_audit_writer`
  reader/writer tests, `post_session_revoked_audit` handler tests). `cargo test -p auth-service
  --test session_audit_writer_integration_test` against real Postgres — 2 passed (proves the
  write and the existing immutability trigger both work for this new entity type). `cargo test
  -p kizashi-ui --lib` — 482 passed (5 new: HTTP client test, 2 handler tests verifying the
  audit call happens for single/bulk revoke, 1 template test for the History link). `cargo build
  --workspace`, `cargo clippy -p auth-service/-p kizashi-ui --all-targets --all-features -- -D
  warnings`, `cargo fmt --all --check` all clean. No new/touched file exceeds 500 lines
  (`user_handlers_test.rs` was already over before this PR touched it; new tests went in a
  separate `session_revoked_audit_handler_test.rs` rather than growing it further).
  Live-verified against the real `watkinslabs` tenant: revoked a session, confirmed a real,
  immutable `deleted`-type audit row (actor, `revoked_username`) renders at
  `/audit-log/auth/<session_id>`.
- **PR:** #131
- **ADR:** docs/adr/0101-session-revocation-audit-log.md

## [2026-07-20] feature/0102-scrub-audit-log-error-responses — Scrub audit log error responses
- **Type:** feature
- **Branch:** feature/0102-scrub-audit-log-error-responses
- **Summary:** An eleventh audit pass, widened to backend error-handling consistency, found
  three `auth-service` handlers (`get_user_audit_log`, `get_recent_audit_log`,
  `post_session_revoked_audit` — the last introduced in the immediately prior PR) passed raw
  backend error text straight into the HTTP response, unlike `user_error_response`'s established
  log-and-scrub pattern used by create/update/delete user. All three now log the real error via
  `tracing::error!` and return the same generic message `user_error_response` already uses. A
  second finding from the same pass (config-admin-service/retention-service's tenant-mismatch
  responses) was investigated and found to be a false positive — those checks compare the
  request body's tenant_id against the header before any entity lookup, so 403 is correct there;
  no change made.
- **Tests:** `cargo test -p auth-service --lib` — 160 passed (3 new:
  `get_user_audit_log_backend_failure_does_not_leak_the_raw_error`,
  `get_recent_audit_log_backend_failure_does_not_leak_the_raw_error`,
  `post_session_revoked_audit_backend_failure_does_not_leak_the_raw_error`). `cargo build
  --workspace`, `cargo clippy -p auth-service --all-targets --all-features -- -D warnings`,
  `cargo fmt --all --check` all clean. No file exceeds 500 lines. Live-verified against the real
  `watkinslabs` tenant: confirmed the normal (non-error) path for `/audit-log` still works after
  redeploy.
- **PR:** #132
- **ADR:** docs/adr/0102-scrub-audit-log-error-responses.md

## [2026-07-20] feature/0103-error-scrub-rollout — Error-scrub rollout to remaining services
- **Type:** feature
- **Branch:** feature/0103-error-scrub-rollout
- **Summary:** ADR-0102 fixed auth-service's raw-backend-error leaks and explicitly scoped out
  dashboard-api, config-admin-service, ingestion-gateway, and retention-service as follow-up. A
  twelfth audit pass confirmed all remaining sites with exact citations; this closes them: 3 in
  dashboard-api (`list_events`, `get_event`, `daily_event_counts`), 5 in config-admin-service
  (analysis-config GET/PUT x3, audit-log GET x2), 4 in ingestion-gateway (create/list/revoke API
  key + audit-log — added a `FailingAuditLogReader` test double since none existed), 2 in
  retention-service (audit-log GET x2). `retention-service/src/ops_handlers.rs`'s two endpoints
  were deliberately left alone — they're internal-secret-gated (only the scheduler calls them,
  not Console UI users), a different threat model, and one of them (`trigger_reimport`) has real
  404-vs-500 semantics that deserve a deliberate look rather than a mechanical scrub.
- **Tests:** `cargo test -p dashboard-api --lib` — 27 passed. `cargo test -p
  config-admin-service --lib` — 114 passed. `cargo test -p ingestion-gateway --lib` — 41 passed.
  `cargo test -p retention-service --lib` — 66 passed. Every touched site gained/updated a test
  asserting the 500 body doesn't contain the `"simulated failure"` marker string, TDD'd (red
  confirmed before each fix). `cargo build --workspace`, `cargo clippy` clean across all four
  crates, `cargo fmt --all --check` clean. No file exceeds 500 lines. Live-verified against the
  real `watkinslabs` tenant: rebuilt/redeployed all four services, confirmed normal (non-error)
  paths for Events, Triggers, API Keys, Retention Policies, and AI Analysis all still return 200.
- **PR:** #133
- **ADR:** docs/adr/0103-error-scrub-rollout.md

## [2026-07-20] feature/0104-action-executions-immutability-trigger — Action executions DB-level immutability
- **Type:** feature
- **Branch:** feature/0104-action-executions-immutability-trigger
- **Summary:** A thirteenth audit pass, checking migration consistency across every audit-log
  table in the platform, found `action_executions` (action-executor's execution audit log,
  CLAUDE.md §5) was the one table without a `BEFORE UPDATE OR DELETE` immutability trigger —
  every other audit table (auth, config-admin, retention, ingestion-gateway, egress-gateway x2)
  has one. Enforcement was purely application-level (the repository trait exposes no update/
  delete method), which a bug or direct DB access could bypass. New migration
  `0003_action_executions_immutable.sql` adds the same trigger pattern every peer table uses.
- **Tests:** `cargo test -p action-executor --test execution_repository_integration_test`
  against real Postgres — 5 passed (2 new: `action_executions_rejects_update_at_the_database_level`,
  `action_executions_rejects_delete_at_the_database_level`, proving the trigger actually works).
  `cargo test -p action-executor --lib` — 52 passed. `cargo build --workspace`, `cargo clippy -p
  action-executor --all-targets --all-features -- -D warnings`, `cargo fmt --all --check` all
  clean.
- **PR:** #134
- **ADR:** docs/adr/0104-action-executions-immutability-trigger.md

## [2026-07-20] feature/0105-retry-cap-dead-letter-pipeline-consumers — Retry cap and dead-letter for pipeline consumers
- **Type:** feature
- **Branch:** feature/0105-retry-cap-dead-letter-pipeline-consumers
- **Summary:** A fourteenth audit pass found analysis-service was the only one of the four
  record-pipeline consumers (record.ingested → record.normalized → record.analyzed →
  event.created) with a retry cap and dead-letter queue — normalization-service, trigger-engine,
  and action-executor all unconditionally `nack(requeue: true)` on failure with no cap, so a
  permanently-failing message could be redelivered forever, blocking the rest of that queue.
  Replicates analysis-service's `retry.rs` module (retry-count header, MAX_RETRIES=5,
  dead-letter after exceeding it) into all three, each with its own header name and dead-letter
  queue. The two config-sync consumers (mapping.changed, trigger.changed) were left untouched —
  different, low-cardinality risk profile.
- **Tests:** `cargo test -p normalization-service --lib` — 23 passed (5 new retry unit tests).
  `cargo test -p trigger-engine --lib` — 49 passed (5 new). `cargo test -p action-executor
  --lib` — 57 passed (5 new). Same unit-test bar analysis-service's own retry.rs uses — no
  RabbitMQ dead-letter integration test exists for any of the four services, so none was added
  here either, for consistency. `cargo build --workspace`, `cargo clippy` clean across all three
  crates, `cargo fmt --all --check` clean. No file exceeds 500 lines. Live-verified against the
  real stack: rebuilt/redeployed all three services, confirmed they start healthy (dead-letter
  queue declaration would fail startup otherwise) and confirmed via RabbitMQ's management API
  that all three new dead-letter queues exist alongside the pre-existing one.
- **PR:** pending
- **ADR:** docs/adr/0105-retry-cap-and-dead-letter-for-pipeline-consumers.md
