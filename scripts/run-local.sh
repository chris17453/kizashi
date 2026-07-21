#!/usr/bin/env bash
# Builds and launches the full Kizashi stack locally as plain background processes (no
# Dockerfiles/docker-compose entries exist yet for the application services themselves —
# only infra is containerized). Each service writes its own logs/<name>.log and run/<name>.pid.
#
# Usage: scripts/run-local.sh [--seed]
#   --seed   also create a demo tenant/user/API key so the Console UI is immediately usable.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p logs run

if [ ! -f .env ]; then
  echo "no .env found, copying .env.example -> .env"
  cp .env.example .env
fi
set -a
# shellcheck disable=SC1091
source .env
set +a

echo "==> ensuring infra is up (postgres, rabbitmq, clickhouse, minio)"
bash scripts/bootstrap.sh

echo "==> building the workspace (this can take a while the first time)"
cargo build --workspace

# Port map — every service defaults its own BIND_ADDR to 0.0.0.0:8080, so each must be given
# a distinct one here. Kept out of .env.example since these are purely local-launcher wiring,
# not per-service runtime config an operator would set in a real deployment.
INGESTION_GATEWAY_PORT=8081
INGESTION_SERVICE_PORT=8082
QUERY_GATEWAY_PORT=8083
DASHBOARD_API_PORT=8084
NORMALIZATION_SERVICE_PORT=8085
ANALYSIS_SERVICE_PORT=8086
TRIGGER_ENGINE_PORT=8087
ACTION_EXECUTOR_PORT=8088
AUTH_SERVICE_PORT=8089
CONFIG_ADMIN_SERVICE_PORT=8090
RETENTION_SERVICE_PORT=8091
OBSERVABILITY_PORT=8092
UI_PORT=8093
INCIDENT_SERVICE_PORT=8096

start() {
  local name="$1" bin="$2"
  shift 2
  echo "==> starting $name on the port in its BIND_ADDR"
  env "$@" "./target/debug/$bin" >"logs/$name.log" 2>&1 &
  echo $! >"run/$name.pid"
}

wait_healthy() {
  local name="$1" url="$2"
  echo -n "    waiting for $name to become healthy"
  for _ in $(seq 1 30); do
    if curl -sf "$url" >/dev/null 2>&1; then
      echo " - ok"
      return 0
    fi
    echo -n "."
    sleep 1
  done
  echo " - TIMED OUT (check logs/$name.log)"
  return 1
}

# --- The pipeline chain, strictly in this order ---
# Every message-bus exchange is declared by its *publisher* on startup
# (RabbitMqEventPublisher::new does `exchange_declare`); consumers only `queue_bind`, which
# requires the exchange to already exist. So each stage's publisher must be up before the next
# stage's consumer starts, regardless of what their HTTP dependencies would otherwise allow:
# ingestion-service (declares record.ingested)
#   -> normalization-service (consumes it, declares record.normalized)
#   -> analysis-service (consumes it, declares record.analyzed)
#   -> trigger-engine (consumes it, declares event.created)
#   -> action-executor (consumes it)
# This ordering constraint isn't documented anywhere else in the codebase — it only surfaces by
# actually running the system, which is exactly what this script is for.
start ingestion-service ingestion-service \
  BIND_ADDR="0.0.0.0:$INGESTION_SERVICE_PORT" \
  DATABASE_URL="$DATABASE_URL" RABBITMQ_URL="$RABBITMQ_URL"
wait_healthy ingestion-service "http://localhost:$INGESTION_SERVICE_PORT/healthz"

start normalization-service normalization-service \
  BIND_ADDR="0.0.0.0:$NORMALIZATION_SERVICE_PORT" DATABASE_URL="$DATABASE_URL" \
  RABBITMQ_URL="$RABBITMQ_URL" INGESTION_SERVICE_URL="http://localhost:$INGESTION_SERVICE_PORT" \
  INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-change-me-in-production}"
wait_healthy normalization-service "http://localhost:$NORMALIZATION_SERVICE_PORT/healthz"

start analysis-service analysis-service \
  BIND_ADDR="0.0.0.0:$ANALYSIS_SERVICE_PORT" DATABASE_URL="$DATABASE_URL" \
  RABBITMQ_URL="$RABBITMQ_URL" \
  AZURE_AI_FOUNDRY_ENDPOINT="${AZURE_AI_FOUNDRY_ENDPOINT:-}" \
  AZURE_AI_FOUNDRY_API_KEY="${AZURE_AI_FOUNDRY_API_KEY:-}" \
  INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-change-me-in-production}"
wait_healthy analysis-service "http://localhost:$ANALYSIS_SERVICE_PORT/healthz"

start trigger-engine trigger-engine \
  BIND_ADDR="0.0.0.0:$TRIGGER_ENGINE_PORT" DATABASE_URL="$DATABASE_URL" \
  RABBITMQ_URL="$RABBITMQ_URL" CLICKHOUSE_URL="$CLICKHOUSE_URL"
wait_healthy trigger-engine "http://localhost:$TRIGGER_ENGINE_PORT/healthz"

start action-executor action-executor \
  BIND_ADDR="0.0.0.0:$ACTION_EXECUTOR_PORT" DATABASE_URL="$DATABASE_URL" \
  RABBITMQ_URL="$RABBITMQ_URL" TRIGGER_ENGINE_URL="http://localhost:$TRIGGER_ENGINE_PORT"
wait_healthy action-executor "http://localhost:$ACTION_EXECUTOR_PORT/healthz"

# --- Everything else: only HTTP dependencies, no exchange-declaration ordering constraint ---
start dashboard-api dashboard-api \
  BIND_ADDR="0.0.0.0:$DASHBOARD_API_PORT" CLICKHOUSE_URL="$CLICKHOUSE_URL"
wait_healthy dashboard-api "http://localhost:$DASHBOARD_API_PORT/healthz"

start config-admin-service config-admin-service \
  BIND_ADDR="0.0.0.0:$CONFIG_ADMIN_SERVICE_PORT" DATABASE_URL="$DATABASE_URL" \
  RABBITMQ_URL="$RABBITMQ_URL"
wait_healthy config-admin-service "http://localhost:$CONFIG_ADMIN_SERVICE_PORT/healthz"

start incident-service incident-service \
  BIND_ADDR="0.0.0.0:$INCIDENT_SERVICE_PORT" DATABASE_URL="$DATABASE_URL"
wait_healthy incident-service "http://localhost:$INCIDENT_SERVICE_PORT/healthz"

start ingestion-gateway ingestion-gateway \
  BIND_ADDR="0.0.0.0:$INGESTION_GATEWAY_PORT" DATABASE_URL="$DATABASE_URL" \
  INGESTION_SERVICE_URL="http://localhost:$INGESTION_SERVICE_PORT" \
  CONFIG_ADMIN_SERVICE_URL="http://localhost:$CONFIG_ADMIN_SERVICE_PORT"
wait_healthy ingestion-gateway "http://localhost:$INGESTION_GATEWAY_PORT/healthz"

start query-gateway query-gateway \
  BIND_ADDR="0.0.0.0:$QUERY_GATEWAY_PORT" DATABASE_URL="$DATABASE_URL" \
  DASHBOARD_API_URL="http://localhost:$DASHBOARD_API_PORT" \
  INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-change-me-in-production}"
wait_healthy query-gateway "http://localhost:$QUERY_GATEWAY_PORT/healthz"

start retention-service retention-service \
  BIND_ADDR="0.0.0.0:$RETENTION_SERVICE_PORT" DATABASE_URL="$DATABASE_URL" \
  INGESTION_SERVICE_URL="http://localhost:$INGESTION_SERVICE_PORT" \
  AWS_REGION="${AWS_REGION:-us-east-1}" S3_ENDPOINT_URL="${S3_ENDPOINT_URL:-http://localhost:9100}" \
  AWS_S3_BUCKET="${AWS_S3_BUCKET:-kizashi-archive}" \
  AWS_ACCESS_KEY_ID="${AWS_ACCESS_KEY_ID:-kizashi}" \
  AWS_SECRET_ACCESS_KEY="${AWS_SECRET_ACCESS_KEY:-kizashi-minio-dev}"
wait_healthy retention-service "http://localhost:$RETENTION_SERVICE_PORT/healthz"

start auth-service auth-service \
  BIND_ADDR="0.0.0.0:$AUTH_SERVICE_PORT" DATABASE_URL="$DATABASE_URL" \
  QUERY_GATEWAY_URL="http://localhost:$QUERY_GATEWAY_PORT" \
  INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-change-me-in-production}" \
  ENTRA_TENANT_ID="${ENTRA_TENANT_ID:-}" ENTRA_CLIENT_ID="${ENTRA_CLIENT_ID:-}" \
  ENTRA_CLIENT_SECRET="${ENTRA_CLIENT_SECRET:-}" ENTRA_REDIRECT_URL="${ENTRA_REDIRECT_URL:-}"
wait_healthy auth-service "http://localhost:$AUTH_SERVICE_PORT/healthz"

start observability observability \
  BIND_ADDR="0.0.0.0:$OBSERVABILITY_PORT" \
  RABBITMQ_MANAGEMENT_URL="${RABBITMQ_MANAGEMENT_URL:-http://kizashi:kizashi@localhost:15672}" \
  SERVICE_REGISTRY="ingestion-gateway=http://localhost:$INGESTION_GATEWAY_PORT,ingestion-service=http://localhost:$INGESTION_SERVICE_PORT,query-gateway=http://localhost:$QUERY_GATEWAY_PORT,dashboard-api=http://localhost:$DASHBOARD_API_PORT,normalization-service=http://localhost:$NORMALIZATION_SERVICE_PORT,analysis-service=http://localhost:$ANALYSIS_SERVICE_PORT,trigger-engine=http://localhost:$TRIGGER_ENGINE_PORT,action-executor=http://localhost:$ACTION_EXECUTOR_PORT,auth-service=http://localhost:$AUTH_SERVICE_PORT,config-admin-service=http://localhost:$CONFIG_ADMIN_SERVICE_PORT,retention-service=http://localhost:$RETENTION_SERVICE_PORT"
wait_healthy observability "http://localhost:$OBSERVABILITY_PORT/healthz"

# --- Tier 3: Console UI, depends on auth/query-gateway/config-admin/observability ---
start kizashi-ui kizashi-ui \
  BIND_ADDR="0.0.0.0:$UI_PORT" \
  AUTH_SERVICE_URL="http://localhost:$AUTH_SERVICE_PORT" \
  QUERY_GATEWAY_URL="http://localhost:$QUERY_GATEWAY_PORT" \
  CONFIG_ADMIN_SERVICE_URL="http://localhost:$CONFIG_ADMIN_SERVICE_PORT" \
  OBSERVABILITY_URL="http://localhost:$OBSERVABILITY_PORT" \
  INGESTION_SERVICE_URL="http://localhost:$INGESTION_SERVICE_PORT" \
  INGESTION_GATEWAY_URL="http://localhost:$INGESTION_GATEWAY_PORT" \
  INGESTION_GATEWAY_PUBLIC_URL="http://localhost:$INGESTION_GATEWAY_PORT" \
  ACTION_EXECUTOR_URL="http://localhost:$ACTION_EXECUTOR_PORT" \
  TRIGGER_ENGINE_URL="http://localhost:$TRIGGER_ENGINE_PORT" \
  INCIDENT_SERVICE_URL="http://localhost:$INCIDENT_SERVICE_PORT"
sleep 2

echo ""
echo "==> all services started. Console UI: http://localhost:$UI_PORT/login"
echo "    Platform health:  http://localhost:$OBSERVABILITY_PORT/v1/health"
echo "    Logs:              logs/<service>.log"
echo "    Stop everything:   scripts/stop-local.sh"

if [ "${1:-}" = "--seed" ]; then
  echo ""
  echo "==> seeding demo tenant/user/API key"
  bash scripts/seed-local-demo.sh
fi
