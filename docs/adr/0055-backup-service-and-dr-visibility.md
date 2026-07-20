# 0055. Backup service and DR visibility

## Context

The compliance rubric introduced in ADR-0051 names backup/DR visibility as another domain: can
an admin (or an auditor) answer "when did we last back up the platform, and did it succeed?" A
full audit found **no backup automation existed anywhere in the codebase** — no `pg_dump`, no
ClickHouse `BACKUP`, no WAL archiving, no scheduled snapshot job, nothing in
`docker-compose.yml`/`scripts/`/CI referencing backups at all. `retention-service`'s
archive/sweep (ADR-0011) is not a substitute: it archives *aged tenant application data* under a
retention policy (and deletes the hot copy), which is a data-lifecycle feature, not a
whole-database disaster-recovery backup.

Per CLAUDE.md's "no half-truths" rule, a status page with nothing real behind it would be worse
than no page at all — it would visually claim compliance coverage that doesn't exist. So this
ADR covers building the actual backup job first, with the visibility page as a thin read layer
on top of real data.

## Decision

**New crate `crates/backup-service`**, not folded into `retention-service` or `observability`:
retention-service's archive is tenant-scoped application data with a different lifecycle owner;
observability is read-only status aggregation with no DB write path or job-execution model.
Backup is whole-database, ops-owned, and needs its own scheduled-execution + persisted-history
shape.

- **The dump itself shells out to the real `pg_dump` binary** (custom format,
  `pg_dump --format=custom`) rather than reimplementing a Postgres dump format in Rust —
  `pg_dump`/`pg_restore` is what any operator already trusts to produce something actually
  restorable; reinventing it would be exactly the "looks compliant, isn't actually restorable"
  gap this whole rubric exists to close. The runtime Docker image needs `postgresql-client`
  installed, gated behind a new `INSTALL_POSTGRES_CLIENT` build arg on the shared `Dockerfile`
  (same opt-in-per-binary pattern as `INSTALL_DOCKER_CLI` for `agent-scheduler`, ADR-0020).
- **Storage: the same MinIO/S3-compatible infra `retention-service` already uses** (ADR-0011),
  in a separate `kizashi-backups` bucket — reuses proven, self-hosted, no-vendor-lock-in storage
  rather than standing up something new, while keeping the bucket separate since backup and
  archive have different lifecycles/retention needs.
- **`backup_runs` table**: `id, started_at, completed_at, status, target, size_bytes, error`. A
  row is created `Running` and transitions exactly once to `Success` or `Failed` — unlike
  `auth_audit_log`/`login_attempts`, this is operational status, not an audit trail, so it is
  **not** append-only-immutable at the DB level; mutation-in-place is the correct shape here.
- **`POST /v1/backup/run`**: triggers one backup pass. Gated on the shared internal secret only
  (no `X-Role` check), the same v1 stopgap as `retention-service`'s `/v1/sweep`
  (ADR-0011 point 5) — it's a service-to-service operational trigger with no session/user behind
  the call. Externally scheduled via a `backup-scheduler` sidecar in `docker-compose.yml` that
  copies `retention-sweep-scheduler`'s shape exactly (a `curl` loop on an interval), standing in
  for a future Kubernetes CronJob (defaults to daily, `BACKUP_INTERVAL_SECONDS`).
- **`GET /v1/backup/status`**: the last N runs, for the Console UI's new `/security/backups`
  page. Gated on both the internal secret *and* `X-Role: Admin` (unlike the trigger endpoint,
  this is read by an authenticated Console UI admin session) — platform-wide, not tenant-scoped,
  since a backup is of the whole database, not one tenant's slice of it.
- Every failure branch in the executor (`crates/backup-service/src/backup_executor.rs`) still
  writes a `Failed` row — a backup that silently fails to even record its own failure would
  defeat the entire point of the status page.

## Consequences

- **ClickHouse is out of scope for v1.** Postgres holds the platform's operational/config state;
  ClickHouse holds `Event` rows, which are themselves derived from re-processable `RawRecord`
  data (and, per ADR-0011, archived separately under retention policy). A ClickHouse backup
  (`BACKUP` statement / `clickhouse-backup` tool) is a legitimate follow-up but a separate,
  larger piece of work — noted here rather than silently folded in.
- **No restore automation yet** — this ships backup + visibility, not a one-click restore flow.
  Restoring is `pg_restore` against the downloaded `.dump` object, run manually by an operator.
  Automating restore verification (e.g. periodically restoring into a scratch DB to prove the
  backup is actually valid) is a natural next step but adds real infrastructure cost; not v1.
- `pg_dump`'s client-tool version in the `debian:bookworm-slim` runtime image (`postgresql-client`
  from Debian's apt repo) is not guaranteed to exactly match the Postgres server's major version
  (`postgres:16-alpine` in `docker-compose.yml`). `pg_dump` is generally forward-compatible
  within reason, but a future server major-version bump should double check this doesn't drift
  into an actual incompatibility.
- Backup status is platform-wide and visible to any tenant's Admin who reaches
  `/security/backups` — this is intentional (it's operational transparency, not another tenant's
  data), but worth remembering it's a deliberate exception to this app's otherwise-universal
  tenant-scoping convention.
