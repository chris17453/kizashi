# ADR-0208: Late-binding entity attribution for Yochō signals

- **Status:** proposed (default adopted for Phase 0; reversible)
- **Date:** 2026-09-26

## Context

Every Yochō signal must resolve to a customer and BU. The plan lists three options (bake
`customer_id` at write, late binding via a versioned mapping joined at query time, or a hybrid
with correction events) and recommends late binding. Phase 0's exit criterion requires signals to
be "attributed". Kizashi's ontology derives object ids by hashing an exact identity field; there
is no fuzzy matching or review queue.

## Decision

1. Signals store **source identities**, not customers: `entity_type` + `entity_id` are the thing
   the extractor saw (email address, domain, Zendesk org, ERP customer number).
2. The entity resolver maintains a **versioned identity → customer/BU mapping** in Postgres
   (append-only versions; each change is an audit row). It is exported to ClickHouse as a
   dictionary; queries and rollups join through it.
3. Fixing a match therefore heals all history on the next dictionary refresh. Scores and alerts
   record the mapping version they used, so past decisions remain explainable.
4. Customers and BUs are modelled as Kizashi ontology object types (`customer`, `business_unit`,
   `contact`), so they appear in the ontology workbench; Yochō adds the matching tiers and review
   queue on top.

## Consequences

- Queries pay a dictionary join; rollups keyed by customer must be recomputed from identity-keyed
  rollups, so rollups are keyed by identity, and customer views aggregate at query time.
- A dictionary refresh interval (default 60 s) bounds how quickly a correction shows.
