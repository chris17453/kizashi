# ADR-0207: Keep Docker Compose; it must also run under Podman

- **Status:** accepted
- **Date:** 2026-09-26

## Context

Kizashi's local stack is `docker-compose.yml` (Postgres, RabbitMQ, ClickHouse, MinIO, services,
Sensors). The Yochō plan targets Fedora with Podman. Both `docker` and `podman` are present on
the development machine.

## Decision

- Keep a single `docker-compose.yml`. It must work under both `docker compose` and
  `podman compose`; features that break Podman (e.g. docker-only socket assumptions) are
  isolated behind a compose profile.
- Yochō services and a mock Foundry endpoint are added to the same file, under a `yocho` profile.
- `scripts/bootstrap.sh` detects the available engine; `KIZASHI_CONTAINER_ENGINE` overrides it.

## Consequences

- The Sensor invoker that shells out to the Docker CLI needs a Podman-compatible path or a
  documented limitation; tracked as a follow-up, not a Phase 0 blocker.
