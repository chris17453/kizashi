-- The Agent status/drill-down/search queries (stats_by_connector, list_by_connector, search)
-- all filter by tenant_id + connector_id and sort by ingested_at. The three single-column
-- indexes from 0001 force the planner into a bitmap AND across them instead of a single index
-- scan -- fine at today's data volumes, not fine at the "thousands of inboxes, hundreds of
-- connectors" scale this platform is meant to reach. One composite index covers all three
-- query shapes.
CREATE INDEX idx_raw_records_tenant_connector_ingested_at
    ON raw_records (tenant_id, connector_id, ingested_at DESC);
