# Single shared Dockerfile for every Kizashi binary (13 services, 6 connectors, the Console
# UI) — one multi-stage build, the binary to produce selected via --build-arg BIN=<name>. One
# template kept in sync in one place beats twenty near-identical Dockerfiles drifting apart.
#
# Build context must be the repo root (this is a Cargo workspace — every crate's Cargo.lock
# resolution depends on the workspace root). Example:
#   docker build --build-arg BIN=ingestion-service -t kizashi/ingestion-service .
#
# Three binaries need more than the base image: `agent-scheduler` (ADR-0020) shells out to the
# `docker` CLI against the host's Docker socket to run each due connector, so it needs the CLI
# installed and (via docker-compose.yml's volume mount + `RUN_AS_USER=root` build arg) socket
# access; `backup-service` (ADR-0055) shells out to `pg_dump`. Every other binary stays on the
# minimal, non-root default. `INSTALL_DOCKER_CLI`/`INSTALL_POSTGRES_CLIENT` and `RUN_AS_USER`
# are opt-in per build, not per-BIN-name magic, so any future binary that needs the same
# capability just sets the same args rather than this file special-casing names.

# syntax=docker/dockerfile:1
FROM rust:1-slim-bookworm AS builder
ARG BIN
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release --bin "${BIN}" \
    && cp "target/release/${BIN}" /tmp/service

FROM debian:bookworm-slim
ARG INSTALL_DOCKER_CLI=false
# Debian bookworm's `docker.io` package ships Docker 20.10 (client API 1.41) — too old to talk
# to a modern host daemon (25.0+/API 1.44+, what this was actually tested against). Pulling
# the official static CLI-only binary instead of the distro package sidesteps that mismatch
# without adding Docker's apt repo/GPG key for what amounts to one binary.
ARG DOCKER_CLI_VERSION=26.1.4
ARG INSTALL_POSTGRES_CLIENT=false
# Debian bookworm's own apt repo ships postgresql-client-15 -- one major version behind
# docker-compose's `postgres:16-alpine`, and `pg_dump` refuses to dump a server newer than
# itself ("aborting because of server version mismatch"), which made backup-service's very
# first live run fail outright (fix/0011). Pull `postgresql-client-16` from the official PGDG
# apt repo instead, so the client always matches the server's major version this compose file
# actually runs.
ARG POSTGRES_CLIENT_MAJOR=16
ARG RUN_AS_USER=kizashi
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl gnupg \
    && if [ "$INSTALL_DOCKER_CLI" = "true" ]; then \
         curl -fsSL "https://download.docker.com/linux/static/stable/x86_64/docker-${DOCKER_CLI_VERSION}.tgz" \
           -o /tmp/docker-cli.tgz \
         && tar -xzf /tmp/docker-cli.tgz -C /tmp \
         && mv /tmp/docker/docker /usr/local/bin/docker \
         && rm -rf /tmp/docker-cli.tgz /tmp/docker \
         && chmod +x /usr/local/bin/docker; \
       fi \
    && if [ "$INSTALL_POSTGRES_CLIENT" = "true" ]; then \
         curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc \
           | gpg --dearmor -o /usr/share/keyrings/pgdg.gpg \
         && echo "deb [signed-by=/usr/share/keyrings/pgdg.gpg] https://apt.postgresql.org/pub/repos/apt bookworm-pgdg main" \
           > /etc/apt/sources.list.d/pgdg.list \
         && apt-get update \
         && apt-get install -y --no-install-recommends "postgresql-client-${POSTGRES_CLIENT_MAJOR}"; \
       fi \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 1000 kizashi
WORKDIR /app
# Every service that runs migrations reads them via env!("CARGO_MANIFEST_DIR") — an absolute
# path baked into the binary at compile time (e.g. /app/crates/ingestion-service/migrations).
# Copying the whole crates/ source tree (not just the compiled binary) is what keeps those
# paths valid at runtime; the extra .rs files are harmless bloat, not a correctness risk.
COPY --from=builder /app/crates /app/crates
COPY --from=builder /tmp/service /usr/local/bin/service
RUN chown -R kizashi:kizashi /app
USER ${RUN_AS_USER}
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/service"]
