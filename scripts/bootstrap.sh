#!/usr/bin/env bash
# Spin up the local dev stack (Postgres, RabbitMQ, ClickHouse, MinIO) and run migrations.
#
# Usage: scripts/bootstrap.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ ! -f .env ]; then
  echo "no .env found, copying .env.example -> .env"
  cp .env.example .env
fi

echo "==> starting docker-compose stack (postgres, rabbitmq, clickhouse, minio)"
docker compose up -d postgres rabbitmq clickhouse minio

echo "==> waiting for postgres to accept connections"
until docker compose exec -T postgres pg_isready -U kizashi >/dev/null 2>&1; do
  sleep 1
done

# A dedicated database for local `cargo test`/integration-test runs -- kept separate from
# `kizashi` (the database the actual running docker-compose stack, and the Console UI a
# developer is looking at in a browser, both read/write) so running tests locally never
# pollutes what's visible in the live stack. CI already gets this for free (a fresh ephemeral
# Postgres container per run); this is the local-dev-loop equivalent. Idempotent: safe to
# re-run bootstrap.sh against an already-provisioned Postgres.
echo "==> ensuring kizashi_test database exists (for local cargo test runs, kept separate from kizashi)"
docker compose exec -T postgres psql -U kizashi -d postgres -tc \
  "SELECT 1 FROM pg_database WHERE datname = 'kizashi_test'" | grep -q 1 || \
  docker compose exec -T postgres psql -U kizashi -d postgres -c "CREATE DATABASE kizashi_test OWNER kizashi;"

echo "==> waiting for clickhouse to accept connections"
until docker compose exec -T clickhouse clickhouse-client --query "SELECT 1" >/dev/null 2>&1; do
  sleep 1
done

echo "==> waiting for minio to accept connections"
until docker compose exec -T minio curl -sf http://localhost:9000/minio/health/live >/dev/null 2>&1; do
  sleep 1
done

if [ -d "$ROOT/crates/common/migrations" ]; then
  echo "==> running migrations"
  for crate_dir in "$ROOT"/crates/*/; do
    if [ -d "${crate_dir}migrations" ]; then
      name="$(basename "$crate_dir")"
      echo "    -> $name"
      (cd "$crate_dir" && sqlx migrate run 2>/dev/null || true)
    fi
  done
else
  echo "==> no migrations found yet, skipping"
fi

echo "==> local dev stack is up. Postgres: localhost:5432, RabbitMQ: localhost:5672 (mgmt: 15672), ClickHouse: localhost:8123, MinIO: localhost:9100 (console: 9101)"
