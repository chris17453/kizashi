-- Makes the Data Viewer's free-text search a real search index instead of a full sequential
-- scan (task: "real search index"). The `search()` query's ILIKE '%pattern%' predicates
-- against raw_payload::text/subject/from/attachment filenames can't use a plain B-tree index
-- (leading wildcard), and the existing GIN index from 0003 only accelerates JSONB
-- containment/key-existence checks, not substring matches. pg_trgm's trigram GIN index is the
-- standard Postgres answer for indexing ILIKE '%x%': same exact matching semantics as today
-- (no behavior change, purely a scan-strategy change the planner picks up once the table is
-- large enough to prefer it over a seq scan -- same "useless at demo scale" caveat as 0003).
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX idx_raw_records_payload_text_trgm
    ON raw_records USING GIN ((raw_payload::text) gin_trgm_ops);

CREATE INDEX idx_raw_records_subject_trgm
    ON raw_records USING GIN ((raw_payload ->> 'subject') gin_trgm_ops);

CREATE INDEX idx_raw_records_from_trgm
    ON raw_records USING GIN ((raw_payload ->> 'from') gin_trgm_ops);
