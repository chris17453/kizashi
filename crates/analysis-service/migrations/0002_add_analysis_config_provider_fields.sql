-- Mirrors config-admin-service's migration 0008 (ADR-0031): this service's local read-mostly
-- copy of AnalysisConfig needs the same columns so `analysis_config.changed` messages carrying
-- provider/model/endpoint/api_key round-trip through upsert/get without data loss.
ALTER TABLE analysis_configs
    ADD COLUMN provider TEXT NOT NULL DEFAULT 'azure_foundry',
    ADD COLUMN model TEXT,
    ADD COLUMN endpoint TEXT,
    ADD COLUMN api_key TEXT;
