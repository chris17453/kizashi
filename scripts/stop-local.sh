#!/usr/bin/env bash
# Stops every service started by scripts/run-local.sh. Infra (postgres/rabbitmq/clickhouse/
# minio) is left running — use `docker compose down` separately if you want to tear that down
# too.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ ! -d run ] || [ -z "$(ls -A run 2>/dev/null)" ]; then
  echo "no running services found (nothing in run/)"
  exit 0
fi

for pidfile in run/*.pid; do
  name="$(basename "$pidfile" .pid)"
  pid="$(cat "$pidfile")"
  if kill -0 "$pid" 2>/dev/null; then
    kill "$pid"
    echo "stopped $name (pid $pid)"
  else
    echo "$name (pid $pid) was already stopped"
  fi
  rm -f "$pidfile"
done
