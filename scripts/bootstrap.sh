#!/usr/bin/env bash
# Spin up the local dev stack (Postgres, RabbitMQ, ClickHouse) and run migrations.
#
# Usage: scripts/bootstrap.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ ! -f .env ]; then
  echo "no .env found, copying .env.example -> .env"
  cp .env.example .env
fi

echo "==> starting docker-compose stack (postgres, rabbitmq, clickhouse)"
docker compose up -d postgres rabbitmq clickhouse

echo "==> waiting for postgres to accept connections"
until docker compose exec -T postgres pg_isready -U kizashi >/dev/null 2>&1; do
  sleep 1
done

echo "==> waiting for clickhouse to accept connections"
until docker compose exec -T clickhouse clickhouse-client --query "SELECT 1" >/dev/null 2>&1; do
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

echo "==> local dev stack is up. Postgres: localhost:5432, RabbitMQ: localhost:5672 (mgmt: 15672), ClickHouse: localhost:8123"
