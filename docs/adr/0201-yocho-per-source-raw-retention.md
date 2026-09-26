# ADR-0201: Per-source raw retention for Yochō sources

- **Status:** proposed (default adopted for Phase 0; reversible)
- **Date:** 2026-09-26

## Context

Kizashi spec §2.6 says raw data is retained so logic can be replayed. Today `raw_records` has
`tenant_id`, and `retention-service` enforces `ttl_days` per (tenant, data_class), archiving to
gzipped NDJSON on S3/Blob before deleting. With no policy row, raw is kept forever.

Yochō wants raw mail content held only in a TTL buffer and purged after extraction, to bound PII
and storage, while signals are kept forever. The plan itself recommends a per-source policy.

## Decision

1. Extend retention policy scope from (tenant, data_class) to (tenant, data_class, optional
   `source_type`/`connector_id`). The most specific matching policy wins.
2. Add a `purge_without_archive` flag on a policy. When set, the sweep hard-deletes expired rows
   without writing an archive. This is how the Yochō mail-content buffer works: raw transport
   mail is purged on TTL and not archived.
3. **Raw retention is operator-configurable, never hard-coded.** TTL, scope (tenant, data class,
   source) and the archive/purge choice are all set per tenant in the admin console and API,
   versioned and audit-logged like every other retention policy change. The shipped default for
   the Yochō raw buffer is **90 days**; tenants can raise or lower it. Changing it takes effect on
   the next sweep with no redeploy.
4. Every other source keeps Kizashi's default: archive then delete, with reimport.
5. The TTL buffer *is* `raw_records`. No second raw store is introduced.
6. Deleting a raw row must never delete signals, skeletons or lineage metadata derived from it
   (ADR-0205, ADR-0206). Lineage records that the raw content is gone, not a dangling pointer.

## Consequences

- Raw replay (workbench), labeling and re-clustering are bounded by the configured per-source TTL
  (90 days by default), as the plan expects. The console shows each tenant's effective window so
  analysts know how far raw replay reaches.
- Every retention policy change is audit-logged, as today; the new columns are part of that row.
- Compliance holds still override TTL; a held mail record is not purged.
