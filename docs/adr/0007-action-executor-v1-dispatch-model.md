# ADR-0007: Action executor v1 dispatch model

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §5.5 defines `ActionExecution.action_type` as one of `email | webhook | teams_alert |
create_ticket | custom`, and spec §6 (service #7) says Action Executor "consumes matched
events; executes actions; writes ActionExecution audit rows." Each of those five action types
implies a genuinely different integration in a full build: SMTP (or a transactional email API)
for `email`, an arbitrary HTTP callback for `webhook`, the MS Graph/Teams incoming-webhook or
Bot Framework API for `teams_alert`, a specific ticketing system's API (Zendesk, Jira, etc.,
plural and tenant-configurable) for `create_ticket`, and an open-ended integration for `custom`.

Building five distinct, real integrations now — particularly `create_ticket`, which has no
single API shape since it depends on which ticketing system a tenant uses, itself a connector
concern (spec §6, service #1 already lists Zendesk as a connector type) — would mean guessing
at integrations this build-out hasn't reached yet, or duplicating connector-layer work inside
Action Executor.

## Decision

For v1, every `ActionType` dispatches through one `HttpActionDispatcher`: it POSTs a JSON body
(the triggering `Event` plus the `ActionRef.config` the operator supplied) to a URL read from
`ActionRef.config["url"]`. This is not a simplification unique to Kizashi — it's how most
real-world "email/Teams/ticket" automation is actually wired in practice: Teams incoming
webhooks, Zapier/Make/n8n relays, SendGrid/Postmark's HTTP send APIs, and most ticketing
systems' REST APIs are all "POST JSON to a URL" underneath. So `HttpActionDispatcher` is
genuinely functional today for any of the five action types an operator points at a real HTTP
endpoint — it is not a stub that always no-ops.

What it does *not* do yet: type-specific request shaping (e.g. Teams' `MessageCard`/Adaptive
Card JSON schema, a ticketing system's specific field names, SMTP as a fallback when no HTTP
email API is configured). Every `ActionExecution` row records `action_type` faithfully even
though dispatch is currently type-agnostic, so a future per-type dispatcher swap-in doesn't
need a data migration — only `HttpActionDispatcher`'s selection logic changes to route by
`action_type` instead of using one dispatcher for all.

## Consequences

- Easier: Action Executor is fully buildable, testable, and *usable* today — an operator can
  point any action at a real webhook receiver (Teams, Slack, Zapier, a ticketing system's
  webhook-shaped endpoint) and it works, without this build-out needing five separate
  integration builds and their respective credentials/SDKs before Action Executor ships at all.
- Harder: an operator who wants `email` dispatched via raw SMTP (no HTTP email API available)
  or `create_ticket` against an API that isn't webhook/REST-shaped cannot use those action
  types yet. This is the deliberate, documented gap — tracked as follow-up type-specific
  dispatchers, not silently shipped as "full email/ticket support."
