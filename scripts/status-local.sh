#!/usr/bin/env bash
# Health-checks every service scripts/run-local.sh started, plus infra. Doesn't require the
# services' env vars — it only reads run/*.pid and hits localhost.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

declare -A PORTS=(
  [ingestion-gateway]=8081 [ingestion-service]=8082 [query-gateway]=8083
  [dashboard-api]=8084 [normalization-service]=8085 [analysis-service]=8086
  [trigger-engine]=8087 [action-executor]=8088 [auth-service]=8089
  [config-admin-service]=8090 [retention-service]=8091 [observability]=8092
  [kizashi-ui]=8093
)

echo "== infra =="
for c in kizashi-postgres-1 kizashi-rabbitmq-1 kizashi-clickhouse-1 kizashi-minio-1; do
  status="$(docker inspect --format='{{.State.Status}}' "$c" 2>/dev/null || echo "not found")"
  printf "  %-22s %s\n" "$c" "$status"
done

echo "== services =="
for name in "${!PORTS[@]}"; do
  port="${PORTS[$name]}"
  pidfile="run/$name.pid"
  if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then
    if curl -sf "http://localhost:$port/healthz" >/dev/null 2>&1; then
      printf "  %-22s up   (pid %s, port %s)\n" "$name" "$(cat "$pidfile")" "$port"
    else
      printf "  %-22s DOWN (process running, /healthz not responding — check logs/%s.log)\n" "$name" "$name"
    fi
  else
    printf "  %-22s not running\n" "$name"
  fi
done | sort
