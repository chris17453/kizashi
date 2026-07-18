# ADR-0003: Fabric/OneLake connector auth flow

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §11 flags "Fabric API/OneLake connector auth flow details (Entra-backed, exact permission
scopes)" as a sprint-0 open item. Spec §6 lists two distinct Fabric connectors: `fabric:sql:
<dataset>` (Fabric SQL analytics endpoint) and `fabric:onelake:<path>` (OneLake file API), both
under service #1 (Connectors/Agents). Both are Microsoft Entra ID-backed, consistent with Auth
Service already treating Entra as a first-class OIDC provider (spec §4, §8) — reusing one Entra
app registration pattern for both the platform's own SSO and its Fabric data access avoids
maintaining two separate Microsoft identity integrations.

Kizashi is multi-tenant and resold (spec §1); each tenant's Fabric/OneLake data lives in
*their* Azure tenant, not Kizashi's. A single shared service principal cannot be given standing
access to every customer's Fabric workspace — that would mean Kizashi requesting broad
delegated permissions across tenants it doesn't operate, which no security-conscious customer
would approve, and which violates spec §8's tenant-isolation posture.

## Decision

Each tenant's Fabric/OneLake connector config (spec §11 "config over code") stores its own
Entra **app registration client credentials** (client ID + client secret, or certificate) that
the *customer* provisions in their own Entra tenant and grants explicit Fabric
workspace/OneLake permissions to — the OAuth2 client-credentials (app-only) flow, not a
delegated/user-flow auth. Kizashi never uses a platform-wide service principal against
customer Fabric tenants.

Minimum required scopes, requested per connector at config time and validated by
Config/Admin Service before the connector is enabled:
- Fabric SQL endpoint: `https://analysis.windows.net/powerbi/api/.default`, scoped to the
  target workspace/dataset via a Fabric workspace-level role assignment (Viewer minimum).
- OneLake: `https://storage.azure.com/.default`, scoped to the specific OneLake path granted
  to the app registration (least privilege — the connector should only see the paths it is
  configured to poll, not the whole tenant's OneLake).

Client secrets are stored the same way as every other per-tenant credential: in the platform's
secret store, referenced by config, never inlined in connector-config JSON and never logged
(CLAUDE.md §5 "No secrets in code or commits" applies to runtime config as much as to source).

## Consequences

- Easier: each customer's Fabric access is independently auditable and revocable from their
  own Entra tenant (they can pull the app registration's grant at any time without contacting
  Kizashi); matches how the rest of the platform already treats Entra as first-class auth.
- Harder: onboarding a Fabric/OneLake connector requires a manual step in the customer's own
  Azure tenant (creating the app registration, granting workspace access) before the connector
  can poll — this is inherent to per-tenant isolation and is documented as a config-admin
  onboarding runbook step rather than automated away by weakening the isolation model.
