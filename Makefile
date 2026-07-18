.PHONY: help infra-up infra-down build run seed stop restart status logs test ci clean

help:
	@echo "Kizashi local dev targets:"
	@echo "  make infra-up    - start Postgres/RabbitMQ/ClickHouse/MinIO (scripts/bootstrap.sh)"
	@echo "  make infra-down  - stop and remove the infra containers and their volumes"
	@echo "  make build       - cargo build --workspace"
	@echo "  make run         - build + launch every service (scripts/run-local.sh)"
	@echo "  make seed        - seed a demo tenant/user/API key into a running stack"
	@echo "  make stop        - stop every service started by 'make run' (infra stays up)"
	@echo "  make restart     - stop then run"
	@echo "  make status      - health-check every running service"
	@echo "  make logs SERVICE=<name> - tail logs/<name>.log"
	@echo "  make test        - cargo test --workspace --lib --bins (no live infra required)"
	@echo "  make ci          - scripts/ci-local.sh against a running local stack"
	@echo "  make clean       - stop services and tear down infra"

infra-up:
	bash scripts/bootstrap.sh

infra-down:
	docker compose down -v

build:
	cargo build --workspace

run:
	bash scripts/run-local.sh

seed:
	bash scripts/seed-local-demo.sh

stop:
	bash scripts/stop-local.sh

restart: stop run

status:
	bash scripts/status-local.sh

logs:
	@test -n "$(SERVICE)" || (echo "usage: make logs SERVICE=<name>  (see logs/ for available names)"; exit 1)
	tail -f "logs/$(SERVICE).log"

test:
	cargo test --workspace --lib --bins

ci:
	bash scripts/ci-local.sh

clean: stop infra-down
