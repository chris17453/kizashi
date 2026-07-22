# Kizashi — Session State (2026-07-21)

Handoff snapshot for continuing work in a future session. Also see `docs/features.md`
(append-only feature log) and `docs/adr/` for full history — this file is a point-in-time
summary, not the source of truth.

## Repo state

- `main` is green: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --all --check` all pass as of commit `94e49d5`.
- No open PRs, no open branches other than `main`.
- Running containers reflect the latest code: `kizashi-ui` was rebuilt/redeployed after every
  UI-affecting merge, `config-admin-service`/`normalization-service` after PR #144.

## What shipped this session (chronological, all merged to `main`)

A long self-directed "audit pass" campaign found and fixed ~15 smaller gaps first (raw-error
leaks, missing audit logging, DB-level immutability triggers, retry/dead-letter caps — see
`docs/features.md` for the full list, PRs #79–#135ish). Then, at the user's direction, the
session pivoted to a structured comparison against Keep (keephq/keep), another AIOps platform,
and shipped:

| PR | What | Notes |
|----|------|-------|
| #136 | Dead-letter queue visibility/replay | `GET/POST /v1/dead-letter[/replay]` on all 4 pipeline services |
| #137 | Event Detail page | `GET /events/:id` — payload, timeline, contributing records |
| #138 | Trigger enable/disable toggle | Parity with Sensors/Retention Policies |
| #139 | Trigger delete | New `TriggerChangeEvent` enum, full CRUD parity |
| #140 | Normalization Mapping delete | Mirrors #139 exactly (`MappingChangeEvent`) |
| #141 | **Incidents MVP** | New `incident-service` (own Postgres schema), `/incidents` list+detail, bulk "Create Incident from Selected" on Events page. See ADR-0111. |
| #142 | Sensors/Providers marketplace reskin | Pure UI — categorized card grid replacing a flat `<select>` on `/sensors/generate` |
| #143 | ADR-0112 (docs only) | Scoped the alert fingerprint/dedup MVP before implementing |
| #144 | **Alert fingerprint/dedup** | `NormalizationMapping.dedup_fields`/`dedup_window_seconds` (opt-in), SHA-256 fingerprint computed in normalization-service, new `record_fingerprints` table, suppresses `record.normalized` republish on exact duplicates. Backend-only, no UI. |

## Explicitly deferred (documented in ADRs, not started)

- **Incidents**: auto-correlation (rule-based grouping into existing open Incidents), alert
  dedup *feeding* Incidents, AI-generated summaries. (ADR-0111)
- **Alert dedup**: partial-duplicate-as-update (currently only exact-duplicate suppression
  ships); any Console UI for configuring `dedup_fields`/`dedup_window_seconds` per mapping —
  today it's API-only (`POST/PUT /v1/normalization-mappings` accepts the fields, no UI form).
  (ADR-0112)

## Open backlog (tracked as tasks, not started)

- **Task #75 — Extensible Action types**: Email-with-templates, Webhook (may partially exist
  via `ActionType::Webhook`, needs review), PDF generation, XLSX generation actions, plus a
  dedicated Console UI page for authoring Actions/Templates. Needs its own scoping ADR first —
  open question: is "Template" a first-class versioned entity (like `NormalizationMapping`) or
  something simpler? User explicitly asked for this; not yet scoped or started.
- **retention-service ops semantics**: `trigger_reimport`'s 404-vs-500 behavior — deliberately
  deferred, tracked in ADR-0103, not urgent.
- Two other Keep-inspired ideas surfaced during research but never selected by the user: a real
  Pipeline topology graph (turned out to already exist from earlier in the session —
  `ui/src/topology.rs` / `/pipeline`) and Events-table bulk actions beyond the
  create-incident one already shipped in #141.

## Operational notes for next session

- **CLAUDE.md governs**: TDD mandatory, sibling `_test.rs` files only (no inline
  `#[cfg(test)] mod tests`), 500-line file cap, `scripts/new-branch.sh` is the only way to
  create branches, ADR required for architecturally-significant decisions, `docs/features.md`
  gets an append-only entry in the *same PR* as the change.
- **Never commit directly on `main`** — this session hit that mistake twice (stashed +
  rebranched both times without losing work). Always check `git branch --show-current` before
  starting new work.
- **`docs/features.md` PR-number fixups**: never blanket-`sed` the "PR: pending" placeholder —
  use the Python `rfind`-based single-span replace already established this session (see any
  recent commit titled "docs: record PR number for ... feature entry" for the exact snippet),
  and verify via `grep -c` before/after that exactly one entry's count dropped.
- **Live verification is mandatory before calling anything done** — every PR this session was
  verified against the real `docker-compose` stack (real login, real curl/API calls, real
  Postgres queries), not just passing unit tests.
- Test tenant for live verification: `tenant_name=watkinslabs&username=operator&password=TestPassw0rd!23`
  against `http://localhost:8093/login`. Internal-secret for direct service calls:
  `X-Internal-Secret: change-me-in-production`.
- Known test/scratch data left in the `watkinslabs` tenant from live verification this session:
  a few throwaway Incidents, Triggers, NormalizationMappings (including a `dedup-test-source`
  and `log` mapping with dedup enabled), and API keys labeled `dedup-test-key*`. Not cleaned up
  — consistent with this session's convention of leaving live-verification artifacts in place
  rather than deleting them.
