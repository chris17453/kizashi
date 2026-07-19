-- Idempotent ingestion (real-world gap): a connector that re-scans an overlapping window on
-- every poll (e.g. IMAP's date-only SINCE search) would otherwise create a new RawRecord — and
-- a new downstream Event/trigger fire — for the same source item every single poll cycle.
-- external_id lets a connector supply a source-stable identifier (email Message-ID, ticket
-- number, ...); the partial unique index makes re-ingesting the same external_id a no-op
-- instead of a duplicate row. Partial (WHERE external_id IS NOT NULL) so connectors with no
-- natural stable id are unaffected.
ALTER TABLE raw_records ADD COLUMN external_id TEXT;

CREATE UNIQUE INDEX raw_records_tenant_connector_external_id_idx
    ON raw_records (tenant_id, connector_id, external_id)
    WHERE external_id IS NOT NULL;
