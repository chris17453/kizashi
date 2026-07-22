-- Ontology Layer Tables: object_type, object, link_type, link, action_type, action_invocation

-- Object Types
CREATE TABLE object_types (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    name VARCHAR(255) NOT NULL,
    version INT NOT NULL DEFAULT 1,
    property_schema JSONB NOT NULL,
    mapping_rules JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, name, version)
);

-- Objects
CREATE TABLE objects (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    object_type_id UUID NOT NULL REFERENCES object_types(id),
    properties JSONB NOT NULL,
    source_lineage JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_objects_tenant_type ON objects (tenant_id, object_type_id);

-- Link Types
CREATE TABLE link_types (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    name VARCHAR(255) NOT NULL,
    source_object_type_id UUID NOT NULL REFERENCES object_types(id),
    target_object_type_id UUID NOT NULL REFERENCES object_types(id),
    cardinality VARCHAR(50) NOT NULL,
    properties_schema JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, name)
);

-- Links
CREATE TABLE links (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    link_type_id UUID NOT NULL REFERENCES link_types(id),
    source_object_id UUID NOT NULL REFERENCES objects(id),
    target_object_id UUID NOT NULL REFERENCES objects(id),
    properties JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, link_type_id, source_object_id, target_object_id)
);
CREATE INDEX idx_links_source ON links (tenant_id, source_object_id);
CREATE INDEX idx_links_target ON links (tenant_id, target_object_id);

-- Action Types
CREATE TABLE action_types (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    name VARCHAR(255) NOT NULL,
    parameter_schema JSONB NOT NULL,
    preconditions JSONB NOT NULL,
    effect_definition JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, name)
);

-- Action Invocations (audit)
CREATE TABLE action_invocations (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    action_type_id UUID NOT NULL REFERENCES action_types(id),
    target_object_ids JSONB NOT NULL,
    parameters JSONB NOT NULL,
    outcome VARCHAR(50) NOT NULL,
    triggering_event_ref JSONB NOT NULL,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_action_invocations_tenant ON action_invocations (tenant_id);
