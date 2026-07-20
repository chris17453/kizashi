# ADR-0042: Close two real RBAC gaps found by a platform-wide write-endpoint audit

## Status

Accepted.

## Context

Per the standing directive to bring this platform to an enterprise compliance bar, a systematic
audit was run across every mutating (POST/PUT/DELETE) HTTP handler in the workspace, checking
whether each one enforces a role/permission check before touching data. Two real gaps surfaced;
everything else audited (config-admin-service, ingestion-gateway, auth-service, and the rest of
the Console UI's write handlers) was already correctly gated.

1. **`retention-service`'s `POST /v1/sweep` and `POST /v1/reimport` had no authentication of any
   kind** — not a missing role check, a complete absence of one. `trigger_sweep` runs a
   retention sweep (archive-then-delete) across every enabled policy platform-wide;
   `trigger_reimport` takes a caller-supplied archive key and republishes that archived batch
   back into ingestion for whichever tenant it belonged to. Any caller able to reach
   `retention-service` over the network — no credential, no header, nothing — could trigger a
   destructive tenant-wide sweep or force an arbitrary reimport. This is the most severe class of
   gap this audit could have found.
2. **Four Console UI POST handlers** (`sensors_handler.rs`'s `post_sensors`,
   `post_delete_sensor`, `post_toggle_sensor`; `api_keys_handler.rs`'s `post_api_keys`,
   `post_revoke_api_key`) called their backend client without first checking
   `session.role.at_least(Role::Operator)`, unlike every sibling UI write handler
   (triggers/mappings/retention-policies/analysis-config/egress-allowlist/branding/users). Not
   independently exploitable — the backend clients still forward `X-Role` and
   config-admin-service/ingestion-gateway correctly reject insufficient roles server-side — but
   a real, concrete UX bug: the UI discards those 403s (`let _ = ...`) and redirects as if the
   action succeeded, so a Viewer clicking "Delete sensor" or "Revoke key" sees success while
   nothing happened.

## Decision

- `retention-service` gained a shared-secret gate on both ops endpoints
  (`X-Internal-Secret` header checked against `internal_secret` in `AppState`), reusing the
  exact pattern query-gateway's `/internal/tokens` already established (ADR-0009) rather than
  inventing a new trust mechanism — these are service-to-service operational triggers (an
  external CronJob-equivalent, ADR-0011 point 5) with no end user or session behind the call, so
  there's no `X-Role` to check, only whether the caller knows the secret.
  `retention-sweep-scheduler`'s sidecar in `docker-compose.yml` now sends it.
- The four UI handlers gained the same `session.role.at_least(Role::Operator)` guard (403 on
  failure) every sibling write handler already has — closes the inconsistency and makes the
  failure mode honest (a Viewer gets a real 403 instead of a misleading success redirect).

## Consequences

- Live-verified the severity of finding 1 was real, not theoretical: before the fix, `curl -X
  POST http://retention-service:8080/v1/sweep` with zero headers against the live deployed
  service returned `200` and actually ran a sweep. After the fix: `401` without the header,
  `401` with the wrong value, `200` only with the correct secret — confirmed against the same
  running container.
- `INTERNAL_API_SECRET` is now a required env var for `retention-service` (previously optional
  since nothing checked it) — `docker-compose.yml` defaults it to
  `change-me-in-production` like every other service already using this pattern, which is a
  known, accepted placeholder for local/dev use, not a claim of production-readiness.
- This audit was scoped to mutating HTTP endpoints only. It did not check for equivalent gaps in
  message-bus consumers (RabbitMQ) or scheduled/internal jobs beyond the two found — worth a
  follow-up pass if the same standard is to be applied there.
