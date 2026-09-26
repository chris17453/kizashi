CREATE TABLE object_annotations (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    object_id UUID NOT NULL REFERENCES objects(id),
    author VARCHAR(255) NOT NULL,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_object_annotations_object ON object_annotations (tenant_id, object_id, created_at DESC);
