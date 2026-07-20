# 0081. Overview dashboard surfaces backend errors instead of silently showing zero

## Context

A fourth audit pass found the Overview dashboard (`GET /overview`) was the one page in the app
where every list handler had already learned to add an `error: Option<String>` field — except
this one. All five backend calls (`sensors_client`, `stats_client.connector_stats`,
`events_client.list_events`, `health_client`, `backlog_client.queue_depths`) used
`.unwrap_or_default()`/`.ok()`, so a genuine outage on any of them rendered a plausible-looking
"0 sensors / 0 records / 0 events / status: unknown" dashboard — indistinguishable from a
genuinely idle, healthy tenant. This is the landing page every user sees first; silently
degrading it into a false-positive "everything's fine" view is a real correctness problem, not
a cosmetic one.

## Decision

Each of the five calls is now matched explicitly, pushing a labeled entry (`"sensors: {e}"`,
`"platform health: {e}"`, etc.) into an `errors: Vec<String>` field on success/failure, same
shape `security_overview_handler.rs` already established for its own KPI tiles. The template
renders each with `{% for e in errors %}<p class="error">{{ e }}</p>{% endfor %}`, same markup
convention as every other error-bearing page. The dashboard still renders with whatever partial
data it does have (a KPI tile still shows 0 for a genuinely failed call) — the fix is that the
failure is now visible above the tiles, not that the page becomes unusable when one backend is
down.

## Consequences

- Purely additive: no behavior change for the success path, only failure visibility.
- The KPI tiles themselves still show `0` for a failed metric (not "unknown" or hidden) — the
  page doesn't need every call to succeed to be useful, it just needs to stop lying about why a
  number is zero.
