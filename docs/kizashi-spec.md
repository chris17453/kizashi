# Kizashi — Enterprise Insights & Action Platform
## Architecture & Product Specification v1.0

---

## 1. Overview

Kizashi is an enterprise data-ingestion, analysis, and automated-action platform. It ingests
structured and unstructured data from arbitrary sources (email, chat, ticketing systems, logs,
databases, data lakes), normalizes it into a common event-oriented model, applies AI/ML-driven
analysis, and lets operators define triggers that fire automated actions when patterns emerge
(e.g. "3 emails from the same customer indicating a missed follow-up," "2 tickets with negative
sentiment from the same account"). Results are surfaced through configurable, dashboards.

Kizashi is designed to be **white-labelable and multi-tenant** — it may be resold to other
companies, so branding/theming, tenancy isolation, and generic connector configuration are
first-class requirements, not afterthoughts.

**Name origin:** Kizashi — a sign, omen, or early indication that something is about to happen.
Fits a platform whose job is detecting early signals in data and acting on them.

**License:** MIT.

---

## 2. Design Principles

1. **API-mediated everything.** No component reads another component's database directly.
   Agents talk to a gateway. The UI talks to a gateway. Internal services talk through
   well-defined APIs or the message bus.
2. **Schema-on-read for ingestion.** The raw data store's schema never changes when a new
   source type is added. Structure is imposed downstream, not at ingest.
3. **No vendor lock-in.** Every dependency (queue, databases, blob storage, auth) must run
   in a container on either Azure or AWS. No cloud-native-only managed services baked into
   the core architecture.
4. **Decoupled stages.** Ingestion, normalization, analysis, aggregation/triggering, and
   action execution are independent, asynchronously connected services — not a single
   synchronous pipeline. Any stage can be scaled, retried, or replaced independently.
5. **Config over code.** Connector mappings, event type schemas, trigger definitions,
   retention policies, and branding are all data, editable by an operator, not code changes.
6. **Everything replayable.** Raw data is retained (subject to policy) so normalization and
   analysis logic can be improved and reprocessed without re-hitting source systems.

---

## 3. High-Level Data Flow

```
 [Connectors/Sensors]              (CronJob-scheduled pollers)
        |
        v
 [Ingestion Gateway] -----------> [Ingestion Service] --> Raw Store (Postgres)
        |                                 |
        |                          publishes "record.ingested" (RabbitMQ)
        |                                 v
        |                       [Normalization Service] --> updates normalized_payload
        |                                 |
        |                          publishes "record.normalized"
        |                                 v
        |                        [Analysis Service] (Azure AI Foundry / Azure ML)
        |                                 |
        |                          publishes "record.analyzed"
        |                                 v
        |                  [Aggregation / Trigger Engine]  (own schedule, decoupled)
        |                                 |
        |                          writes Event --> Aggregate Store (ClickHouse)
        |                          publishes "event.created"
        |                                 v
        |                        [Action Executor] --> executes triggered actions
        |                                 |                (email, webhook, Teams, ticket)
        |                          writes to Events/Audit table (Postgres)
        |
 [Query Gateway] <--------------- [Dashboard/Query API] <--- reads Aggregate + Events stores
        ^
        |
   [Console UI]
```

---

## 4. Tech Stack

| Layer                 | Choice                                                          |
|-----------------------|-----------------------------------------------------------------|
| Application code      | Rust                                                            |
| UI                    | Rust-based React-equivalent framework                           |
| Message bus           | RabbitMQ                                                        |
| Hot / live store      | Postgres                                                        |
| Events / audit store  | Postgres (append-only table, event-sourcing style)              |
| Aggregate / analytics | ClickHouse                                                      |
| Blob / archival       | Azure Blob Storage and AWS S3 (both supported)                  |
| AI/ML analysis        | Azure AI Foundry, Azure ML                                      |
| Auth                  | Entra ID (OIDC), local (Postgres, hashed credentials), generic OAuth (any OIDC provider) |
| Containers            | Docker (local dev via Docker Compose)                           |
| Managed deploy        | Azure Container Apps (or equivalent) first                      |
| Orchestrated deploy   | Kubernetes (AKS/EKS) later, via Helm charts                     |

All datastores and the message bus are self-hosted in containers — no managed-service-only
dependency, to preserve Azure/AWS portability and avoid licensing/managed-service premiums.

---

## 5. Core Data Model

### 5.1 RawRecord (Ingestion tier — Postgres)

The stable, generic envelope every connector writes to, regardless of source type. This schema
must not change as new source types are added.

```
RawRecord {
  id: UUID
  connector_id: string        // "zendesk" | "graph:teams" | "graph:mail" |
                               // "sql:<connection_id>" | "fabric:sql:<dataset>" |
                               // "fabric:onelake:<path>" | "generic:<id>"
  source_type: enum           // message | ticket | log | sql_row | fabric_record | generic
  ingested_at: timestamp
  occurred_at: timestamp?     // nullable — not all sources carry a clean event time
  raw_payload: JSONB          // untouched, source-native shape
  normalized_payload: JSONB?  // populated by Normalization Service
  tenant_id: UUID             // multi-tenancy
}
```

### 5.2 Event (Aggregate tier — ClickHouse)

```
Event {
  id: UUID
  tenant_id: UUID
  event_type: string          // references EventTypeDefinition
  source_connector_ids: [string]
  entity_ref: string          // e.g. customer id, thread id
  group_key: string           // dimension used for clustering
  payload: JSON                // structured detail (scores, references to raw records)
  occurred_at: timestamp
  created_at: timestamp
  status: enum                // new | triggered | actioned | dismissed
}
```

### 5.3 EventTypeDefinition (Config store — Postgres)

```
EventTypeDefinition {
  id: UUID
  tenant_id: UUID
  name: string                 // e.g. "sentiment.negative"
  field_schema: JSON schema    // defines shape of Event.payload for this type
  version: int
}
```

### 5.4 TriggerDefinition

```
TriggerDefinition {
  id: UUID
  tenant_id: UUID
  name: string
  event_type_match: string
  condition: JSON/DSL          // e.g. count >= 3 within group_key over window
  window: duration
  actions: [ActionRef]
  enabled: boolean
}
```

### 5.5 Action / Action Execution Log (Events/audit tier — Postgres, append-only)

```
ActionExecution {
  id: UUID
  trigger_id: UUID
  event_id: UUID
  action_type: enum            // email | webhook | teams_alert | create_ticket | custom
  status: enum                 // pending | sent | failed | retried
  executed_at: timestamp
  detail: JSON
}
```

### 5.6 NormalizationMapping (Config store)

```
NormalizationMapping {
  id: UUID
  tenant_id: UUID
  source_type: string
  field_map: JSON               // e.g. { "text": "$.description", "entity_ref": "$.requester_id" }
  version: int
}
```

---

## 6. Services (Deployable Units)

| # | Service                     | Responsibility |
|---|------------------------------|-----------------|
| 1 | Connectors/Agents            | Poll sources (Zendesk, MS Graph/Teams, MS Graph/Mail, Direct SQL, Fabric SQL endpoint, Fabric API/OneLake, Generic REST reader); push to Ingestion Gateway |
| 2 | Ingestion Gateway             | Single agent-facing entry point; service auth, rate limiting, routing |
| 3 | Ingestion Service             | Validates/persists RawRecord; publishes `record.ingested` |
| 4 | Normalization Service         | Applies NormalizationMapping config; produces normalized_payload; publishes `record.normalized` |
| 5 | Analysis Service              | Calls Azure AI Foundry/ML; writes analysis results; publishes `record.analyzed` |
| 6 | Aggregation/Trigger Engine    | Scheduled, decoupled; groups by group_key; evaluates TriggerDefinitions; writes Events; publishes `event.created` |
| 7 | Action Executor               | Consumes matched events; executes actions; writes ActionExecution audit rows |
| 8 | Query Gateway                 | Single dashboard/UI-facing entry point; user auth enforcement |
| 9 | Dashboard/Query API Service   | Reads ClickHouse + Postgres; serves dashboards, reports, event browsing |
| 10 | Auth Service                 | Entra OIDC, local login, generic OAuth; issues sessions/tokens |
| 11 | Config/Admin Service          | Manages connector configs, normalization mappings, event type definitions, trigger definitions, retention policy, branding/theming |
| 12 | Retention/Archival Service    | Enforces retention policy; moves aged data to Blob/S3; supports reimport |
| 13 | Platform Observability        | Metrics/health for all services; pipeline backlog/lag visibility |

---

## 7. Console / UI Requirements

Styled after Red Hat OpenShift / Instana enterprise consoles: dark-mode-first design language,
with full light mode support and per-tenant branding/theming (logo, color scheme).

- **Console shell:** left nav, tenant/project switcher, global search
- **Topology view:** visualize connector → pipeline stage → event → action as a live graph
- **Configurable dashboards:** widget-based, drag/arrange, per-user or per-team, bound to
  saved queries or event types
- **Reporting:** scheduled report generation (PDF/email), recurring, saved queries/views
- **Event type management UI:** create/edit/version event types, define field schemas, map
  source fields to event type fields
- **Trigger builder UI:** define conditions, windows, actions; dry-run/test mode
- **Data lifecycle UI:** configure retention per data class (raw/normalized/events),
  archival destination, disposal rules, compliance holds, and reimport-from-archive
- **RBAC/admin UI:** roles/permissions per tenant, audit log of configuration changes
- **API surface:** every console capability is also available as a REST API, with API
  keys/service accounts for external integration

---

## 8. Multi-Tenancy & Security

- Tenant isolation enforced at the data layer (`tenant_id` on every row) and at the
  gateway layer (auth context scopes all downstream queries)
- RBAC: roles/permissions scoped per tenant, OpenShift-project-style
- Audit log: all admin/config changes (trigger edits, mapping changes, retention policy
  changes) recorded immutably
- Auth: Entra ID (OIDC) as first-class module, local login (Postgres, hashed credentials —
  bcrypt/argon2), and generic OAuth (config-driven, any OIDC-compliant provider) to support
  resale to companies not on Microsoft identity

---

## 9. Data Lifecycle

- **Retention:** configurable TTL per data class (raw, normalized, event/audit)
- **Archival:** aged data moved to Azure Blob or AWS S3 in a self-describing, replayable
  format (must be re-ingestible through the same pipeline)
- **Disposal:** hard delete after retention period, with compliance-hold override
- **Reimport:** archived data can be re-fed through ingestion → normalization → analysis,
  meaning archive format must preserve enough fidelity to replay the full pipeline

---

## 10. Deployment Path

1. **Local development/testing:** Docker Compose — all 13 services, RabbitMQ, Postgres,
   ClickHouse running locally for dev and integration testing
2. **Managed container environment:** Azure Container Apps (or AWS equivalent) as the
   first production target — no Kubernetes-only primitives assumed at this stage
3. **Kubernetes:** AKS/EKS with Helm charts, once the platform needs orchestration-level
   features (autoscaling, advanced scheduling, multi-region)

Containers must run standalone at every stage — nothing in the base image/config assumes
a Kubernetes control plane is present until the Helm stage.

---

## 11. Open Items for Development Kickoff

These are flagged as needing a decision during sprint 0, not yet resolved in this spec:

- Condition DSL for triggers: full expression language vs. fixed condition "shapes"
  (count-over-window, threshold-over-window) for v1
- Fabric API/OneLake connector auth flow details (Entra-backed, exact permission scopes)
- Analysis Service invocation pattern: synchronous per-record calls to Foundry/ML vs.
  batched invocation
- Repository layout: single mono-repo (13 Rust crates/workspaces) vs. per-service repos
- Archive format specification (exact schema for replayable archived records)

---

*End of v1.0 specification.*
