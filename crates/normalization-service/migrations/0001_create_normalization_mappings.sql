-- NormalizationMapping config store (spec §5.6). Versioned per tenant/source_type; the
-- highest version row for a given (tenant_id, source_type) is the active mapping.
CREATE TABLE normalization_mappings (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    source_type TEXT NOT NULL,
    field_map JSONB NOT NULL,
    version INTEGER NOT NULL
);

CREATE INDEX idx_normalization_mappings_tenant_source
    ON normalization_mappings (tenant_id, source_type, version DESC);
