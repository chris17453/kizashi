#!/usr/bin/env bash
# Seeds a demo tenant, local Console UI users for each RBAC persona, and a connector API key, so a freshly-launched
# stack (scripts/run-local.sh) is immediately usable instead of requiring hand-written SQL —
# the same gap every manual smoke test this project has relied on so far had to work around.
#
# Talks to Postgres directly via `docker compose exec` (not through any service's HTTP API —
# there is no self-service API-key/user-provisioning endpoint yet, a gap this script works
# around rather than papering over). Safe to re-run — uses ON CONFLICT DO NOTHING throughout.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

# Fixed, not random — a throwaway local-only demo credential (never leaves localhost, backed
# by a local docker-compose Postgres), not a real secret, so a stable value is more useful here
# than a regenerated one: re-running this script against the same local Postgres always logs
# into the same demo tenant with the same credentials instead of accumulating a fresh set.
DEMO_PASSWORD="kizashi-local-demo-password"
OPERATOR_PASSWORD="kizashi-local-operator-password"
VIEWER_PASSWORD="kizashi-local-viewer-password"
API_KEY="kizashi-local-demo-api-key"
DEMO_TENANT_FILE="run/demo-tenant.env"
TENANT_ID="00000000-0000-0000-0000-000000000001"
TENANT_NAME="acme"
USER_ID="00000000-0000-0000-0000-000000000002"
OPERATOR_USER_ID="00000000-0000-0000-0000-000000000004"
VIEWER_USER_ID="00000000-0000-0000-0000-000000000005"
KEY_ID="00000000-0000-0000-0000-000000000003"
CUSTOMER_TYPE_ID="00000000-0000-0000-0000-000000000010"
TICKET_TYPE_ID="00000000-0000-0000-0000-000000000011"
SUPPORT_TEAM_TYPE_ID="00000000-0000-0000-0000-000000000012"
CUSTOMER_OBJECT_ID="00000000-0000-0000-0000-000000000020"
TICKET_OBJECT_ID="00000000-0000-0000-0000-000000000021"
CUSTOMER_OBJECT_ID_2="00000000-0000-0000-0000-000000000022"
CUSTOMER_OBJECT_ID_3="00000000-0000-0000-0000-000000000023"
TICKET_OBJECT_ID_2="00000000-0000-0000-0000-000000000024"
TICKET_OBJECT_ID_3="00000000-0000-0000-0000-000000000025"
TICKET_OBJECT_ID_4="00000000-0000-0000-0000-000000000026"
SUPPORT_TEAM_OBJECT_ID="00000000-0000-0000-0000-000000000027"
LINK_TYPE_ID="00000000-0000-0000-0000-000000000030"
LINK_ID="00000000-0000-0000-0000-000000000031"
LINK_ID_2="00000000-0000-0000-0000-000000000032"
LINK_ID_3="00000000-0000-0000-0000-000000000033"
LINK_ID_4="00000000-0000-0000-0000-000000000034"
SUPPORT_LINK_TYPE_ID="00000000-0000-0000-0000-000000000035"
SUPPORT_LINK_ID="00000000-0000-0000-0000-000000000036"
SUPPORT_LINK_ID_2="00000000-0000-0000-0000-000000000037"
SUPPORT_LINK_ID_3="00000000-0000-0000-0000-000000000038"
ACTION_TYPE_ID="00000000-0000-0000-0000-000000000040"
ACTION_TYPE_ID_2="00000000-0000-0000-0000-000000000044"
ACTION_TYPE_ID_3="00000000-0000-0000-0000-000000000045"
INVOCATION_ID="00000000-0000-0000-0000-000000000041"
INVOCATION_ID_2="00000000-0000-0000-0000-000000000042"
INVOCATION_ID_3="00000000-0000-0000-0000-000000000043"
INVOCATION_ID_4="00000000-0000-0000-0000-000000000046"
INVOCATION_ID_5="00000000-0000-0000-0000-000000000047"
SENSOR_ID="00000000-0000-0000-0000-000000000050"
SENSOR_ID_2="00000000-0000-0000-0000-000000000051"
SENSOR_ID_3="00000000-0000-0000-0000-000000000052"
SENSOR_ID_4="00000000-0000-0000-0000-000000000053"
SENSOR_ID_5="00000000-0000-0000-0000-000000000054"
SENSOR_ID_6="00000000-0000-0000-0000-000000000055"
RECORD_ID="00000000-0000-0000-0000-000000000100"
RECORD_ID_2="00000000-0000-0000-0000-000000000101"
RECORD_ID_3="00000000-0000-0000-0000-000000000102"
RECORD_ID_4="00000000-0000-0000-0000-000000000103"
RECORD_ID_5="00000000-0000-0000-0000-000000000104"
RECORD_ID_6="00000000-0000-0000-0000-000000000105"
RECORD_ID_7="00000000-0000-0000-0000-000000000106"
RECORD_ID_8="00000000-0000-0000-0000-000000000107"
RECORD_ID_9="00000000-0000-0000-0000-000000000108"
RECORD_ID_10="00000000-0000-0000-0000-000000000109"
RECORD_ID_11="00000000-0000-0000-0000-000000000110"
RECORD_ID_12="00000000-0000-0000-0000-000000000111"
RECORD_ID_13="00000000-0000-0000-0000-000000000112"
RECORD_ID_14="00000000-0000-0000-0000-000000000113"
RECORD_ID_15="00000000-0000-0000-0000-000000000114"
RECORD_ID_16="00000000-0000-0000-0000-000000000115"
RECORD_ID_17="00000000-0000-0000-0000-000000000116"
RECORD_ID_18="00000000-0000-0000-0000-000000000117"
EVENT_ID="00000000-0000-0000-0000-000000000200"
EVENT_ID_2="00000000-0000-0000-0000-000000000201"
EVENT_ID_3="00000000-0000-0000-0000-000000000202"
EVENT_ID_4="00000000-0000-0000-0000-000000000203"
EVENT_ID_5="00000000-0000-0000-0000-000000000204"
EVENT_ID_6="00000000-0000-0000-0000-000000000205"
EVENT_ID_7="00000000-0000-0000-0000-000000000206"
EVENT_ID_8="00000000-0000-0000-0000-000000000207"
EVENT_ID_9="00000000-0000-0000-0000-000000000208"
EVENT_ID_10="00000000-0000-0000-0000-000000000209"
EVENT_ID_11="00000000-0000-0000-0000-000000000210"
EVENT_ID_12="00000000-0000-0000-0000-000000000211"
INCIDENT_ID="00000000-0000-0000-0000-000000000060"
INCIDENT_ID_2="00000000-0000-0000-0000-000000000063"
INCIDENT_ID_3="00000000-0000-0000-0000-000000000064"
INCIDENT_ID_4="00000000-0000-0000-0000-000000000065"
INCIDENT_NOTE_ID="00000000-0000-0000-0000-000000000061"
INCIDENT_NOTE_AUDIT_ID="00000000-0000-0000-0000-000000000062"
MAPPING_ID="00000000-0000-0000-0000-000000000070"
TRIGGER_ID="00000000-0000-0000-0000-000000000071"
RETENTION_ID="00000000-0000-0000-0000-000000000080"
RETENTION_ID_2="00000000-0000-0000-0000-000000000081"
RETENTION_ID_3="00000000-0000-0000-0000-000000000082"
EVENT_DEFINITION_ID="00000000-0000-0000-0000-000000000090"
EVENT_DEFINITION_ID_2="00000000-0000-0000-0000-000000000091"
EVENT_DEFINITION_ID_3="00000000-0000-0000-0000-000000000092"
EVENT_DEFINITION_ID_4="00000000-0000-0000-0000-000000000093"

KEY_HASH="$(python3 -c "import hashlib,sys; print(hashlib.sha256(sys.argv[1].encode()).hexdigest())" "$API_KEY")"
PASSWORD_HASH="$(cargo run -q -p auth-service --bin hash_password -- "$DEMO_PASSWORD")"
OPERATOR_PASSWORD_HASH="$(cargo run -q -p auth-service --bin hash_password -- "$OPERATOR_PASSWORD")"
VIEWER_PASSWORD_HASH="$(cargo run -q -p auth-service --bin hash_password -- "$VIEWER_PASSWORD")"
# The local launcher exports the same DATABASE_URL to every service. Derive the compose database
# from that URL so the seed follows the actual local stack (the checked-in `.env` intentionally
# points at the isolated `kizashi_test` database).
DB_NAME="${DATABASE_URL##*/}"

# ON CONFLICT (id) DO UPDATE, not DO NOTHING — the row's *id* is what's actually fixed/stable
# across runs; if this script's own constants ever change (as they did once already, going
# from a space-separated demo password to a hyphenated one), re-running must converge to the
# new values rather than erroring on a stale row with the same id but a different key_hash.
docker compose exec -T postgres psql -U kizashi -d "$DB_NAME" -v ON_ERROR_STOP=1 <<SQL
INSERT INTO auth_service.tenants (id, name)
VALUES ('$TENANT_ID', '$TENANT_NAME')
ON CONFLICT (id) DO UPDATE SET name = excluded.name;

INSERT INTO auth_service.local_users (id, tenant_id, username, password_hash, role)
VALUES ('$USER_ID', '$TENANT_ID', 'demo', '$PASSWORD_HASH', 'admin')
ON CONFLICT (id) DO UPDATE SET password_hash = excluded.password_hash, role = excluded.role;

-- Stable operator/viewer personas make the three RBAC boundaries directly testable in a
-- fresh local workspace. These are local-only demo credentials, never production defaults.
INSERT INTO auth_service.local_users (id, tenant_id, username, password_hash, role)
VALUES
  ('$OPERATOR_USER_ID', '$TENANT_ID', 'operator', '$OPERATOR_PASSWORD_HASH', 'operator'),
  ('$VIEWER_USER_ID', '$TENANT_ID', 'viewer', '$VIEWER_PASSWORD_HASH', 'viewer')
ON CONFLICT (id) DO UPDATE SET password_hash = excluded.password_hash, role = excluded.role;

INSERT INTO ingestion_gateway.api_keys (id, tenant_id, key_hash, label, created_at)
VALUES ('$KEY_ID', '$TENANT_ID', '$KEY_HASH', 'local-demo', now())
ON CONFLICT (id) DO UPDATE SET key_hash = excluded.key_hash, revoked_at = NULL;

-- Real event contracts make the signal registry useful on a clean launch. These definitions
-- describe the same classes written to ClickHouse below; source mappings are carried in the
-- schema extension consumed by the Event Types console.
INSERT INTO config_admin_service.event_type_definitions
  (id, tenant_id, name, field_schema, version)
VALUES
  ('$EVENT_DEFINITION_ID', '$TENANT_ID', 'customer.health.degraded',
   '{"type":"object","properties":{"severity":{"type":"string"},"reason":{"type":"string"},"entity_ref":{"type":"string"}},"required":["severity","reason"],"x-kizashi-source-mapping":{"severity":"$.severity","reason":"$.reason","entity_ref":"$.customer_id"}}', 1),
  ('$EVENT_DEFINITION_ID_2', '$TENANT_ID', 'customer.health.critical',
   '{"type":"object","properties":{"severity":{"type":"string"},"reason":{"type":"string"},"entity_ref":{"type":"string"}},"required":["severity","reason"],"x-kizashi-source-mapping":{"severity":"$.severity","reason":"$.reason","entity_ref":"$.customer_id"}}', 1),
  ('$EVENT_DEFINITION_ID_3', '$TENANT_ID', 'access.role_mapping.review',
   '{"type":"object","properties":{"severity":{"type":"string"},"reason":{"type":"string"},"entity_ref":{"type":"string"}},"required":["severity","reason"],"x-kizashi-source-mapping":{"severity":"$.severity","reason":"$.reason","entity_ref":"$.customer_id"}}', 1),
  ('$EVENT_DEFINITION_ID_4', '$TENANT_ID', 'certificate.rotation.completed',
   '{"type":"object","properties":{"severity":{"type":"string"},"reason":{"type":"string"},"entity_ref":{"type":"string"}},"required":["severity","reason"],"x-kizashi-source-mapping":{"severity":"$.severity","reason":"$.reason","entity_ref":"$.customer_id"}}', 1)
ON CONFLICT (tenant_id, name, version) DO UPDATE SET field_schema = excluded.field_schema;

-- A coherent demo ontology makes the Console immediately explorable after launch. These are
-- normal ontology rows (not UI fixtures): the same API serves them as it serves derived data.
INSERT INTO ontology_service.object_types
  (id, tenant_id, name, version, property_schema, mapping_rules)
VALUES
  ('$CUSTOMER_TYPE_ID', '$TENANT_ID', 'Customer', 1,
   '{"id":{"type":"string"},"name":{"type":"string"},"plan":{"type":"string"},"health":{"type":"string"}}',
   '[{"source_type":"ticket","fields":{"id":"customer_id","name":"customer_name","plan":"plan","health":"health"},"identity_field":"id"}]'),
  ('$TICKET_TYPE_ID', '$TENANT_ID', 'Support Ticket', 1,
   '{"id":{"type":"string"},"subject":{"type":"string"},"priority":{"type":"string"},"status":{"type":"string"}}',
   '[{"source_type":"ticket","fields":{"id":"ticket_id","subject":"subject","priority":"priority","status":"status"},"identity_field":"id"}]'),
  ('$SUPPORT_TEAM_TYPE_ID', '$TENANT_ID', 'Support Team', 1,
   '{"id":{"type":"string"},"name":{"type":"string"},"coverage":{"type":"string"}}',
   '[]')
ON CONFLICT (id) DO UPDATE SET property_schema = excluded.property_schema, mapping_rules = excluded.mapping_rules, updated_at = now();

INSERT INTO ontology_service.objects
  (id, tenant_id, object_type_id, properties, source_lineage)
VALUES
  ('$CUSTOMER_OBJECT_ID', '$TENANT_ID', '$CUSTOMER_TYPE_ID',
   '{"id":"cust-042","name":"Northwind Health","plan":"Enterprise","health":"At risk"}',
   '["00000000-0000-0000-0000-000000000100"]'),
  ('$TICKET_OBJECT_ID', '$TENANT_ID', '$TICKET_TYPE_ID',
   '{"id":"ticket-1842","subject":"SSO provisioning is delayed","priority":"High","status":"Investigating"}',
   '["00000000-0000-0000-0000-000000000101"]'),
  ('$CUSTOMER_OBJECT_ID_2', '$TENANT_ID', '$CUSTOMER_TYPE_ID',
   '{"id":"cust-017","name":"Contoso Logistics","plan":"Business","health":"Degraded"}',
   '["00000000-0000-0000-0000-000000000102"]'),
  ('$CUSTOMER_OBJECT_ID_3', '$TENANT_ID', '$CUSTOMER_TYPE_ID',
   '{"id":"cust-091","name":"Fabrikam Manufacturing","plan":"Enterprise","health":"Healthy"}',
   '["00000000-0000-0000-0000-000000000104"]'),
  ('$TICKET_OBJECT_ID_2', '$TENANT_ID', '$TICKET_TYPE_ID',
   '{"id":"ticket-1851","subject":"SCIM sync is dropping groups","priority":"Critical","status":"Investigating"}',
   '["00000000-0000-0000-0000-000000000102"]'),
  ('$TICKET_OBJECT_ID_3', '$TENANT_ID', '$TICKET_TYPE_ID',
   '{"id":"ticket-1860","subject":"Role mapping review requested","priority":"Medium","status":"Open"}',
   '["00000000-0000-0000-0000-000000000103"]'),
  ('$TICKET_OBJECT_ID_4', '$TENANT_ID', '$TICKET_TYPE_ID',
   '{"id":"ticket-1812","subject":"SAML certificate rotation complete","priority":"Low","status":"Resolved"}',
   '["00000000-0000-0000-0000-000000000105"]'),
  ('$SUPPORT_TEAM_OBJECT_ID', '$TENANT_ID', '$SUPPORT_TEAM_TYPE_ID',
   '{"id":"team-identity-ops","name":"Identity Operations","coverage":"SSO and SCIM"}',
   '[]')
ON CONFLICT (id) DO UPDATE SET properties = excluded.properties, source_lineage = excluded.source_lineage, updated_at = now();

INSERT INTO ontology_service.link_types
  (id, tenant_id, name, source_object_type_id, target_object_type_id, cardinality, properties_schema)
VALUES ('$LINK_TYPE_ID', '$TENANT_ID', 'Raised by', '$TICKET_TYPE_ID', '$CUSTOMER_TYPE_ID', 'many-to-one', '{}')
ON CONFLICT (id) DO UPDATE SET cardinality = excluded.cardinality, updated_at = now();

INSERT INTO ontology_service.link_types
  (id, tenant_id, name, source_object_type_id, target_object_type_id, cardinality, properties_schema)
VALUES ('$SUPPORT_LINK_TYPE_ID', '$TENANT_ID', 'Supported by', '$CUSTOMER_TYPE_ID', '$SUPPORT_TEAM_TYPE_ID', 'many-to-one', '{}')
ON CONFLICT (id) DO UPDATE SET cardinality = excluded.cardinality, updated_at = now();

INSERT INTO ontology_service.links
  (id, tenant_id, link_type_id, source_object_id, target_object_id, properties)
VALUES ('$LINK_ID', '$TENANT_ID', '$LINK_TYPE_ID', '$TICKET_OBJECT_ID', '$CUSTOMER_OBJECT_ID', '{"confidence":0.98}')
,
  ('$LINK_ID_2', '$TENANT_ID', '$LINK_TYPE_ID', '$TICKET_OBJECT_ID_2', '$CUSTOMER_OBJECT_ID_2', '{"confidence":0.96}'),
  ('$LINK_ID_3', '$TENANT_ID', '$LINK_TYPE_ID', '$TICKET_OBJECT_ID_3', '$CUSTOMER_OBJECT_ID_2', '{"confidence":0.91}'),
  ('$LINK_ID_4', '$TENANT_ID', '$LINK_TYPE_ID', '$TICKET_OBJECT_ID_4', '$CUSTOMER_OBJECT_ID_3', '{"confidence":0.99}')
ON CONFLICT (id) DO UPDATE SET properties = excluded.properties, updated_at = now();

INSERT INTO ontology_service.links
  (id, tenant_id, link_type_id, source_object_id, target_object_id, properties)
VALUES
  ('$SUPPORT_LINK_ID', '$TENANT_ID', '$SUPPORT_LINK_TYPE_ID', '$CUSTOMER_OBJECT_ID', '$SUPPORT_TEAM_OBJECT_ID', '{"confidence":0.99}'),
  ('$SUPPORT_LINK_ID_2', '$TENANT_ID', '$SUPPORT_LINK_TYPE_ID', '$CUSTOMER_OBJECT_ID_2', '$SUPPORT_TEAM_OBJECT_ID', '{"confidence":0.97}'),
  ('$SUPPORT_LINK_ID_3', '$TENANT_ID', '$SUPPORT_LINK_TYPE_ID', '$CUSTOMER_OBJECT_ID_3', '$SUPPORT_TEAM_OBJECT_ID', '{"confidence":0.94}')
ON CONFLICT (id) DO UPDATE SET properties = excluded.properties, updated_at = now();

INSERT INTO ontology_service.action_types
  (id, tenant_id, name, target_object_type_id, parameter_schema, preconditions, effect_definition)
VALUES ('$ACTION_TYPE_ID', '$TENANT_ID', 'Escalate support ticket', '$TICKET_TYPE_ID',
        '{"reason":{"type":"string"}}', '{"status":"Investigating"}', '{"priority":"Critical"}')
ON CONFLICT (id) DO UPDATE SET target_object_type_id = excluded.target_object_type_id, parameter_schema = excluded.parameter_schema, updated_at = now();

INSERT INTO ontology_service.action_types
  (id, tenant_id, name, target_object_type_id, parameter_schema, preconditions, effect_definition)
VALUES
  ('$ACTION_TYPE_ID_2', '$TENANT_ID', 'Notify executive sponsor', '$TICKET_TYPE_ID',
   '{"recipient":{"type":"string"},"summary":{"type":"string"}}',
   '{"priority":"Critical"}', '{"notification":"Executive sponsor notified"}'),
  ('$ACTION_TYPE_ID_3', '$TENANT_ID', 'Update ticket lifecycle', '$TICKET_TYPE_ID',
   '{"status":{"type":"string"}}',
   '{}', '{"status":{"\$parameter":"status"}}')
ON CONFLICT (id) DO UPDATE SET target_object_type_id = excluded.target_object_type_id,
  parameter_schema = excluded.parameter_schema, preconditions = excluded.preconditions,
  effect_definition = excluded.effect_definition, updated_at = now();

UPDATE ontology_service.action_types
SET target_object_type_id = '$TICKET_TYPE_ID', updated_at = now()
WHERE tenant_id = '$TENANT_ID'
  AND name IN ('Set ticket status', 'Notify executive');

INSERT INTO ontology_service.action_invocations
  (id, tenant_id, action_type_id, target_object_ids, parameters, outcome, triggering_event_ref, executed_at)
VALUES ('$INVOCATION_ID', '$TENANT_ID', '$ACTION_TYPE_ID',
        '["$TICKET_OBJECT_ID"]', '{"reason":"Enterprise customer impact"}', 'Completed',
        '{"event_type":"ticket.priority.changed","id":"00000000-0000-0000-0000-000000000200","actor":"demo"}', now() - interval '18 minutes')
ON CONFLICT (id) DO UPDATE SET outcome = excluded.outcome, parameters = excluded.parameters, triggering_event_ref = excluded.triggering_event_ref, executed_at = excluded.executed_at;

INSERT INTO ontology_service.action_invocations
  (id, tenant_id, action_type_id, target_object_ids, parameters, outcome, triggering_event_ref, executed_at)
VALUES
  ('$INVOCATION_ID_2', '$TENANT_ID', '$ACTION_TYPE_ID', '["$TICKET_OBJECT_ID_2"]', '{"reason":"Critical SCIM sync degradation"}', 'Completed',
   '{"event_id":"00000000-0000-0000-0000-000000000201","incident_id":"00000000-0000-0000-0000-000000000063","actor":"demo","source":"console"}', now() - interval '11 minutes'),
  ('$INVOCATION_ID_3', '$TENANT_ID', '$ACTION_TYPE_ID', '["$TICKET_OBJECT_ID_3"]', '{"reason":123}', 'Rejected: parameter validation failed',
   '{"event_id":"00000000-0000-0000-0000-000000000202","actor":"demo","source":"console"}', now() - interval '7 minutes')
ON CONFLICT (id) DO UPDATE SET outcome = excluded.outcome, parameters = excluded.parameters, triggering_event_ref = excluded.triggering_event_ref, executed_at = excluded.executed_at;

INSERT INTO ontology_service.action_invocations
  (id, tenant_id, action_type_id, target_object_ids, parameters, outcome, triggering_event_ref, executed_at)
VALUES
  ('$INVOCATION_ID_4', '$TENANT_ID', '$ACTION_TYPE_ID_2', '["$TICKET_OBJECT_ID_2"]',
   '{"recipient":"vp-support@acme.example","summary":"SCIM provisioning outage"}', 'Completed',
   '{"event_id":"00000000-0000-0000-0000-000000000201","incident_id":"00000000-0000-0000-0000-000000000063","actor":"demo","source":"console"}', now() - interval '9 minutes'),
  ('$INVOCATION_ID_5', '$TENANT_ID', '$ACTION_TYPE_ID_3', '["$TICKET_OBJECT_ID_3"]',
   '{"status":"Acknowledged"}', 'Needs review',
   '{"event_id":"00000000-0000-0000-0000-000000000202","actor":"demo","source":"console"}', now() - interval '4 minutes')
ON CONFLICT (id) DO UPDATE SET action_type_id = excluded.action_type_id, outcome = excluded.outcome,
  parameters = excluded.parameters, triggering_event_ref = excluded.triggering_event_ref, executed_at = excluded.executed_at;

INSERT INTO config_admin_service.agents
  (id, tenant_id, connector_type, name, config, enabled)
VALUES ('$SENSOR_ID', '$TENANT_ID', 'zendesk', 'support-intake',
        '{"source":"demo","workspace":"acme-support"}', true)
ON CONFLICT (id) DO UPDATE SET name = excluded.name, config = excluded.config, enabled = true;

INSERT INTO config_admin_service.agents
  (id, tenant_id, connector_type, name, config, enabled)
VALUES
  ('$SENSOR_ID_2', '$TENANT_ID', 'zendesk', 'zendesk-primary', '{"source":"demo","workspace":"acme-support"}', true),
  ('$SENSOR_ID_3', '$TENANT_ID', 'graph-mail', 'graph-mailbox', '{"source":"demo","mailbox":"support@acme.example"}', true),
  ('$SENSOR_ID_4', '$TENANT_ID', 'graph-teams', 'graph-teams-warroom', '{"source":"demo","team":"identity-ops"}', true),
  ('$SENSOR_ID_5', '$TENANT_ID', 'sql', 'billing-sql', '{"source":"demo","database":"billing"}', true),
  ('$SENSOR_ID_6', '$TENANT_ID', 'fabric', 'warehouse-fabric', '{"source":"demo","workspace":"operations"}', true)
ON CONFLICT (id) DO UPDATE SET name = excluded.name, config = excluded.config, enabled = true;

INSERT INTO config_admin_service.normalization_mappings
  (id, tenant_id, source_type, field_map, version, dedup_fields, dedup_window_seconds)
VALUES ('$MAPPING_ID', '$TENANT_ID', 'ticket',
        '{"ticket_id":"$.ticket_id","customer_id":"$.customer_id","customer_name":"$.customer_name","subject":"$.subject","priority":"$.priority","status":"$.status","plan":"$.plan","health":"$.health"}',
        1, '["ticket_id"]', 86400)
ON CONFLICT (id) DO UPDATE SET field_map = excluded.field_map, dedup_fields = excluded.dedup_fields,
  dedup_window_seconds = excluded.dedup_window_seconds, version = excluded.version;

INSERT INTO config_admin_service.analysis_configs
  (tenant_id, prompt, updated_at, provider, model, endpoint)
VALUES ('$TENANT_ID',
        'Classify support tickets for customer health, identity provisioning risk, urgency, and the event types needed for operational triage.',
        now(), 'azure_foundry', NULL, NULL)
ON CONFLICT (tenant_id) DO UPDATE SET prompt = excluded.prompt, updated_at = excluded.updated_at,
  provider = excluded.provider, model = excluded.model, endpoint = excluded.endpoint;

INSERT INTO config_admin_service.trigger_definitions
  (id, tenant_id, name, event_type_match, condition, window_seconds, actions, enabled)
VALUES ('$TRIGGER_ID', '$TENANT_ID', 'Identity ticket escalation', 'customer.health.degraded',
        '{"shape":"count_over_window","count":2}', 3600, '[]', true)
ON CONFLICT (id) DO UPDATE SET name = excluded.name, event_type_match = excluded.event_type_match,
  condition = excluded.condition, window_seconds = excluded.window_seconds, enabled = excluded.enabled;

INSERT INTO normalization_service.normalization_mappings
  (id, tenant_id, source_type, field_map, version, dedup_fields, dedup_window_seconds)
VALUES ('$MAPPING_ID', '$TENANT_ID', 'ticket',
        '{"ticket_id":"$.ticket_id","customer_id":"$.customer_id","customer_name":"$.customer_name","subject":"$.subject","priority":"$.priority","status":"$.status","plan":"$.plan","health":"$.health"}',
        1, '["ticket_id"]', 86400)
ON CONFLICT (id) DO UPDATE SET field_map = excluded.field_map, dedup_fields = excluded.dedup_fields,
  dedup_window_seconds = excluded.dedup_window_seconds, version = excluded.version;

INSERT INTO ingestion_service.raw_records
  (id, connector_id, source_type, ingested_at, occurred_at, raw_payload, normalized_payload, tenant_id)
VALUES
  ('$RECORD_ID', 'support-intake', '"ticket"', now() - interval '12 minutes', now() - interval '14 minutes',
   '{"ticket_id":"ticket-1842","customer_id":"cust-042","customer_name":"Northwind Health","subject":"SSO provisioning is delayed","priority":"High","status":"Investigating","plan":"Enterprise","health":"At risk"}',
   '{"ticket_id":"ticket-1842","customer_id":"cust-042","customer_name":"Northwind Health","subject":"SSO provisioning is delayed","priority":"High","status":"Investigating","plan":"Enterprise","health":"At risk"}', '$TENANT_ID'),
  ('$RECORD_ID_2', 'support-intake', '"ticket"', now() - interval '8 minutes', now() - interval '9 minutes',
   '{"ticket_id":"ticket-1837","customer_id":"cust-042","customer_name":"Northwind Health","subject":"SCIM group sync warning","priority":"Medium","status":"Open","plan":"Enterprise","health":"At risk"}',
   '{"ticket_id":"ticket-1837","customer_id":"cust-042","customer_name":"Northwind Health","subject":"SCIM group sync warning","priority":"Medium","status":"Open","plan":"Enterprise","health":"At risk"}', '$TENANT_ID'),
  ('$RECORD_ID_3', 'support-intake', '"ticket"', now() - interval '16 minutes', now() - interval '18 minutes',
   '{"ticket_id":"ticket-1851","customer_id":"cust-017","customer_name":"Contoso Logistics","subject":"SCIM sync is dropping groups","priority":"Critical","status":"Investigating","plan":"Business","health":"Degraded"}',
   '{"ticket_id":"ticket-1851","customer_id":"cust-017","customer_name":"Contoso Logistics","subject":"SCIM sync is dropping groups","priority":"Critical","status":"Investigating","plan":"Business","health":"Degraded"}', '$TENANT_ID'),
  ('$RECORD_ID_4', 'support-intake', '"ticket"', now() - interval '22 minutes', now() - interval '25 minutes',
   '{"ticket_id":"ticket-1860","customer_id":"cust-017","customer_name":"Contoso Logistics","subject":"Role mapping review requested","priority":"Medium","status":"Open","plan":"Business","health":"Degraded"}',
   '{"ticket_id":"ticket-1860","customer_id":"cust-017","customer_name":"Contoso Logistics","subject":"Role mapping review requested","priority":"Medium","status":"Open","plan":"Business","health":"Degraded"}', '$TENANT_ID'),
  ('$RECORD_ID_5', 'support-intake', '"ticket"', now() - interval '31 minutes', now() - interval '34 minutes',
   '{"ticket_id":"ticket-1812","customer_id":"cust-091","customer_name":"Fabrikam Manufacturing","subject":"SAML certificate rotation complete","priority":"Low","status":"Resolved","plan":"Enterprise","health":"Healthy"}',
   '{"ticket_id":"ticket-1812","customer_id":"cust-091","customer_name":"Fabrikam Manufacturing","subject":"SAML certificate rotation complete","priority":"Low","status":"Resolved","plan":"Enterprise","health":"Healthy"}', '$TENANT_ID'),
  ('$RECORD_ID_6', 'support-intake', '"ticket"', now() - interval '4 minutes', now() - interval '5 minutes',
   '{"ticket_id":"ticket-1870","customer_id":"cust-042","customer_name":"Northwind Health","subject":"Admin invite latency increased","priority":"High","status":"Open","plan":"Enterprise","health":"At risk"}',
   '{"ticket_id":"ticket-1870","customer_id":"cust-042","customer_name":"Northwind Health","subject":"Admin invite latency increased","priority":"High","status":"Open","plan":"Enterprise","health":"At risk"}', '$TENANT_ID')
ON CONFLICT (id) DO UPDATE SET raw_payload = excluded.raw_payload, normalized_payload = excluded.normalized_payload, ingested_at = excluded.ingested_at;

-- Spread the local workspace across multiple source systems and days so the Explorer's
-- composition, timeline, and normalization visuals have a useful operating shape on first run.
INSERT INTO ingestion_service.raw_records
  (id, connector_id, source_type, ingested_at, occurred_at, raw_payload, normalized_payload, tenant_id)
VALUES
  ('$RECORD_ID_7', 'zendesk-primary', '"ticket"', '2026-07-18 10:15:00+00', '2026-07-18 10:14:00+00',
   '{"ticket_id":"ticket-1771","customer_id":"cust-017","customer_name":"Contoso Logistics","subject":"Provisioning timeout after group change","priority":"High","status":"Open","plan":"Business","health":"Degraded"}',
   '{"ticket_id":"ticket-1771","customer_id":"cust-017","customer_name":"Contoso Logistics","subject":"Provisioning timeout after group change","priority":"High","status":"Open","plan":"Business","health":"Degraded"}', '$TENANT_ID'),
  ('$RECORD_ID_8', 'zendesk-primary', '"ticket"', '2026-07-19 13:40:00+00', '2026-07-19 13:39:00+00',
   '{"ticket_id":"ticket-1784","customer_id":"cust-091","customer_name":"Fabrikam Manufacturing","subject":"SCIM reconciliation completed","priority":"Low","status":"Resolved","plan":"Enterprise","health":"Healthy"}',
   '{"ticket_id":"ticket-1784","customer_id":"cust-091","customer_name":"Fabrikam Manufacturing","subject":"SCIM reconciliation completed","priority":"Low","status":"Resolved","plan":"Enterprise","health":"Healthy"}', '$TENANT_ID'),
  ('$RECORD_ID_9', 'graph-mailbox', '"message"', '2026-07-20 08:05:00+00', '2026-07-20 08:04:00+00',
   '{"message_id":"mail-501","subject":"VIP customer cannot authenticate","from":"exec@northwind.example","customer_id":"cust-042","priority":"Critical","health":"At risk"}',
   '{"message_id":"mail-501","subject":"VIP customer cannot authenticate","from":"exec@northwind.example","customer_id":"cust-042","priority":"Critical","health":"At risk"}', '$TENANT_ID'),
  ('$RECORD_ID_10', 'graph-mailbox', '"message"', '2026-07-21 16:25:00+00', '2026-07-21 16:24:00+00',
   '{"message_id":"mail-502","subject":"Identity operations weekly review","from":"identity-ops@acme.example","customer_id":"cust-017","priority":"Medium","health":"Degraded"}',
   NULL, '$TENANT_ID'),
  ('$RECORD_ID_11', 'graph-teams-warroom', '"message"', '2026-07-21 17:10:00+00', '2026-07-21 17:09:00+00',
   '{"message_id":"teams-701","subject":"War room opened for provisioning latency","team":"identity-ops","severity":"High","status":"Investigating"}',
   '{"message_id":"teams-701","subject":"War room opened for provisioning latency","team":"identity-ops","severity":"High","status":"Investigating"}', '$TENANT_ID'),
  ('$RECORD_ID_12', 'billing-sql', '"sql_row"', '2026-07-22 09:30:00+00', '2026-07-22 09:29:00+00',
   '{"row_id":"invoice-901","customer_id":"cust-042","subject":"Enterprise renewal at risk","amount":125000,"status":"review"}',
   '{"row_id":"invoice-901","customer_id":"cust-042","subject":"Enterprise renewal at risk","amount":125000,"status":"review"}', '$TENANT_ID'),
  ('$RECORD_ID_13', 'billing-sql', '"sql_row"', '2026-07-22 11:45:00+00', '2026-07-22 11:44:00+00',
   '{"row_id":"invoice-902","customer_id":"cust-091","subject":"Renewal processed","amount":68000,"status":"paid"}',
   NULL, '$TENANT_ID'),
  ('$RECORD_ID_14', 'warehouse-fabric', '"fabric_record"', '2026-07-23 07:20:00+00', '2026-07-23 07:19:00+00',
   '{"entity_id":"health-042","customer_id":"cust-042","metric":"support_health","value":42,"band":"critical"}',
   '{"entity_id":"health-042","customer_id":"cust-042","metric":"support_health","value":42,"band":"critical"}', '$TENANT_ID'),
  ('$RECORD_ID_15', 'warehouse-fabric', '"fabric_record"', '2026-07-23 12:05:00+00', '2026-07-23 12:04:00+00',
   '{"entity_id":"health-017","customer_id":"cust-017","metric":"support_health","value":58,"band":"degraded"}',
   '{"entity_id":"health-017","customer_id":"cust-017","metric":"support_health","value":58,"band":"degraded"}', '$TENANT_ID'),
  ('$RECORD_ID_16', 'support-intake', '"ticket"', '2026-07-23 15:15:00+00', '2026-07-23 15:14:00+00',
   '{"ticket_id":"ticket-1884","customer_id":"cust-042","customer_name":"Northwind Health","subject":"Executive escalation: login failure","priority":"Critical","status":"Open","plan":"Enterprise","health":"At risk"}',
   NULL, '$TENANT_ID'),
  ('$RECORD_ID_17', 'zendesk-primary', '"ticket"', '2026-07-24 02:30:00+00', '2026-07-24 02:29:00+00',
   '{"ticket_id":"ticket-1891","customer_id":"cust-017","customer_name":"Contoso Logistics","subject":"Access request still pending","priority":"Medium","status":"Open","plan":"Business","health":"Degraded"}',
   '{"ticket_id":"ticket-1891","customer_id":"cust-017","customer_name":"Contoso Logistics","subject":"Access request still pending","priority":"Medium","status":"Open","plan":"Business","health":"Degraded"}', '$TENANT_ID'),
  ('$RECORD_ID_18', 'graph-mailbox', '"message"', '2026-07-24 04:55:00+00', '2026-07-24 04:54:00+00',
   '{"message_id":"mail-503","subject":"Customer health follow-up","from":"csm@fabrikam.example","customer_id":"cust-091","priority":"Low","health":"Healthy"}',
   '{"message_id":"mail-503","subject":"Customer health follow-up","from":"csm@fabrikam.example","customer_id":"cust-091","priority":"Low","health":"Healthy"}', '$TENANT_ID')
ON CONFLICT (id) DO UPDATE SET connector_id = excluded.connector_id, source_type = excluded.source_type,
  ingested_at = excluded.ingested_at, occurred_at = excluded.occurred_at,
  raw_payload = excluded.raw_payload, normalized_payload = excluded.normalized_payload;

-- Turn the same degraded-customer event into an operational incident so the demo has a
-- complete source -> event -> response thread across the Console pages.
INSERT INTO incident_service.incidents
  (id, tenant_id, title, summary, severity, status, assigned_to, created_at, updated_at, resolved_at)
VALUES
  ('$INCIDENT_ID', '$TENANT_ID', 'Northwind SSO provisioning degradation',
   'Multiple identity tickets indicate delayed SSO provisioning for an Enterprise customer.',
   'high', 'open', 'demo', now() - interval '6 minutes', now() - interval '2 minutes', NULL),
  ('$INCIDENT_ID_2', '$TENANT_ID', 'Contoso SCIM group sync outage',
   'Critical group synchronization failures are blocking access provisioning for a Business customer.',
   'critical', 'acknowledged', 'demo', now() - interval '14 minutes', now() - interval '4 minutes', NULL),
  ('$INCIDENT_ID_3', '$TENANT_ID', 'Fabrikam certificate rotation follow-up',
   'Certificate rotation completed successfully; retain the case for post-change verification.',
   'low', 'resolved', 'demo', now() - interval '42 minutes', now() - interval '30 minutes', now() - interval '30 minutes'),
  ('$INCIDENT_ID_4', '$TENANT_ID', 'Northwind access review follow-up',
   'A second Northwind signal is waiting for an accountable operator to claim the investigation.',
   'high', 'open', NULL, now() - interval '3 minutes', now() - interval '3 minutes', NULL)
ON CONFLICT (id) DO UPDATE SET title = excluded.title, summary = excluded.summary,
  severity = excluded.severity, status = excluded.status, assigned_to = excluded.assigned_to, updated_at = excluded.updated_at,
  resolved_at = excluded.resolved_at;

INSERT INTO incident_service.incident_events (incident_id, event_id, linked_at)
VALUES ('$INCIDENT_ID', '$EVENT_ID', now() - interval '5 minutes')
ON CONFLICT (incident_id, event_id) DO NOTHING;

INSERT INTO incident_service.incident_events (incident_id, event_id, linked_at)
VALUES
  ('$INCIDENT_ID_2', '$EVENT_ID_2', now() - interval '13 minutes'),
  ('$INCIDENT_ID_3', '$EVENT_ID_4', now() - interval '31 minutes'),
  ('$INCIDENT_ID_4', '$EVENT_ID_12', now() - interval '2 minutes')
ON CONFLICT (incident_id, event_id) DO NOTHING;

INSERT INTO incident_service.incident_notes
  (id, tenant_id, incident_id, author, body, created_at)
VALUES
  ('$INCIDENT_NOTE_ID', '$TENANT_ID', '$INCIDENT_ID', 'demo',
   'Validated the customer impact against the two linked tickets. Escalation is ready for the support leadership handoff.',
   now() - interval '90 seconds')
ON CONFLICT (id) DO UPDATE SET body = excluded.body, author = excluded.author, created_at = excluded.created_at;

INSERT INTO incident_service.incident_audit_log
  (id, tenant_id, entity_type, entity_id, change_type, actor, before, after, changed_at)
VALUES
  ('$INCIDENT_NOTE_AUDIT_ID', '$TENANT_ID', 'incident_note', '$INCIDENT_ID', '"created"', 'demo', NULL,
   jsonb_build_object('id', '$INCIDENT_NOTE_ID', 'author', 'demo', 'body', 'Validated the customer impact against the two linked tickets. Escalation is ready for the support leadership handoff.'),
   now() - interval '90 seconds')
-- Incident audit rows are intentionally immutable. The fixed demo row is already correct on
-- subsequent launches, so preserve it instead of attempting an UPDATE that the append-only
-- trigger rejects.
ON CONFLICT (id) DO NOTHING;

INSERT INTO retention_service.retention_policies
  (id, tenant_id, data_class, ttl_days, enabled)
VALUES
  ('$RETENTION_ID', '$TENANT_ID', '"raw"', 90, true),
  ('$RETENTION_ID_2', '$TENANT_ID', '"normalized"', 30, true),
  ('$RETENTION_ID_3', '$TENANT_ID', '"event"', 180, true)
ON CONFLICT (tenant_id, data_class) DO UPDATE SET ttl_days = excluded.ttl_days,
  enabled = excluded.enabled;

INSERT INTO egress_gateway.tenant_allowlists (tenant_id, domains)
VALUES ('$TENANT_ID', ARRAY['zendesk.com', 'graph.microsoft.com', 'login.microsoftonline.com', 'api.openai.com'])
ON CONFLICT (tenant_id) DO UPDATE SET domains = excluded.domains;
SQL

# Seed the same event store used by Trigger Engine so Events, Overview, and record journeys
# have a connected operational story on a fresh local launch. ClickHouse is append-oriented;
# use a stable id and replace the demo row before inserting to keep this script idempotent.
if [ -n "${CLICKHOUSE_URL:-}" ]; then
  curl -sfS "$CLICKHOUSE_URL/" --data-binary 'CREATE TABLE IF NOT EXISTS events (id UUID, tenant_id UUID, event_type String, source_connector_ids Array(String), entity_ref String, group_key String, payload String, occurred_at DateTime64(3), created_at DateTime64(3), status String, record_ids Array(UUID)) ENGINE = MergeTree() ORDER BY (tenant_id, occurred_at)' >/dev/null
  curl -sfS "$CLICKHOUSE_URL/" --data-binary "ALTER TABLE events DELETE WHERE tenant_id = '$TENANT_ID' AND id IN ('$EVENT_ID', '$EVENT_ID_2', '$EVENT_ID_3', '$EVENT_ID_4', '$EVENT_ID_5', '$EVENT_ID_6', '$EVENT_ID_7', '$EVENT_ID_8', '$EVENT_ID_9', '$EVENT_ID_10', '$EVENT_ID_11', '$EVENT_ID_12')" >/dev/null || true
  sleep 1
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"customer.health.degraded\",\"source_connector_ids\":[\"support-intake\"],\"entity_ref\":\"ticket-1842\",\"group_key\":\"Northwind Health\",\"payload\":\"{\\\"severity\\\":\\\"high\\\",\\\"reason\\\":\\\"Multiple identity tickets detected\\\"}\",\"occurred_at\":\"2026-07-23 01:20:00.000\",\"created_at\":\"2026-07-23 01:20:00.000\",\"status\":\"new\",\"record_ids\":[\"$RECORD_ID\",\"$RECORD_ID_2\"]}" >/dev/null
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID_2\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"customer.health.critical\",\"source_connector_ids\":[\"support-intake\"],\"entity_ref\":\"ticket-1851\",\"group_key\":\"Contoso Logistics\",\"payload\":\"{\\\"severity\\\":\\\"critical\\\",\\\"reason\\\":\\\"SCIM group sync failures\\\"}\",\"occurred_at\":\"2026-07-23 01:12:00.000\",\"created_at\":\"2026-07-23 01:12:00.000\",\"status\":\"triggered\",\"record_ids\":[\"$RECORD_ID_3\"]}" >/dev/null
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID_3\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"access.role_mapping.review\",\"source_connector_ids\":[\"support-intake\"],\"entity_ref\":\"ticket-1860\",\"group_key\":\"Contoso Logistics\",\"payload\":\"{\\\"severity\\\":\\\"medium\\\",\\\"reason\\\":\\\"Role mapping requires review\\\"}\",\"occurred_at\":\"2026-07-23 01:08:00.000\",\"created_at\":\"2026-07-23 01:08:00.000\",\"status\":\"new\",\"record_ids\":[\"$RECORD_ID_4\"]}" >/dev/null
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID_4\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"certificate.rotation.completed\",\"source_connector_ids\":[\"support-intake\"],\"entity_ref\":\"ticket-1812\",\"group_key\":\"Fabrikam Manufacturing\",\"payload\":\"{\\\"severity\\\":\\\"low\\\",\\\"reason\\\":\\\"SAML certificate rotation complete\\\"}\",\"occurred_at\":\"2026-07-23 00:49:00.000\",\"created_at\":\"2026-07-23 00:49:00.000\",\"status\":\"actioned\",\"record_ids\":[\"$RECORD_ID_5\"]}" >/dev/null
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID_5\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"customer.health.degraded\",\"source_connector_ids\":[\"zendesk-primary\"],\"entity_ref\":\"ticket-1771\",\"group_key\":\"Contoso Logistics\",\"payload\":\"{\\\"severity\\\":\\\"high\\\",\\\"reason\\\":\\\"Provisioning timeout\\\"}\",\"occurred_at\":\"2026-07-18 10:16:00.000\",\"created_at\":\"2026-07-18 10:16:00.000\",\"status\":\"new\",\"record_ids\":[\"$RECORD_ID_7\"]}" >/dev/null
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID_6\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"certificate.rotation.completed\",\"source_connector_ids\":[\"zendesk-primary\"],\"entity_ref\":\"ticket-1784\",\"group_key\":\"Fabrikam Manufacturing\",\"payload\":\"{\\\"severity\\\":\\\"low\\\",\\\"reason\\\":\\\"SCIM reconciliation completed\\\"}\",\"occurred_at\":\"2026-07-19 13:41:00.000\",\"created_at\":\"2026-07-19 13:41:00.000\",\"status\":\"actioned\",\"record_ids\":[\"$RECORD_ID_8\"]}" >/dev/null
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID_7\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"access.authentication.failure\",\"source_connector_ids\":[\"graph-mailbox\"],\"entity_ref\":\"cust-042\",\"group_key\":\"Northwind Health\",\"payload\":\"{\\\"severity\\\":\\\"critical\\\",\\\"reason\\\":\\\"VIP customer authentication failure\\\"}\",\"occurred_at\":\"2026-07-20 08:06:00.000\",\"created_at\":\"2026-07-20 08:06:00.000\",\"status\":\"triggered\",\"record_ids\":[\"$RECORD_ID_9\"]}" >/dev/null
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID_8\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"support.review.required\",\"source_connector_ids\":[\"graph-mailbox\"],\"entity_ref\":\"cust-017\",\"group_key\":\"Identity Operations\",\"payload\":\"{\\\"severity\\\":\\\"medium\\\",\\\"reason\\\":\\\"Weekly review needs an owner\\\"}\",\"occurred_at\":\"2026-07-21 16:26:00.000\",\"created_at\":\"2026-07-21 16:26:00.000\",\"status\":\"new\",\"record_ids\":[\"$RECORD_ID_10\"]}" >/dev/null
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID_9\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"access.provisioning.latency\",\"source_connector_ids\":[\"graph-teams-warroom\"],\"entity_ref\":\"team-identity-ops\",\"group_key\":\"Identity Operations\",\"payload\":\"{\\\"severity\\\":\\\"high\\\",\\\"reason\\\":\\\"War room opened\\\"}\",\"occurred_at\":\"2026-07-21 17:11:00.000\",\"created_at\":\"2026-07-21 17:11:00.000\",\"status\":\"triggered\",\"record_ids\":[\"$RECORD_ID_11\"]}" >/dev/null
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID_10\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"billing.renewal.risk\",\"source_connector_ids\":[\"billing-sql\"],\"entity_ref\":\"cust-042\",\"group_key\":\"Northwind Health\",\"payload\":\"{\\\"severity\\\":\\\"high\\\",\\\"reason\\\":\\\"Enterprise renewal at risk\\\"}\",\"occurred_at\":\"2026-07-22 09:31:00.000\",\"created_at\":\"2026-07-22 09:31:00.000\",\"status\":\"new\",\"record_ids\":[\"$RECORD_ID_12\"]}" >/dev/null
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID_11\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"customer.health.critical\",\"source_connector_ids\":[\"warehouse-fabric\"],\"entity_ref\":\"cust-042\",\"group_key\":\"Northwind Health\",\"payload\":\"{\\\"severity\\\":\\\"critical\\\",\\\"reason\\\":\\\"Support health metric crossed threshold\\\"}\",\"occurred_at\":\"2026-07-23 07:21:00.000\",\"created_at\":\"2026-07-23 07:21:00.000\",\"status\":\"triggered\",\"record_ids\":[\"$RECORD_ID_14\"]}" >/dev/null
  curl -sfS "$CLICKHOUSE_URL/?query=INSERT%20INTO%20events%20FORMAT%20JSONEachRow" --data-binary "{\"id\":\"$EVENT_ID_12\",\"tenant_id\":\"$TENANT_ID\",\"event_type\":\"customer.health.degraded\",\"source_connector_ids\":[\"support-intake\"],\"entity_ref\":\"ticket-1884\",\"group_key\":\"Northwind Health\",\"payload\":\"{\\\"severity\\\":\\\"critical\\\",\\\"reason\\\":\\\"Executive escalation\\\"}\",\"occurred_at\":\"2026-07-23 15:16:00.000\",\"created_at\":\"2026-07-23 15:16:00.000\",\"status\":\"new\",\"record_ids\":[\"$RECORD_ID_16\"]}" >/dev/null
fi

mkdir -p run
cat >"$DEMO_TENANT_FILE" <<EOF
DEMO_TENANT_ID="$TENANT_ID"
DEMO_TENANT_NAME="$TENANT_NAME"
DEMO_USERNAME="demo"
DEMO_PASSWORD="$DEMO_PASSWORD"
DEMO_API_KEY="$API_KEY"
EOF

echo ""
echo "==> demo credentials (also saved to $DEMO_TENANT_FILE):"
echo "    Workspace:  $TENANT_NAME"
echo "    Username:   demo (admin)"
echo "    Password:   $DEMO_PASSWORD"
echo "    Operator:   operator / $OPERATOR_PASSWORD"
echo "    Viewer:     viewer / $VIEWER_PASSWORD"
echo "    API key:    $API_KEY  (for POST http://localhost:8081/v1/ingest, header X-Api-Key)"
