# 0077. Local test runs use a separate database from the live stack

## Context

Earlier this session, a user complaint about "so much FAKE DATA" in the Console UI was traced
to its root cause: `.env`'s `DATABASE_URL` pointed at the exact same Postgres database
(`kizashi`, on the `docker-compose.yml`-exposed `POSTGRES_PORT`) that the live docker-compose
stack — and therefore the Console UI a developer is looking at in a browser — reads and writes.
Per CLAUDE.md's mandate to test against real infra rather than mocks, every `cargo test`
integration/repository test run on a developer's machine was writing real fixture rows (test
tenants, sensors, triggers, analysis configs, normalization mappings) directly into that same
database, with no cleanup afterward. At the time this was root-caused and the accumulated junk
was manually cleaned up, but the underlying cause — no separation between "database the live
stack uses" and "database local tests write into" — was left as a flagged, unfixed follow-up.

CI itself was never affected: each CI run gets its own fresh, ephemeral Postgres service
container (`.github/workflows/*.yml`), discarded after the run. This was purely a local
dev-loop gap.

## Decision

- `scripts/bootstrap.sh` now creates a second database, `kizashi_test`, on the same Postgres
  instance/port as `kizashi` (idempotent — checks `pg_database` first, safe to re-run).
- `.env`/`.env.example`'s `DATABASE_URL` now points at `kizashi_test`, not `kizashi`. This
  variable is consumed only by things run directly on the host (`cargo test`, `cargo run`,
  manual `psql`) — every docker-compose service hardcodes its own
  `postgres://.../kizashi` directly in `docker-compose.yml`, entirely unaffected by this change.
  Verified: after this change, running `config-admin-service`'s real-Postgres integration test
  suite (19 tests, all passing) added 9 rows to `kizashi_test` while `kizashi`'s
  `trigger_definitions` count stayed at its real value (1) throughout.
- No schema/migration change needed: every crate's integration test already runs its own
  `sqlx::migrate::Migrator` against whatever `DATABASE_URL` resolves to at test time, so
  `kizashi_test` gets migrated to the current schema the first time any test suite runs
  against it.

## Consequences

- `kizashi_test` will accumulate its own test-fixture junk over repeated local `cargo test`
  runs, same as before — just no longer where it's visible to anyone using the live stack. If
  `kizashi_test` itself needs periodic resetting, `docker compose exec postgres psql -U kizashi
  -d postgres -c "DROP DATABASE kizashi_test;"` followed by re-running `scripts/bootstrap.sh`
  recreates it clean; not automated here since it's a rare, deliberate operator action, not a
  routine one.
- A developer who already has a `.env` from before this change keeps their old `DATABASE_URL`
  pointing at `kizashi` until they diff against `.env.example` and update it by hand — `.env` is
  gitignored and never auto-updated. Worth a one-time callout to any existing local
  contributor, not something this change can force.
- `scripts/bootstrap.sh`'s existing per-crate `sqlx migrate run` loop (best-effort, errors
  suppressed) now runs against `kizashi_test` instead of `kizashi` — harmless, since each
  service already self-migrates its own real database against its own hardcoded
  `docker-compose.yml` `DATABASE_URL` at startup; that loop was already redundant with
  self-migration, not the source of truth for the live stack's schema.
