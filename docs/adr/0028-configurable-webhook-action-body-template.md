# ADR-0028: Configurable webhook action body template

- **Status:** accepted
- **Date:** 2026-07-19

## Context

ADR-0007 established the v1 dispatch model: every `ActionType` goes through one HTTP-POST
dispatcher, sending a fixed envelope `{action_type, action_config, event}`. That envelope's own
doc comment claimed it was "genuinely functional against any webhook-shaped endpoint (Teams
incoming webhooks, Slack, Zapier/n8n relays, most ticketing/email HTTP APIs)" — but fix/0004
(ADR-adjacent, not its own ADR) found and fixed exactly this claim being false for Teams: a real
Teams incoming webhook validates and requires an `@type: MessageCard` shape, not the generic
envelope, so a `TeamsAlert` action needed its own dedicated dispatcher (`TeamsAlertAction
Dispatcher`) to actually work.

That fix only covered `ActionType::TeamsAlert`, which has its own enum variant. Every other
target with its own required body shape — Slack's incoming webhooks (`{"text": "..."}`
minimum), PagerDuty's Events API v2 envelope, a Jira/ServiceNow REST body — has no dedicated
`ActionType` variant of its own; they're configured today as a generic `Webhook` action, which
still only ever sends the fixed envelope. Building a dedicated dispatcher per vendor (as was
done for Teams) doesn't scale to "every webhook target with opinions about its body shape" —
that's an open-ended, ever-growing list, not a closed enumerable set the way `ActionType` is.

## Decision

**Add an optional `body_template` field to a `Webhook`/`CreateTicket`/`Custom` action's
`config`.** When present, `HttpActionDispatcher` renders it — walking the JSON tree and
substituting `{{event_type}}`, `{{entity_ref}}`, `{{group_key}}`, `{{tenant_id}}`,
`{{occurred_at}}`, and `{{payload}}` placeholders in every string leaf with the firing event's
actual values — and sends the rendered result as the POST body, instead of the generic
envelope. An unrecognized placeholder is left as literal text, not an error; there is no
template compilation step, no code execution, no logic beyond string substitution, so this
can't panic on operator-authored config (the same guarantee CLAUDE.md §2's property-test
requirement holds the trigger DSL to, applied here by construction rather than by fuzz-testing
a whole expression language). Without a `body_template`, behavior is unchanged — the generic
envelope is still sent, so this is purely additive.

This generalizes what the Teams fix did ad hoc into a reusable mechanism: Slack, PagerDuty,
Jira, ServiceNow, or any other webhook target with its own required shape can now be configured
without a new Rust dispatcher per vendor. `TeamsAlertActionDispatcher` itself is unchanged —
still the dedicated path for `ActionType::TeamsAlert`, since that shape (Connector Card facts
built from every event field, not just a handful of placeholders) is richer than simple string
substitution can express cleanly.

## Consequences

- Easier: adding support for a new webhook-shaped third-party target is now a config change an
  operator makes themselves, not a new dispatcher PR. The placeholder set can grow additively
  later (e.g. `{{id}}`, `{{status}}`) without touching existing configured actions.
- Harder: placeholder substitution is deliberately not a full template language (no
  conditionals, loops, nested field access into `payload`, or escaping control) — an operator
  needing something more expressive than flat field substitution still needs a dedicated
  dispatcher or an external relay (Zapier/n8n), same as before this ADR. If demand for richer
  templating emerges, that's a follow-up decision, not retrofitted here ahead of a real need.
