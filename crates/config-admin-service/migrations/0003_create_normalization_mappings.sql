-- NormalizationMapping config store, owned by Config/Admin Service (spec §5.6, ADR-0010). Not
-- yet read by Normalization Service, which still interim-owns its own copy (ADR-0010).
CREATE TABLE normalization_mappings (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    source_type TEXT NOT NULL,
    field_map JSONB NOT NULL,
    version INTEGER NOT NULL
);

CREATE INDEX idx_normalization_mappings_tenant_source
    ON normalization_mappings (tenant_id, source_type, version DESC);
