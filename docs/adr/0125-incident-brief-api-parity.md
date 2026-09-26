# ADR-0125: Incident brief API parity

## Status

Accepted

## Context

The Console could regenerate an incident brief from linked evidence, including the bounded AI
provider fallback, but external operator tooling had no equivalent versioned API operation.

## Decision

Add `POST /api/v1/incidents/:id/brief`. The endpoint requires an operator principal, re-reads the
incident within the principal's tenant, fetches up to 100 linked events through the query client,
generates the same deterministic evidence brief (or configured AI brief with deterministic
fallback), and persists the summary through the audited Incident Service update contract.

## Consequences

- Automation and external operator shells can refresh case briefs without scraping HTML.
- Tenant and role boundaries match the browser workflow.
- The endpoint returns the updated incident detail; missing linked events are omitted while query
  failures fail closed with a gateway error.
