# 0050. Redact the AI analysis config API key from read responses

## Context

Building the Permissions Reference page (ADR-0048) required auditing every role check across the
platform to describe accurately what each role can do. That audit found a real, confirmed
secret-exposure bug: `GET /v1/analysis-config` (config-admin-service) returned the full stored
`AnalysisConfig`, including the configured AI provider's plaintext `api_key`, to any authenticated
caller regardless of role — a Viewer, the lowest tier with no write access anywhere else in the
platform, could read out the secret simply by loading the AI Analysis Config page or calling the
endpoint directly.

The Console UI's existing form also depended on this leak: it pre-filled the API key password
field with the real stored value so that "save without touching the key field" would resubmit the
same value unchanged. Redacting the read response without addressing this would turn every
prompt-only edit into an accidental key wipe.

## Decision

`GET /v1/analysis-config` now returns a dedicated `AnalysisConfigView` response type (distinct
from the internal `AnalysisConfig` storage/audit struct) where `api_key` is always `None`, plus a
new `api_key_configured: bool` so callers can distinguish "a key exists" from "no key was ever
set" without ever seeing the value. `None` was chosen over a masked placeholder string (e.g.
`"********"`) because a placeholder risks being naively round-tripped back into storage as a
literal fake credential by a client that doesn't special-case it — a strictly worse failure mode
than an empty field forcing an explicit decision.

`PUT /v1/analysis-config`'s `api_key` field became tri-state (`Option<Option<String>>`): field
omitted means "keep the existing key unchanged," explicit `null` means "clear it," and a value
means "set it." This is purely additive — every existing caller that already sends the field
explicitly is unaffected. The Console UI's form no longer prefills the key field with the real
value (impossible now that reads are redacted) and gained an explicit "clear the configured API
key" checkbox for the one case that previously relied on the leak to express itself.

## Consequences

- Any future backend endpoint that stores a secret alongside other config must apply the same
  pattern from the start: a redacted read-response type distinct from the storage type, plus a
  tri-state write field if "leave unchanged" needs to be distinguishable from "clear it" — this is
  now the established precedent, not a one-off special case for analysis config.
- The `PUT` response still echoes the real key back to whoever just submitted it (the same
  request that supplied the value) — this is not a leak, since the caller already knows what they
  just typed.
- No database schema change — this is entirely a read/write-shape fix at the API boundary.
