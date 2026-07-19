# ADR-0033: Narrow the IMAP connector's poll window after the first poll

## Status

Accepted

## Context

ADR-0032 made repeated ingestion of the same message safe (a duplicate `external_id` is a
no-op, and never re-fires downstream Triggers). It deliberately did *not* address the other
half of the same problem: efficiency. Before this change, `agent-scheduler` invoked the IMAP
connector with the exact same `IMAP_SINCE_DATE` from the Agent's static config on every single
poll, forever — an Agent configured with a 6-month backfill window would re-fetch the *entire*
6 months of message bodies over IMAP every 5 minutes (or whatever `poll_interval_seconds` is
set to), not just re-check for new mail. This was caught live, against a real personal mailbox
with hundreds of real messages, before it ran unattended: it is wasteful bandwidth and IMAP
server load, and repeated full-history re-fetches against a real mail server every few minutes
is the kind of behavior that gets an account rate-limited or flagged, not just "inefficient."

`agent-scheduler` already tracks `last_polled_at` per Agent (`StoredAgent.last_polled_at`,
`AgentRepository::mark_polled`) — but only used it to decide *whether* an Agent's poll is due,
never passed it to the invoker, so the connector itself had no way to know "you've already
covered up to roughly this point."

## Decision

`Invoker::invoke` now takes `last_polled_at: Option<DateTime<Utc>>` alongside the `Agent`.
`DockerInvoker::build_run_args` uses it to override `IMAP_SINCE_DATE` specifically for
`connector_type == "imap"`: on the first-ever poll (`last_polled_at: None`), the operator's
originally-configured backfill date is used as-is; on every later poll, it's overridden to
`last_polled_at - 1 day` (a coarse but safe overlap — IMAP's `SEARCH SINCE` command is
date-granularity only, so exact-boundary precision isn't available regardless).

This is a deliberate, narrow, connector-specific special case in `DockerInvoker`, not a
generic "every connector understands a since-window" mechanism. IMAP is the one connector this
scheduler currently knows re-scans a stateless date window; other connectors (Zendesk, Graph
Mail/Teams, SQL, Fabric) are unaffected by this change and keep using whatever config they were
given, unchanged.

### Why not a real UID-based cursor instead

A proper fix tracks the IMAP server's own notion of "new" — the last-seen UID — and fetches
only messages after it, which IMAP supports precisely (unlike date search). That requires the
connector to report its progress back to something durable between polls, since each poll is a
fresh, stateless process (ADR-0013). No such write-back path exists yet (connectors currently
only talk to Ingestion Gateway, one-way). Building that — a scoped, connector-writable "advance
my own cursor" endpoint, likely on Config Admin Service or Agent Scheduler itself — is real
follow-up work, tracked here rather than quietly deferred. The day-granularity `last_polled_at`
narrowing in this ADR is the pragmatic interim fix: it turns "re-fetch the entire configured
backfill window forever" into "re-fetch roughly the last day," which combined with ADR-0032's
dedup closes the operationally serious part of the gap now.

## Consequences

- An IMAP Agent's poll volume drops from O(entire configured backfill window) to
  O(~1 day + dedup-skipped rows) after its first poll — the real problem this ADR exists to
  fix.
- The 1-day overlap means IMAP still re-scans (and dedup-discards) roughly a day's worth of
  already-seen messages on every poll. This is intentionally conservative given date-only
  search granularity; a UID-based cursor (see above) would remove this overlap entirely.
- If `agent-scheduler` itself is ever restarted and its local `last_polled_at` bookkeeping is
  lost (its own Postgres mirror is durable, so this only matters if that table is wiped), an
  IMAP Agent's next poll falls back to `None` and re-does the original full backfill once —
  acceptable, not silently wrong, since dedup still applies.
