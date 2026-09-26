# ADR-0209: RustFS replaces the MinIO container image

## Status

Accepted — 2026-09-26

## Context

Retention archival (ADR-0011) and `backup-service` need an S3-compatible object
store, and CI plus local compose ran `minio/minio`. MinIO stopped publishing
pullable images on Docker Hub and Quay, so CI's archival integration tests and
`docker compose up` both broke on image pull. The spec requires self-hosted,
non-vendor-locked dependencies.

## Decision

Run `rustfs/rustfs:1.0.0` (Apache-2.0) in CI and in `docker-compose.yml`,
pinned to an exact version. RustFS speaks the S3 API and serves MinIO's
`/minio/health/live` endpoint, so it runs under the existing `minio` service
name on the same ports. Every `http://minio:9000` endpoint, script, and Helm
default keeps working without change. Credentials move from
`MINIO_ROOT_USER`/`MINIO_ROOT_PASSWORD` to `RUSTFS_ACCESS_KEY`/`RUSTFS_SECRET_KEY`,
still sourced from `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`.

Services keep talking to it through the AWS S3 SDK only; nothing depends on
RustFS-specific APIs.

## Consequences

- CI and local stacks pull again; the archival and backup integration tests
  run against a real S3 server.
- The MinIO web console (port 9101) is gone from local compose.
- The `minio` service name is now a misnomer, kept deliberately to avoid
  churning endpoints. Renaming it is a separate change if it ever matters.
- Customers bringing their own S3 (AWS, MinIO, Ceph, RustFS) are unaffected,
  since the SDK boundary is unchanged.
