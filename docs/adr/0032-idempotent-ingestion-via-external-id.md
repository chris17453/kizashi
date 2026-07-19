# ADR-0032: Idempotent ingestion via `external_id`

## Status

Accepted

## Context

Connectors are stateless per invocation (ADR-0013): `agent-scheduler` invokes a connector's
container fresh on every poll, and the connector is handed a `since`-style config value (e.g.
`IMAP_SINCE_DATE`) to know what window of source data to fetch. Nothing in the current design
advances that value between polls — it is whatever the operator configured when the Agent was
created, or (with a future "smart backfill" scheduler enhancement) whatever the scheduler
computes fresh each time. Either way, a connector's poll window commonly *overlaps* the
previous poll's window, and for at least one real connector (IMAP), it is forced to: IMAP's
`SEARCH SINCE <date>` command only has day granularity, so a mailbox polled every 5 minutes
re-scans the entire current day on every single poll.

Before this change, every record returned by a poll became a brand-new `RawRecord` row,
regardless of whether ingestion had already seen it. For a connector that re-scans an
overlapping window (which, per above, is not an edge case but the common case for at least
IMAP), this meant the same source item was ingested again and again — a new row per poll,
each one independently flowing through Normalization → Analysis → Trigger Engine, potentially
firing a Trigger and creating an `Event` every single poll cycle for the same underlying
message, forever. For a real trigger like "alert on suspected acquisition-scam email," this is
not a cosmetic annoyance — it would flood a dashboard and Action Executor with a duplicate
alert every 5 minutes for the same email indefinitely.

## Decision

Add an optional `external_id: Option<String>` field to `RawRecord`. A connector that has a
natural, source-stable identifier for an item (an email's `Message-Id` header, a ticket number,
a row's primary key) sets it; connectors with no such id leave it `None` and get no dedup
(unchanged behavior).

Ingestion Service enforces uniqueness on `(tenant_id, connector_id, external_id)` via a
**partial unique index** (`WHERE external_id IS NOT NULL`, so records with no external id are
never compared against each other) and inserts with `ON CONFLICT ... DO NOTHING`. The insert
path returns whether the row was actually new; `ingest_handler` only publishes
`record.ingested` on a real insert, not on a dedup no-op — so a duplicate never reaches
Normalization/Analysis/Trigger Engine at all, not just "gets stored twice but doesn't fire
downstream."

For the IMAP connector specifically, `external_id` is the message's `Message-Id` header
(RFC 5322 §3.6.4 — globally stable, present on nearly all real mail), falling back to
`"{connector_id}:{uid}"` for the rare message missing that header (IMAP UIDs are unique and
non-reused within one mailbox, so this fallback is still correctly deduping within that
mailbox).

### Alternatives considered

- **Connector-side cursor tracking (e.g. last-seen IMAP UID persisted and advanced by
  `agent-scheduler` between polls).** This is the "don't re-scan the same window at all"
  approach, and is worth doing eventually as a poll-efficiency improvement. It does not by
  itself fully solve correctness, though: IMAP `SEARCH SINCE` is date-only, so a UID-based
  cursor still requires the connector to fetch by UID range rather than by date, which is a
  larger connector-level change deferred as a follow-up. Idempotent ingestion is the
  correctness backstop regardless of how precise the connector's own windowing gets — even a
  perfect cursor design benefits from not depending on exactly-once delivery.
- **Dedup in Normalization/Analysis/Trigger Engine instead of Ingestion Service.** Rejected:
  ingestion is the one place all downstream consumers agree is authoritative for "has this
  source item been seen before," and catching the duplicate at the earliest possible point
  avoids wasted normalization/analysis work and AI/ML spend on a record that will just be
  discarded downstream anyway.

## Consequences

- `RawRecord` gains one optional field; every existing constructor call compiles unchanged
  (`RawRecord::new(...)` still yields `external_id: None`).
- Connectors with no natural external id (SQL rows without a distinguishing key, generic
  webhook payloads, etc.) are unaffected — this is purely additive.
- A connector wanting dedup must supply a *stable* id — if a source's identifier changes
  between polls for the same logical item, dedup silently fails to recognize it as a repeat.
  This is a known, accepted limitation, not a bug: getting a wrong-but-present external id from
  a connector is the connector's responsibility, same as getting `raw_payload` right.
- No retroactive dedup: rows ingested before this migration have `external_id = NULL` and are
  not deduped against future polls of the same source item. Acceptable since this only affects
  the (currently zero) rows already ingested by connectors that will use `external_id` going
  forward.
