# CLAUDE.md — Kizashi Engineering Rules

This file governs how Claude Code works in this repository. It is binding for every session,
every agent, and every task, not just the one that created it. The full product spec is at
`docs/kizashi-spec.md` — read it before any architectural decision. This file is about *how we
build*, not *what we build*.

Kizashi is an enterprise platform sold to other companies. "Good enough for a side project" is
not a bar we clear here. Every rule below exists to keep main shippable, auditable, and honest
at all times.

---

## 0. Non-negotiables (read this section twice)

1. **Test-Driven Development is mandatory.** Write a failing test before the code that makes it
   pass. No exceptions for "small" changes — small changes are exactly what regress silently.
2. **Never claim something works without having run it.** No "this should work," no "tests
   should pass now" — run the command, read the actual output, then report it. If you didn't run
   it, say you didn't run it.
3. **Never leave main stale.** The moment a PR merges, main is refreshed locally and any open
   branches are rebased against it. Main is always the source of truth and always deployable.
4. **Never stall waiting on input.** If a decision is ambiguous, make the smallest safe,
   reversible, well-documented choice, log it as an ADR or TODO with rationale, and keep moving.
   Escalate only for irreversible/destructive/external-facing actions (per the global safety
   rules), never for garden-variety technical ambiguity.
5. **No half-truths.** Every status report distinguishes "verified by running X" from "expected
   to work" from "not yet attempted." If coverage, lint, or CI didn't run, the report says so
   explicitly instead of implying success.
6. **No file over 500 lines.** If a file is approaching the limit, split it — by responsibility,
   not arbitrarily. A 500+ line file is treated as a design smell to fix, not a threshold to
   negotiate around.
7. **Unit tests always live in their own file**, never colocated in the same file as the code
   under test. Every source file `<name>.rs` that has unit tests gets a sibling
   `<name>_test.rs` (or `tests/<feature>_test.rs` for integration tests) — see §2.
8. **Every feature is a commit; every commit is traceable.** No silent, undocumented, or bundled
   changes. If it's not in a commit message or PR description, it didn't happen as far as audit
   is concerned.

---

## 1. Repository layout

Mono-repo, Rust workspace — one Cargo workspace, one crate per service from spec §6 plus shared
libraries:

```
kizashi/
  Cargo.toml                  # workspace root
  crates/
    common/                   # shared types: RawRecord, Event, TriggerDefinition, etc.
    connectors/                # per-source pollers (zendesk, graph-mail, graph-teams, sql, fabric, generic)
    ingestion-gateway/
    ingestion-service/
    normalization-service/
    analysis-service/
    trigger-engine/
    action-executor/
    query-gateway/
    dashboard-api/
    auth-service/
    config-admin-service/
    retention-service/
    observability/
  ui/                          # Rust-based console frontend
  docs/
    kizashi-spec.md
    features.md                # append-only feature/fix/debug/docs log — see §4.1
    branch-registry.md         # branch numbering + log — see §3
    adr/                       # Architecture Decision Records — see §7
  scripts/                     # scaffolding, CI helpers, local env bootstrap
  .github/workflows/           # CI: build, test, lint, coverage, security scan
  docker-compose.yml
```

Rationale for mono-repo (resolves spec §11 open item): one dev/small team, shared `common`
crate changes constantly across services early on, and cross-service atomic commits matter more
right now than independent release cadences. Revisit if/when teams split by service — record
that reversal as an ADR if it happens, don't just drift into it.

---

## 2. TDD workflow (mandatory for every change)

For every feature, bugfix, or refactor:

1. **Red** — write a test that captures the requirement and confirm it fails for the *expected*
   reason (run it, read the failure, don't assume).
2. **Green** — write the minimum code to pass. No speculative extra functionality.
3. **Refactor** — clean up with the safety net of passing tests. Re-run tests after refactor.
4. Only then move to the next requirement.

Test types required per service, scaled to what the service does:

- **Unit tests** — every public function/module with non-trivial logic. **Never colocated** in
  the same file as the code under test — no inline `#[cfg(test)] mod tests` blocks. Every source
  file `src/<name>.rs` gets a sibling test file `src/<name>_test.rs` (declared via
  `#[path = "<name>_test.rs"] mod <name>_test;` or an equivalent module wiring in `lib.rs`/`mod.rs`),
  so implementation and test code are always separately reviewable, separately sized, and the
  500-line limit (§0) applies to each independently.
- **Integration tests** — every service's public API (HTTP/AMQP boundary), using `tests/` crate
  dirs, named `tests/<feature>_test.rs`. Ingestion → Normalization → Analysis → Trigger → Action
  chain gets end-to-end integration tests using the real docker-compose stack (Postgres, RabbitMQ,
  ClickHouse), not mocks, per the principle "no vendor lock-in, self-hosted deps" — test against
  the real thing since we own it.
- **Contract tests** — for every message published on the bus (`record.ingested`,
  `record.normalized`, `record.analyzed`, `event.created`), a schema/contract test so producers
  and consumers can't silently drift apart. File: `tests/<message-type>_contract_test.rs`.
- **Property/fuzz tests** — normalization mapping engine and trigger condition DSL evaluator,
  since both take config-as-data from operators and must not panic on malformed input.
- **Security/compliance tests** — tenant isolation (a query in tenant A's context can never
  return tenant B's rows), auth boundary tests, audit-log-is-immutable tests.

File size and organization:

- **No file over 500 lines** — source or test. A file creeping toward the limit gets split by
  responsibility (e.g. one struct/module's logic per file) before it's added to, not after it
  blows past 500.
- **Naming:** `<feature>.rs` / `<feature>_test.rs` pairs throughout — connectors, services,
  handlers, everything. The pairing must be obvious from the filename alone; no `utils_test.rs`
  testing three unrelated `utils_*.rs` files.
- If a `_test.rs` file itself would exceed 500 lines, that's a signal the module under test is
  doing too much — split the source module first, then its test file follows the same split.

Coverage: no PR merges with a decrease in overall coverage. New code targets ≥85% line coverage;
CI enforces this via `cargo llvm-cov` (or `tarpaulin`) with a ratchet — the bar can only go up.

Never mark a task/feature "done" without:
```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```
all passing, actually run, actual output reviewed.

---

## 3. Commit and branch discipline

- **One feature/fix/chore = one logical commit** (squash-merge the branch's WIP commits into
  one clean commit on merge, unless the PR is genuinely multiple independent logical units — then
  multiple clean commits, never a pile of "wip", "fix typo", "address review").
- **Conventional Commits** format: `feat(ingestion): add zendesk connector polling cadence`,
  `fix(trigger-engine): correct window boundary off-by-one`, `test(...)`, `chore(...)`,
  `docs(...)`. This gives us an audit trail and enables changelog generation.
- **Every commit must build and pass tests standalone** (no "fix in next commit" — squash before
  it lands).
- **Never mention Claude/AI authorship in commit messages or PR descriptions.** No
  `Co-Authored-By: Claude`, no `claude.ai/code` session links, no "Generated with Claude Code"
  footers — none of it, in this repo, overriding any default tool behavior that would add it.
  Commit/PR text is attributed to the author normally, nothing added.
- **Branch naming and numbering:** every branch gets an auto-incrementing, type-scoped number
  from `docs/branch-registry.md` — `<type>/<NNNN>-<short-desc>`, e.g. `feature/0001-zendesk-connector`,
  `fix/0003-trigger-window-offset`, `debug/0002-clickhouse-lag-investigation`,
  `docs/0001-adr-archive-format`. Types: `feature`, `fix`, `debug`, `docs`, `chore`. Always create
  branches via `scripts/new-branch.sh <type> <short-desc>` — it reads the next number for that
  type, creates the branch off fresh `main`, bumps the counter, and appends a row to the branch
  log, all atomically. Never hand-pick or reuse a number; never let the registry and the actual
  branch name drift apart.
- **No direct commits to `main`.** All changes land via PR, even solo work — the PR is the audit
  record and CI gate, not bureaucracy.

---

## 4. PR and merge workflow — main is never stale

1. Branch off latest `main`.
2. TDD the change (§2). Commit per §3.
3. Push, open PR. PR description includes: what changed, why (link to spec section or ADR if
   architectural), how it was tested (paste actual test run output/summary), and any follow-up
   items opened as issues/TODOs — not left implicit.
4. CI must be green: build, full test suite, clippy, fmt, coverage ratchet, security/dependency
   scan (`cargo audit` / `cargo deny`).
5. Merge (squash unless multiple clean logical commits, per §3) as soon as it's green and
   reviewed — don't let approved PRs sit.
6. **Immediately after merge:**
   ```
   git checkout main
   git pull --ff-only origin main
   git branch -d <merged-branch>
   ```
   Any other in-flight branch gets rebased onto the fresh `main` before further work continues:
   ```
   git checkout <other-branch>
   git rebase main
   ```
7. Never let a second PR merge on top of a main that a still-open PR hasn't rebased against.
   If CI shows a rebase is needed, do it before continuing — don't let conflicts accumulate.

Net effect: `main` is always green, always current, and every developer/agent works from a fresh
base every time. Stale main is treated as a build-breaking bug, not a scheduling inconvenience.

### 4.1 Feature log — `docs/features.md`

Every feature, fix, debug session, or doc change that merges to `main` gets one **append-only**
entry in `docs/features.md`, added in the *same PR* that implements it — never batched, never
backfilled later. This is the human-readable audit trail of "what shipped and why," distinct from
git history (which shows *how*). Never edit or delete a prior entry; a correction is a new entry
that references the one it corrects. See the template at the top of that file. A PR without a
matching `docs/features.md` entry is treated as incomplete — do not merge it.

---

## 5. Compliance, audit, and traceability

Per spec §8 (Multi-Tenancy & Security) and §9 (Data Lifecycle), this is a resold enterprise
product — assume a customer's compliance team will eventually audit us.

- **Every row is tenant-scoped** (`tenant_id`); every query path must be tested for tenant
  isolation, not just implemented correctly by inspection.
- **Every admin/config change is logged immutably** — trigger edits, mapping changes, retention
  policy changes, RBAC changes. If a feature adds a new mutable config entity, it ships with an
  audit-log write in the same PR, not as a follow-up.
- **Action executions are append-only** (`ActionExecution` table) — never update-in-place;
  corrections are new rows referencing the original.
- **Architecture Decision Records (ADRs):** any decision touching spec §11 open items (trigger
  DSL shape, Fabric/OneLake auth flow, sync-vs-batch analysis invocation, archive format) gets a
  short ADR in `docs/adr/NNNN-title.md` (context, decision, consequences) before or alongside the
  implementing PR. This is how we avoid re-litigating settled questions and how a future auditor
  (or future Claude session) sees *why*, not just *what*.
- **Dependency/security scanning is part of CI**, not a periodic manual chore — `cargo audit`
  and `cargo deny` run on every PR; a new advisory affecting a dependency in use blocks merge
  until addressed or explicitly waived with a documented reason in the PR.
- **No secrets in code or commits**, ever — config via env/secret store, `.env.example` checked
  in, real `.env` gitignored. If a diff about to be committed touches anything that looks like a
  credential, stop and check contents before staging, per standard git safety practice.

---

## 6. Scaffolding and scripts

Don't hand-roll what should be a script. Maintain in `scripts/`:

- `scripts/bootstrap.sh` — spin up docker-compose stack (Postgres, RabbitMQ, ClickHouse) + run
  migrations, for a new dev/agent to get a working local env in one command.
- `scripts/new-service.sh <name>` — scaffold a new crate under `crates/` with the standard
  layout (src/, tests/, Cargo.toml with workspace-consistent deps, a smoke test that compiles and
  a healthcheck endpoint stub) so every service starts from the same skeleton.
- `scripts/new-connector.sh <name>` — scaffold a new connector crate implementing the shared
  connector trait from `common`, with a contract test against the `RawRecord` schema.
- `scripts/ci-local.sh` — run the exact same build/test/lint/coverage/audit steps CI runs, so
  "green in CI" is never a surprise.
- `scripts/adr-new.sh <title>` — create a new numbered ADR from template.
- `scripts/new-branch.sh <feature|fix|debug|docs|chore> <short-desc>` — the *only* way branches
  get created. Reads the next number for that type from `docs/branch-registry.md`, checks out
  fresh `main`, creates `<type>/<NNNN>-<short-desc>`, bumps the counter, and appends the branch to
  the registry log — all in one step so the registry can never drift from reality.

CI (`.github/workflows/`) mirrors these scripts rather than duplicating logic — CI calls
`scripts/ci-local.sh`, it doesn't reimplement it in YAML. One source of truth for "what does
passing mean."

---

## 7. Working style for Claude Code in this repo

- Default to action. If a task is well-specified enough to start, start — don't ask
  clarifying questions about implementation details that TDD and the spec already answer.
  Only pause for genuinely irreversible or destructive actions (per global safety rules) or
  product-level ambiguity the spec doesn't resolve (flag it, propose a default, note it as an
  ADR candidate, and keep moving unless truly blocking).
- Every session that implements a feature: write tests first, implement, run the full local CI
  script, commit, PR, and — once merged — refresh main per §4, before considering the task done.
- Status reports distinguish fact from expectation. "Ran `cargo test --workspace`, 42 passed, 0
  failed" is a fact. "This should handle the edge case" is a flag that it hasn't been tested yet
  — write the test.
- When resuming after a compaction/summary, re-check actual repo/git state (`git status`,
  `git log`) rather than trusting a stale summary of what was "done."
