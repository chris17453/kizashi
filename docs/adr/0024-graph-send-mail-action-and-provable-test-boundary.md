# ADR-0024: Graph send-mail-as-user action, and where its live-verification boundary sits

- **Status:** accepted
- **Date:** 2026-07-19

## Context

The third and final Phase 5 send action: sending an email as a real mailbox user via Microsoft
Graph's `POST /users/{id}/sendMail`, the roadmap's "cheapest of the three" since
`connector_runtime::fetch_access_token` (the Entra ID app-only client-credentials flow,
ADR-0003) already exists and is proven in production use by `graph-mail`/`graph-teams`. The
real design questions were small — routing shape (reuse ADR-0023's `RoutingActionDispatcher`
pattern or invent a new one) and, more importantly, **how far live verification can honestly
go without a real Entra tenant**, since this environment has no real Azure app registration to
test against.

## Decision

1. **New `GraphSendMailActionDispatcher`**, selected by `RoutingActionDispatcher` when an
   `ActionType::Email` action's config carries `graph_client_id` (checked before the plain-HTTP
   fallback, same shape as ADR-0023's `smtp_host` check for `SmtpActionDispatcher`; SMTP takes
   precedence if a config somehow carries both fields, since that's the more specific/older
   convention). Reads `graph_token_url`, `graph_client_id`, `graph_client_secret`,
   `graph_from_user_id`, optional `graph_base_url` (default
   `https://graph.microsoft.com/v1.0`), `to`, optional `subject` from the action config, fetches
   a token via `fetch_access_token` with scope `https://graph.microsoft.com/.default` (identical
   to `graph-mail`'s own scope), and POSTs the Graph `sendMail` payload
   (`{"message": {...}, "saveToSentItems": "false"}`).
2. **Live verification follows the exact same documented boundary
   `fabric_connector_integration_test.rs` already established (ADR-0013's note): the token
   fetch and HTTP request/response handling are tested against real stub servers (a real TCP
   connection, real HTTP request construction, real bearer-token attachment, real status-code
   branching for success/rejection/unreachable-token-endpoint), but the actual Microsoft Graph
   API surface itself is stubbed, not real** — this environment has no Entra app registration
   to test against, the same limitation ADR-0009 already documents for OIDC's browser hop.
   This is not a weaker test than the SMTP/IMAP actions' real-server verification — it's an
   honest reflection of what's provable without a customer's real Azure tenant, stated
   explicitly rather than glossed over.

## Consequences

- `RoutingActionDispatcher` now composes three dispatchers (`HttpActionDispatcher`,
  `SmtpActionDispatcher`, `GraphSendMailActionDispatcher`); each remains independently testable
  and none of their existing behavior changed by this addition.
- Known gap, not built here: a genuine end-to-end test against a real Microsoft 365 tenant is
  not possible in this environment and is explicitly out of scope — an operator deploying this
  against their own tenant is the actual first real-world validation, same as every other
  Graph-backed connector in this codebase.
