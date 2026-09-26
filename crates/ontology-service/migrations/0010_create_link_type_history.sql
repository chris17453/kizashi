-- Immutable snapshots of governed ontology relationship-type definitions.
CREATE TABLE link_type_history (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    link_type_id UUID NOT NULL,
    change_type VARCHAR(32) NOT NULL,
    actor VARCHAR(255) NOT NULL,
    before_state JSONB,
    after_state JSONB,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_link_type_history_lookup
    ON link_type_history (tenant_id, link_type_id, changed_at DESC);

INSERT INTO link_type_history
    (id, tenant_id, link_type_id, change_type, actor, before_state, after_state, changed_at)
SELECT md5(id::text || ':created')::uuid,
       tenant_id,
       id,
       'created',
       'system',
       NULL,
       jsonb_build_object(
           'name', name,
           'source_object_type_id', source_object_type_id,
           'target_object_type_id', target_object_type_id,
           'cardinality', cardinality,
           'properties_schema', properties_schema
       ),
       created_at
FROM link_types;
