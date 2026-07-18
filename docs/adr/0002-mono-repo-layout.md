# ADR-0002: Mono-repo layout

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §11 flags "Repository layout: single mono-repo (13 Rust crates/workspaces) vs.
per-service repos" as an open item for sprint 0. This formalizes the decision already made and
acted on in CLAUDE.md §1, recording the rationale as an ADR per CLAUDE.md §5 (any decision
touching a spec §11 open item gets an ADR).

Current team is a single developer (+ agent sessions), and the `common` crate (RawRecord,
Event, TriggerDefinition, NormalizationMapping, ActionExecution, EventTypeDefinition, the
`Connector` trait) changes on nearly every feature during this early build-out — every one of
the 13 services and all connectors depend on it directly. Per-service repos would mean either
publishing `common` to a private registry and version-bumping it on every change (slow,
ceremony-heavy for a fast-moving pre-v1 schema) or vendoring/submoduling it (worse). Spec §5's
data model is also explicitly the cross-service contract published over the message bus
(spec §3) — keeping producers and consumers of that contract in one repo makes a single
atomic commit able to touch a schema change and every consumer it affects at once, which a
multi-repo split would turn into a multi-PR coordination problem.

## Decision

One Cargo workspace at the repo root (`kizashi/Cargo.toml`), one crate per deployable service
under `crates/` plus shared library crates (`common`, per-connector crates under
`crates/connectors/`), and the Rust-based console under `ui/`. Layout as documented in
CLAUDE.md §1. `scripts/new-service.sh` and `scripts/new-connector.sh` scaffold new crates
directly into this workspace.

## Consequences

- Easier: atomic cross-service commits when `common`'s schemas change; one CI pipeline, one
  `cargo test --workspace` / `cargo clippy --workspace` covers everything; no internal package
  registry or version-pinning ceremony needed pre-v1.
- Harder: independent release cadences per service aren't possible without extra tooling
  (path-filtered CI jobs, per-crate versioning) — not needed yet at this team size, but is the
  concrete trigger for revisiting this ADR: if/when services get independent deploy schedules
  or the team splits by service, that's a new ADR reversing this one, not a silent drift into
  per-service repos.
