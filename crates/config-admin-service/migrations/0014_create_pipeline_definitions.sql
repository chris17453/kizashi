CREATE TABLE pipeline_definitions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    data_source_id UUID NOT NULL,
    target_object_type_id UUID,
    mode TEXT NOT NULL,
    steps JSONB NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name),
    FOREIGN KEY (data_source_id) REFERENCES data_sources(id)
);

CREATE INDEX idx_pipeline_definitions_tenant ON pipeline_definitions (tenant_id);
CREATE INDEX idx_pipeline_definitions_data_source ON pipeline_definitions (tenant_id, data_source_id);
