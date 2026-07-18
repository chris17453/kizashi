# ADR-0014: Console UI v1 scope: server-rendered Rust web app, shell plus read-only events/triggers/health views

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §7 describes an extensive console: shell/nav, a live topology graph, configurable
drag-arrange dashboards, scheduled PDF/email reporting, event type management, a trigger
builder with dry-run mode, a data lifecycle UI, RBAC/admin UI, and full API-surface parity —
styled after OpenShift/Instana. CLAUDE.md §1 places it at `ui/` as a "Rust-based console
frontend," which is compatible with either a client-side WASM SPA (Yew/Leptos/Dioxus) or a
server-rendered Rust web app; the spec doesn't mandate which.

Every other service in this repo is tested the same way: `tower::ServiceExt::oneshot` against
an in-process `axum::Router`, no browser, no headless-browser driver, no JS test runner
anywhere in the toolchain (CLAUDE.md §2's TDD requirement has been satisfied this way for all
thirteen other crates). A WASM SPA's natural test story (`wasm-bindgen-test` against a real
browser via `chromedriver`/`geckodriver`) requires tooling not present anywhere else in this
build, and CLAUDE.md §0 requires tests be run and their actual output read before anything is
called done — introducing a testing methodology this repo has no other example of, for the
single highest-uncertainty piece of the stack, is a bad place to take on that risk.

Every backend capability the console needs already exists as a JSON API (query-gateway/
dashboard-api for events, config-admin-service for triggers/mappings, observability for
health, auth-service for login) — the console's job is presenting them, not inventing new
backend logic.

## Decision

`ui/` is a server-rendered Rust web app: `axum` + `askama` (compile-time-checked HTML
templates, consistent with this codebase's preference for compile-time-checked correctness
over runtime string templating) rendering full HTML pages, no WASM build step, no client-side
JS framework. This makes it testable with exactly the same `tower::ServiceExt` pattern as
every other service — a request goes in, HTML comes out, assertions run on the response body —
no new test infrastructure, no browser automation dependency added to CI.

v1 ships:
- **Console shell**: left nav, dark-mode-first layout matching spec §7's OpenShift/Instana
  styling direction (CSS only, no theming engine yet).
- **Login**: a form posting to Auth Service's local-login endpoint; on success, the UI's own
  backend stores `{bearer_token, tenant_id}` in a server-side session map keyed by a random
  session id set as an `HttpOnly` cookie — Auth Service itself has no session/cookie layer
  (ADR-0009 says explicitly "that's Console UI's job once built"), so this is that job, done as
  simply as correctness allows: an in-memory map, not a JWT/signed-cookie scheme, since the UI
  process is itself stateless-service-shaped and doesn't need distributed session validation
  for v1.
- **Events view**: reads `GET /v1/events` through Query Gateway using the session's bearer
  token, rendered as a table.
- **Triggers view**: reads `GET /v1/trigger-definitions` through Config/Admin Service,
  read-only list (no create/edit form — that's the "trigger builder UI" spec §7 describes, a
  materially bigger piece deferred whole, not half-built).
- **Platform health view**: reads `GET /v1/health` through Observability, rendered as a
  per-service status list.

Explicitly deferred, each a real scoped follow-up rather than a stub page: topology graph,
configurable/drag-arrange dashboards, scheduled PDF/email reporting, event type management UI,
trigger builder with dry-run, data lifecycle UI (retention policy CRUD from the browser),
RBAC/admin UI, per-tenant branding/theming, and full API-surface parity (only four of the
platform's read/write surfaces are wired up).

## Consequences

- Easier: the whole UI is testable exactly like every other crate in this repo — no new CI
  dependency, no browser driver flakiness, no separate build toolchain (`trunk`/`wasm-pack`)
  to install and pin versions for; a page that renders wrong is a unit-test-catchable string
  assertion, not something that only fails visually in a browser.
- Harder: no client-side interactivity beyond what plain HTML forms/links provide in v1 (no
  live-updating dashboards, no drag-and-drop, no client-side graph rendering for topology) —
  every meaningful interaction is a full page load. This is a real UX ceiling, not a
  implementation detail; if/when the topology graph or configurable dashboards are built, that
  work will likely need client-side rendering (a small islands-of-interactivity WASM component,
  or a JS charting library loaded for just that page) layered onto this server-rendered
  shell, not a wholesale rewrite — the shell, session handling, and read views built here stay
  useful either way.
