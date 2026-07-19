-- A connector-opaque resume point (e.g. the highest IMAP UID seen), reported by the connector
-- itself via Connector::checkpoint and captured from its stdout by DockerInvoker (ADR-0034).
-- NULL means "no checkpoint yet" — the connector's static config (e.g. IMAP_SINCE_DATE) is
-- used as-is, same as before this migration.
ALTER TABLE agents ADD COLUMN last_checkpoint TEXT;
