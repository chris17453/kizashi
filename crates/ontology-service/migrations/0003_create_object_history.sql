-- Immutable object snapshots for ontology investigation history.
CREATE TABLE object_history (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    object_id UUID NOT NULL,
    change_type VARCHAR(32) NOT NULL,
    actor VARCHAR(255) NOT NULL,
    before_state JSONB,
    after_state JSONB,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_object_history_object ON object_history (tenant_id, object_id, changed_at DESC);

-- Give existing modeled objects a truthful starting point instead of making the history panel
-- look empty merely because they predate this migration.
INSERT INTO object_history (id, tenant_id, object_id, change_type, actor, before_state, after_state, changed_at)
SELECT md5(id::text || ':created')::uuid,
       tenant_id,
       id,
       'created',
       'system',
       NULL,
       jsonb_build_object('object_type_id', object_type_id, 'properties', properties, 'source_lineage', source_lineage),
       created_at
FROM objects;
