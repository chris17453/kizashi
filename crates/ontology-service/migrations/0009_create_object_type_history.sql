-- Immutable snapshots of governed ontology object-type definitions.
CREATE TABLE object_type_history (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    object_type_id UUID NOT NULL,
    change_type VARCHAR(32) NOT NULL,
    actor VARCHAR(255) NOT NULL,
    before_state JSONB,
    after_state JSONB,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_object_type_history_lookup
    ON object_type_history (tenant_id, object_type_id, changed_at DESC);

INSERT INTO object_type_history
    (id, tenant_id, object_type_id, change_type, actor, before_state, after_state, changed_at)
SELECT md5(id::text || ':created')::uuid,
       tenant_id,
       id,
       'created',
       'system',
       NULL,
       jsonb_build_object(
           'name', name,
           'version', version,
           'property_schema', property_schema,
           'mapping_rules', mapping_rules
       ),
       created_at
FROM object_types;
