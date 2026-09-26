# ADR-0186: Fresh JSON export for the compliance snapshot

## Status

Accepted — 2026-07-25

## Context

The Compliance Snapshot page already assembled an auditor-facing control
summary and the authenticated versioned API already exposed the same
evidence-backed posture as JSON. The browser surface only offered printing,
which made it harder to hand the exact current control state to governance
automation or preserve a structured audit artifact.

## Decision

Add an explicit `Export fresh JSON` action to the admin-only compliance page.
The browser fetches `/api/v1/security/compliance-report` with the current
session and `cache: no-store`, creates a dated JSON download, and reports
success or failure in the page without navigating away. The API remains the
source of truth for tenant scope, authorization, metrics, control states, and
degradation errors.

## Consequences

Auditors and operators can preserve a structured point-in-time evidence bundle
without screen scraping or relying on browser print output. The export is
fresh at click time and carries the API's explicit errors, while the existing
printable snapshot remains available for human review.
