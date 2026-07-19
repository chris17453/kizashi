#!/usr/bin/env bash
# Seeds a demo tenant, a local Console UI user, and a connector API key, so a freshly-launched
# stack (scripts/run-local.sh) is immediately usable instead of requiring hand-written SQL —
# the same gap every manual smoke test this project has relied on so far had to work around.
#
# Talks to Postgres directly via `docker compose exec` (not through any service's HTTP API —
# there is no self-service API-key/user-provisioning endpoint yet, a gap this script works
# around rather than papering over). Safe to re-run — uses ON CONFLICT DO NOTHING throughout.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Fixed, not random — a throwaway local-only demo credential (never leaves localhost, backed
# by a local docker-compose Postgres), not a real secret, so a stable value is more useful here
# than a regenerated one: re-running this script against the same local Postgres always logs
# into the same demo tenant with the same credentials instead of accumulating a fresh set.
DEMO_PASSWORD="kizashi-local-demo-password"
API_KEY="kizashi-local-demo-api-key"
DEMO_TENANT_FILE="run/demo-tenant.env"
TENANT_ID="00000000-0000-0000-0000-000000000001"
TENANT_NAME="acme"
USER_ID="00000000-0000-0000-0000-000000000002"
KEY_ID="00000000-0000-0000-0000-000000000003"

KEY_HASH="$(python3 -c "import hashlib,sys; print(hashlib.sha256(sys.argv[1].encode()).hexdigest())" "$API_KEY")"
PASSWORD_HASH="$(cargo run -q -p auth-service --bin hash_password -- "$DEMO_PASSWORD")"

# ON CONFLICT (id) DO UPDATE, not DO NOTHING — the row's *id* is what's actually fixed/stable
# across runs; if this script's own constants ever change (as they did once already, going
# from a space-separated demo password to a hyphenated one), re-running must converge to the
# new values rather than erroring on a stale row with the same id but a different key_hash.
docker compose exec -T postgres psql -U kizashi -d kizashi -v ON_ERROR_STOP=1 <<SQL
INSERT INTO auth_service.tenants (id, name)
VALUES ('$TENANT_ID', '$TENANT_NAME')
ON CONFLICT (id) DO UPDATE SET name = excluded.name;

INSERT INTO auth_service.local_users (id, tenant_id, username, password_hash, role)
VALUES ('$USER_ID', '$TENANT_ID', 'demo', '$PASSWORD_HASH', 'admin')
ON CONFLICT (id) DO UPDATE SET password_hash = excluded.password_hash, role = excluded.role;

INSERT INTO ingestion_gateway.api_keys (id, tenant_id, key_hash, label, created_at)
VALUES ('$KEY_ID', '$TENANT_ID', '$KEY_HASH', 'local-demo', now())
ON CONFLICT (id) DO UPDATE SET key_hash = excluded.key_hash, revoked_at = NULL;
SQL

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
echo "    Username:   demo"
echo "    Password:   $DEMO_PASSWORD"
echo "    API key:    $API_KEY  (for POST http://localhost:8081/v1/ingest, header X-Api-Key)"
