# Branch Registry

Tracks every branch ever created, by type, with an auto-incrementing per-type counter. This is
the single source of truth for the next branch number — never guess or reuse a number, always
read the counter below, use it, then increment it in the same commit that creates the branch.

Managed by `scripts/new-branch.sh` (see CLAUDE.md §6/§8) — use that script rather than hand-editing
counters where possible, but the table itself is the audit record and must stay in sync even for
manually created branches.

## Counters (next number to use, per type)

| Type          | Prefix     | Next number |
|---------------|------------|-------------|
| feature       | `feature/` | 0001        |
| fix           | `fix/`     | 0002        |
| debug         | `debug/`   | 0001        |
| docs          | `docs/`    | 0002         |
| chore         | `chore/`   | 0002         |

## Branch log (append-only, newest last)

| # | Branch                       | Type    | Created    | Status | PR | Notes |
|---|-------------------------------|---------|------------|--------|----|-------|
| 0001 | `chore/0001-bootstrap-scaffolding` | chore | 2026-07-18 | merged | #1 | |
| 0001 | `docs/0001-adr-open-items` | docs | 2026-07-18 | merged | #2 | |
| 0001 | `fix/0001-branch-registry-order` | fix | 2026-07-18 | open | pending | |
