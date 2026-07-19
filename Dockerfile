# Single shared Dockerfile for every Kizashi binary (13 services, 6 connectors, the Console
# UI) — one multi-stage build, the binary to produce selected via --build-arg BIN=<name>. One
# template kept in sync in one place beats twenty near-identical Dockerfiles drifting apart.
#
# Build context must be the repo root (this is a Cargo workspace — every crate's Cargo.lock
# resolution depends on the workspace root). Example:
#   docker build --build-arg BIN=ingestion-service -t kizashi/ingestion-service .

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
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
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
USER kizashi
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/service"]
