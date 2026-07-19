-- Per-tenant AI provider/model configuration (ADR-0031), extending ADR-0019's prompt-only
-- config. provider defaults to 'azure_foundry' so every existing row keeps today's platform-
-- wide behavior unchanged. model/endpoint/api_key are only meaningful for 'openai_compatible'.
-- api_key is stored in plaintext, same as every other config-as-data field in this table — a
-- known, accepted interim posture (see ADR-0031's Decision section), not a silent gap.
ALTER TABLE analysis_configs
    ADD COLUMN provider TEXT NOT NULL DEFAULT 'azure_foundry',
    ADD COLUMN model TEXT,
    ADD COLUMN endpoint TEXT,
    ADD COLUMN api_key TEXT;
