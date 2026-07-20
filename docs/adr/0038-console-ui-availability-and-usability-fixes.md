# ADR-0038: Console UI availability (compose dependency decoupling) and usability fixes

## Status

Accepted.

## Context

A live audit of the Console UI (`kizashi-ui`) — prompted by direct, sharp user feedback that the
UI was unusable — surfaced two classes of real problems, not cosmetic ones:

1. **The container wasn't reachable at all.** `docker ps` showed `kizashi-kizashi-ui-1` in
   `Created` state, never actually started. Root-caused to `docker-compose.yml`:
   `kizashi-ui`'s `depends_on` block required `service_healthy` on ten other services,
   including a chain through `trigger-engine` → `analysis-service`. When `analysis-service`
   went unhealthy during this session's earlier incident (see ADR-0037), `docker compose up`
   for *any* service touching that dependency graph — including `kizashi-ui` itself — refused
   to bring the UI container up at all. One backend's transient health problem took the entire
   operator-facing Console UI down with it, with no visible error beyond a connection refusal.
   This is the most likely explanation for "the UI looks broken" — it may simply not have been
   running.
2. **Real usability gaps once actually rendered.** Screenshotted every page via headless Chrome
   (not just read the templates) and found two concrete defects:
   - The Data Viewer's "Connector ID" search field was free text only, with no way to pick from
     the tenant's actually-registered Sensors — an operator had to already know/remember the
     exact string.
   - The trigger-creation form rendered every field for every condition shape
     (threshold/count/correlated) simultaneously and unconditionally, relying on help text
     ("fill in the field group matching whichever Condition you selected — the others are
     ignored") to compensate for a form that doesn't actually guide the user. The AI Analysis
     page already solved the identical problem correctly (JS-driven show/hide keyed off a
     provider `<select>`) — Triggers just didn't follow that established pattern.

## Decision

- **`kizashi-ui`'s `depends_on` conditions changed from `service_healthy` to `service_started`**
  for all ten backends it lists. This preserves container start *ordering* (compose still waits
  for each to at least begin starting) without hard-blocking the UI's own availability on their
  health. The Console UI already renders per-page/per-widget degraded states for unreachable
  backends (status pills, error banners) — it doesn't need a backend to be *healthy* to be
  useful, just to exist. No other service's `depends_on` graph was touched; this fix is scoped
  to the UI's own entry in the compose file, since it's the one whose unavailability is
  directly user-facing.
- **Data Viewer**: `DataTemplate.sensor_names: Vec<String>` populated from
  `SensorsClient::list_sensors` (capped at 500 — a plain HTML `<datalist>` stops being a usable
  picker well past that, and the field must keep accepting arbitrary free text regardless, since
  not every `connector_id` that appears in ingested records is necessarily a registered Sensor).
  Wired via `<input list="sensor-names">` + `<datalist>`, not a `<select>`, specifically so
  free-text search still works for unregistered/historical connector ids while registered
  Sensors autocomplete.
- **Triggers**: wrapped the threshold/count/correlated field groups in named `<div>`s and added
  a `kizashiUpdateTriggerConditionFields()` handler on the Condition `<select>`'s `onchange`,
  mirroring the AI Analysis page's existing pattern exactly rather than inventing a new one.

## Consequences

- The compose dependency fix is a genuine reliability improvement independent of the usability
  fixes: it closes a real single-point-of-failure discovered live, not a hypothetical one.
- `service_started` is weaker ordering than `service_healthy` — if this proves insufficient in
  practice (e.g. a genuinely required backend isn't ready when the UI's first request arrives),
  the individual HTTP clients' existing retry/error-surface behavior is the fallback, not a
  compose-level guarantee. Worth revisiting if that turns out to be a real problem, not assumed
  away here.
- This audit was not exhaustive — it covered every page in the nav via real screenshots, but
  only fixed the two concrete, highest-signal defects found. Further UI passes should keep using
  the same discipline (screenshot real rendered pages via headless Chrome, not just read
  templates) rather than assuming template code implies correct rendering.
