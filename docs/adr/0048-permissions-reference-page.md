# 0048. Permissions reference page

## Context

RBAC (ADR-0016) has three tiers — Viewer, Operator, Admin — enforced independently by every
backend service's own `role.at_least(min)` checks. There was no single place documenting what
each tier actually grants; answering "what can an Operator do" required reading source across
five services area by area. An enterprise buyer or auditor evaluating access control expects a
written permission matrix, not an invitation to read Rust source.

A full audit of every role check across config-admin-service, retention-service, egress-gateway,
auth-service, and ingestion-gateway (plus the Console UI's own client-side mirrors) was run to
build this page's content, so it reflects what the code actually enforces rather than an
aspirational design. Two notable findings from that audit, both reflected in the page:

- **Users/RBAC and Active Sessions are the only areas where even *viewing* requires Admin** —
  every other area follows "Viewer can read, Operator can also write."
- **Audit Log and Saved Searches have no role restriction at all** — any authenticated tenant
  member, including Viewer, has full read (and for Saved Searches, write) access, by explicit
  design (documented in existing code comments, e.g. `config-admin-service/src/handlers.rs`'s
  `get_recent_audit_log`: "any authenticated tenant member may view it").

The same audit also found a real bug, fixed alongside this page (see the fix's own commit): AI
Analysis Config's `GET /v1/analysis-config` returned the configured provider's API key
unredacted to any authenticated role, including Viewer.

## Decision

Add `GET /security/permissions`: a static reference table, one row per functional area (Sensors,
Triggers, Field Mappings, Retention Policies, Egress Allowlist, AI Analysis Config, API Keys,
Users/RBAC, Audit Log, Active Sessions, Branding, Saved Searches), showing what Viewer/Operator/
Admin can do in each, transcribed directly from the audit above. No backend calls — pure static
content, viewable by any authenticated session (matching the "reference material," not
"privileged data," nature of the page itself).

## Consequences

- This table can drift from the code if a future PR changes what a role can do without updating
  it — there's no automated check tying the two together. The page's own header text says so
  explicitly ("if this table and the running system ever disagree, the running system is
  right"), and any PR that changes a role gate should update this table in the same PR, the same
  discipline already expected of audit-log-writing config changes (CLAUDE.md §5).
- No new backend endpoints, schema, or client code — the entire feature is one new UI page.
