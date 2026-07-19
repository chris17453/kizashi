# ADR-0023: SMTP send action, routed by a new `RoutingActionDispatcher`

- **Status:** accepted
- **Date:** 2026-07-19

## Context

Phase 5 of the gap-closing roadmap calls for a real SMTP send action. Today, `ActionType::Email`
is dispatched exactly like every other action type — `HttpActionDispatcher` POSTs the event as
JSON to whatever `url` the action config carries (ADR-0007's "single HTTP-POST dispatch model
for every ActionType"). That's genuinely useful when "Email" means "call some HTTP email-relay
API" (SendGrid, Postmark, an internal notification service), but it cannot send an actual RFC
5322 email over SMTP — there was no way to configure a trigger to just send a plain email via a
mail server.

The real design question: how does one action type end up dispatched two different ways
(HTTP webhook vs. real SMTP) without either duplicating `ActionDispatcher`'s call sites or
breaking every existing `Email` action already configured as a webhook.

## Decision

1. **A new `SmtpActionDispatcher`** builds and sends a real message via `lettre`
   (`AsyncSmtpTransport<Tokio1Executor>`), reading `smtp_host`, optional `smtp_port` (default
   587), optional `smtp_use_tls` (default `true`, using STARTTLS via `starttls_relay`; `false`
   uses `builder_dangerous` for plain-TCP test/on-prem servers), `from`, `to` (a string or array
   of addresses), and optional `subject`/`smtp_username`/`smtp_password` from the action's
   `config` JSON. Config validation errors (missing `from`/`to`/`smtp_host`, malformed
   addresses) return a new `DispatchError::InvalidConfig` variant, distinct from `MissingUrl`
   (an HTTP-dispatch-specific error) so a caller can tell "this action's config is simply wrong
   for its dispatch path" apart from "no url field at all."
2. **A new `RoutingActionDispatcher`** (the dispatcher `main.rs` actually wires into
   `ActionDeps`) inspects each `ActionRef` before dispatching: `ActionType::Email` **with an
   `smtp_host` field present** routes to `SmtpActionDispatcher`; everything else — every other
   action type, and any `Email` action *without* `smtp_host` — routes to the existing
   `HttpActionDispatcher` unchanged. This means an operator's already-configured "Email via
   webhook relay" trigger keeps working exactly as before; adding `smtp_host` to a new Email
   action's config is what opts it into a real SMTP send. No migration, no breaking change,
   no new `ActionType` variant needed — the routing key is "does this config look like SMTP
   config," which is unambiguous (`HttpActionDispatcher` never expects an `smtp_host` field).
3. **Live testing reuses the same `greenmail` test server** the IMAP connector's ADR-0022
   already introduced (it speaks both SMTP and IMAP) — `action-executor`'s live integration
   test sends a real message via `SmtpActionDispatcher`, then reads it back with a real
   `ImapConnector::poll` call to prove actual delivery, not just "the SMTP server accepted the
   DATA command." This is a genuine cross-crate proof: two independently-built pieces (the SMTP
   send path and the IMAP read path) verifying each other against one real server, not two
   halves of a mock.

## Consequences

- Both the IMAP connector's own live test and this action's live test now share one greenmail
  mailbox in CI (both seed a message with a distinct subject) — the IMAP test was changed to
  search for its expected message by subject rather than assume it's the only/first message
  present, since assuming exclusive ownership of a shared external resource across test suites
  is fragile by construction, not just bad luck.
- `HttpActionDispatcher` itself is unchanged — `RoutingActionDispatcher` composes it rather than
  modifying it, keeping its existing test suite and behavior intact.
- Known gap, not built here: SMTP connection pooling/reuse across dispatches (a fresh
  `AsyncSmtpTransport` is built per send, matching `HttpActionDispatcher`'s existing
  fresh-client-per-dispatch pattern for the same multi-tenant-single-process reason — see
  ADR-0021's `HttpActionDispatcher` rationale) and routing this connection through Egress
  Gateway (SMTP, like IMAP, is not an HTTP CONNECT-tunnelable protocol) — both tracked as
  follow-ups.
