# ADR-0015: Console UI reverses ADR-0014's no-JS constraint — adds client-side JS for charts and components

- **Status:** accepted
- **Date:** 2026-07-18
- **Reverses:** ADR-0014 (the "no client-side JS framework" part of it — the server-rendered
  `axum` + `askama` shell, session handling, and every existing read view it established stay
  as-is; this reverses only the "graphs/components need client-side rendering" deferral)

## Context

ADR-0014 chose a server-rendered, zero-JS Console UI specifically because this build
environment has no browser-automation tooling (`chromedriver`/`geckodriver`/`wasm-bindgen-test`)
to test JS interactions the same disciplined way every other crate in this repo is tested
(`tower::ServiceExt::oneshot` against an in-process router, real assertions on real output).
That ADR explicitly anticipated this exact reversal, saying live-updating graphs and richer
interactivity would need "a small islands-of-interactivity WASM component, or a JS charting
library loaded for just that page... layered onto this server-rendered shell, not a wholesale
rewrite."

The user has now explicitly directed that the console needs real client-side interactivity —
graphs and components, "the whole 9 yards" — and rejected the no-JS constraint outright. This
is product direction, not a technical constraint that's been resolved; the testing-tooling gap
ADR-0014 was protecting against still exists and isn't being silently ignored, it's being
accepted as a tradeoff now that the product requirement outweighs it.

## Decision

Client-side JS is now in scope for `ui/`. Concretely, in priority order:

1. **Charts first**: real graphs (ingestion volume over time, event counts, connector status)
   on the Reports/Overview pages, using a small vendored (not CDN-loaded — this is an
   enterprise on-prem-capable product, it must not depend on reaching an external CDN at
   runtime) charting library, progressively enhancing the existing server-rendered HTML rather
   than replacing it. The server still renders the underlying data as HTML (table/JSON script
   tag); JS reads that and draws the chart. A JS-disabled or JS-failed page still shows correct
   data, just without the graph — this is deliberate, not a compatibility afterthought: the
   server-rendered baseline from ADR-0014 is what makes every existing page still testable via
   `tower::ServiceExt` without a browser, and stays that way.
2. **Components next**: interactive UI pieces (live-filtering tables, modals, inline
   edit) built as plain vanilla JS / Web Components against the existing server-rendered HTML,
   not a virtual-DOM framework, for the same reason — no new build toolchain, no new language
   in a Rust-only monorepo, testable incrementally.
3. **A real React SPA is explicitly NOT what this ADR authorizes.** That would mean adding a
   Node.js/npm toolchain to a Rust-only monorepo (new CI steps, new dependency ecosystem,
   `package.json`/lockfile, a JS bundler, and — critically — the same untested-tooling problem
   ADR-0014 raised, now for an entire framework instead of a no-JS constraint) and effectively
   rewriting the whole Console UI from scratch. If a full SPA rewrite is still wanted after
   seeing the charts/components approach, that is large enough to deserve its own ADR and its
   own scoped effort — not something to back into as a side effect of "add some graphs."

Every backend read path stays exactly as-is (query-gateway, config-admin-service,
ingestion-service, observability, auth-service) — this ADR only changes how the already-served
HTML is rendered/enhanced in the browser, not what data flows or where it comes from.

## Consequences

- Easier: graphs and richer interactivity are now possible without a framework migration;
  existing server-rendered pages, their tests, and the session/auth plumbing all stay valid —
  nothing built so far is thrown away.
- Harder: JS added to a page has no automated test coverage in this repo's toolchain (no
  browser driver, no JS test runner) — every JS-dependent behavior needs to be manually
  verified in a real browser and the verification explicitly reported as such (CLAUDE.md §0's
  "distinguish verified from expected" applies with extra weight here, since it's the one part
  of the stack this repo's usual TDD loop can't reach).
- A genuine SPA/component-framework rewrite remains a real possibility, but as its own future
  decision (its own ADR, its own scoped rollout), not something this change silently commits
  to.
