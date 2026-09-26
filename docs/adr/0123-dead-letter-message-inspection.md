# ADR-0123: Dead-letter message inspection

## Status

Accepted

## Context

The console could show dead-letter queue depth and replay the oldest message, but replay was
blind. Operators had no way to distinguish a stale tenant configuration, malformed payload, or
transient downstream failure before changing queue state.

## Decision

Each pipeline dead-letter manager exposes an internal-secret-protected `GET /v1/dead-letter/peek`
endpoint. It reads the oldest message with `basic_get`, returns a bounded UTF-8 body preview and
byte size, then negatively acknowledges the delivery with `requeue=true`. The message is not
consumed or modified. The Console fetches the preview only when the queue has messages and shows
it behind an explicit inspection disclosure next to the existing one-at-a-time replay control.

The preview is limited to 4 KiB and remains available through the operator Console's
internal-secret boundary and the versioned `/api/v1/actions/dead-letter/peek` route. The public
route still requires an operator principal and does not pretend the mixed queue is tenant-scoped.

## Consequences

- Operators can diagnose the oldest failure before replaying it.
- Peeking preserves queue depth and ordering semantics.
- Large or binary payloads are represented as a bounded lossy UTF-8 preview, so inspection never
  creates unbounded response or memory pressure.
- The preview may contain operational payload data and is therefore restricted to operator/admin
  access through the existing internal control plane.
