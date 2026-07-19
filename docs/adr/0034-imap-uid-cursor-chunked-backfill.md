# ADR-0034: IMAP UID cursor with chunked backfill

## Status

Accepted — supersedes ADR-0033's day-overlap approach for the IMAP connector specifically.

## Context

ADR-0033 shipped a coarse fix for IMAP's "re-scans an unbounded configured window forever"
problem: narrow `IMAP_SINCE_DATE` to `last_polled_at - 1 day` after the first poll. That was an
improvement, but wrong for a real mailbox with meaningful volume: a day's worth of mail can
easily be hundreds of messages, and re-scanning (then dedup-discarding) that whole day on
*every* poll interval is still real, avoidable IMAP/network load — not what an enterprise-grade
sync does. Correctly flagged as insufficient before it shipped to a real mailbox.

The other missing half: a bounded historical backfill (e.g. "the last 6 months") was being
fetched in one unbounded burst — a single poll would `SEARCH`+`FETCH` the entire window's
message bodies in one shot, which is exactly what hit `ingestion-gateway`'s rate limit ceiling
(600 records in one call) during live testing. Real enterprise sync systems (Gmail API's
`historyId`, IMAP clients like Thunderbird/offlineimap, Salesforce's replication cursors) don't
work this way: backfill is chunked/paginated, and steady-state sync is cursor-based, not a
repeated full re-scan.

## Decision

Two changes, one mechanism:

**1. A real cursor, not a date approximation.** `common::connector::Connector` gains
`checkpoint(&self, records: &[RawRecord]) -> Option<String>`, default `None`. `ImapConnector`
implements it as the highest `uid` among the records it just polled. IMAP UIDs are
monotonically increasing and never reused within a mailbox (unlike `SEARCH SINCE`, which is
date-only), so `UID {last_uid + 1}:*` gives an *exact* incremental fetch — no overlap needed,
no re-scanning already-seen messages.

`PollSummary` (connector-runtime) now carries this checkpoint. The `imap` connector's `main.rs`
prints it to stdout as a plain marker line (`KIZASHI_CHECKPOINT=<value>`) after a successful
poll. `DockerInvoker` (agent-scheduler) captures the `docker run` process's stdout, extracts
that line, and persists it as `last_checkpoint` on the Agent's row (new migration,
`agents.last_checkpoint TEXT`, via `AgentRepository::mark_polled`). The next invocation passes
it back in as `IMAP_SINCE_UID`.

This keeps the connector process itself fully stateless per invocation (ADR-0013 unchanged):
it never writes anything durable itself, only reports what it saw on its own stdout: the
orchestrator (agent-scheduler) owns persisting and replaying the cursor.

**2. Chunked fetch, same mechanism for backfill and steady-state.** `ImapConnector` gains
`max_records_per_poll` (default 200 via `IMAP_MAX_RECORDS_PER_POLL`). Matched UIDs are sorted
ascending and truncated to this cap before fetching. Combined with the checkpoint above, this
means: a large backfill is automatically consumed in bounded chunks across successive poll
cycles (chunk 1 this poll, checkpoint advances, chunk 2 next poll, ...) using the *exact same
code path* as ordinary "what's new" polling once caught up — there is no separate "backfill
mode" vs. "streaming mode" to build or reason about. The system just naturally transitions from
"lots of chunks in quick succession" to "usually zero or a handful of new messages per poll" as
it catches up to real time.

## Consequences

- IMAP poll volume per cycle is now bounded (≤`max_records_per_poll`) and *exact* — no re-scan
  of already-ingested messages, no rate-limit-ceiling bursts, no day-of-overlap waste.
- `agents.last_checkpoint` is connector-opaque (`TEXT`) by design — this scheduler doesn't need
  to understand what a checkpoint *means*, only persist and replay it verbatim. Any future
  connector that wants the same treatment implements `Connector::checkpoint` and prints the
  same stdout marker; no scheduler changes required.
- If `agent-scheduler`'s own Postgres table is ever wiped, an Agent's `last_checkpoint` resets
  to `None` and its next poll re-runs the full configured backfill once — same acceptable
  fallback ADR-0033 already documented, still true here.
- Real Agent `mail-watkinslabs-com` was disabled twice during this ADR's development (once for
  ADR-0033's now-superseded approach, once again while this one was built) rather than left
  running against a real mailbox with a known-insufficient design — re-enabled only after this
  fix was live-verified.
