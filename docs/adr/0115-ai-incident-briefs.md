# ADR-0115: Governed AI incident briefs

- **Status:** accepted
- **Date:** 2026-07-24

## Context

Incident records can already regenerate a deterministic evidence brief, but operators still need
to manually turn event payloads into an impact and next-step narrative. The platform already owns
tenant AI provider configuration and fallback behavior in analysis-service; the Console must not
call an LLM directly or persist an unverified model response outside the incident audit path.

## Decision

Add an internal-secret-protected `POST /v1/incident-brief` endpoint to analysis-service. The
request is tenant-scoped through `X-Tenant-Id` and contains a bounded evidence object. The service
resolves the tenant's configured provider/model, uses the existing transient-failure fallback,
requires a concise text response, and rejects oversized or malformed results. The response is
returned to the Console, which persists it only through the existing audited incident update
contract.

If the provider is unavailable, the Console keeps the deterministic evidence brief instead. This
preserves operator visibility and avoids making incident lifecycle work depend on an external model.

## Consequences

AI briefs are available through the same tenant configuration and governance boundary as normal
analysis, while every saved summary remains part of the incident's immutable audit history. Model
availability is still deployment-specific; the deterministic fallback makes the feature safe for
local and disconnected environments.
