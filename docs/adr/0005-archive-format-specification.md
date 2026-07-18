# ADR-0005: Archive format specification

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §11 flags "Archive format specification (exact schema for replayable archived records)"
as a sprint-0 open item. Spec §9 (Data Lifecycle) requires archived data to be moved to Azure
Blob or AWS S3 "in a self-describing, replayable format (must be re-ingestible through the
same pipeline)," and reimport must "preserve enough fidelity to replay the full pipeline"
(ingestion → normalization → analysis). Retention/Archival Service (spec §6, service #12) is
the only writer/reader of this format.

Because `RawRecord`'s schema is intentionally stable and generic (spec §5.1, "schema-on-read
for ingestion" — spec §2 principle 2), the archive format's natural shape is a direct,
lossless serialization of `RawRecord` rows, not a bespoke export schema that would need its own
versioning discipline separate from the live schema.

## Decision

Archived data is written as **newline-delimited JSON (NDJSON), one `RawRecord` per line**,
gzip-compressed, batched into files by tenant and time window:
`archive/<tenant_id>/<data_class>/<yyyy>/<mm>/<dd>/<batch_id>.ndjson.gz`, where `data_class` is
`raw` (RawRecord rows) — normalized_payload and analysis results, if retained past their own
policy window, travel as fields already embedded on the `RawRecord` row rather than as
separate archive files, since `RawRecord.normalized_payload` already carries that fidelity per
spec §5.1.

Each archive file carries a manifest header as its first line (a `RawRecord`-shaped envelope
is not used for the manifest — it is a distinct one-line JSON object) recording:
`format_version`, `tenant_id`, `data_class`, `record_count`, `window_start`/`window_end`, and
the `common` crate schema version the records were serialized with. `format_version` lets
Retention/Archival Service evolve the archive envelope later without breaking readers of
already-archived files — reimport dispatches on this field.

Reimport re-feeds each `RawRecord` line back through the Ingestion Gateway's normal ingestion
path (spec §3), re-triggering normalization → analysis exactly as a live-polled record would,
which is what "replayable" requires — archived records are not special-cased downstream of
ingestion.

## Consequences

- Easier: no second schema to keep in sync with `RawRecord` — archival is "serialize what we
  already have," so any `common` crate change that stays backward-compatible (spec's own
  requirement that RawRecord's schema not change) automatically stays archive-compatible;
  NDJSON+gzip is streamable for both writing (no need to hold a full window in memory) and
  reading during reimport.
- Harder: because normalized/analysis fields live embedded on the archived `RawRecord`, a
  tenant that wants to retain raw data but discard analysis results earlier needs
  Retention/Archival Service to strip those fields before archiving that row, rather than
  deleting a separate archive file — this is a field-level redaction step in the archival
  write path, tracked as part of that service's build-out (task #11), not a gap in this format.
