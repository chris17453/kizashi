-- Structured email search (subject/from/attachment-filename, common::EmailPayload's documented
-- raw_payload shape) reaches into raw_payload's top-level keys and its attachments array.
-- A GIN index over the whole JSONB column lets Postgres use an index scan for containment/
-- existence checks on those keys instead of evaluating every row's raw_payload -- necessary at
-- the "thousands of inboxes, hundreds of connector APIs" scale this platform targets, useless
-- at today's demo-scale row counts (the planner just won't pick it until the table is large).
CREATE INDEX idx_raw_records_raw_payload_gin ON raw_records USING GIN (raw_payload);
