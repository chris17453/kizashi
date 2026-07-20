# 0059. Nav-wide tenant branding

## Context

ADR-0041 shipped white-label branding for the login page only, and said so explicitly: "Nav-wide
branding (post-login pages) is deferred, tracked, not done... would require threading a branding
fetch through every single page handler's template struct, since Askama's shared `layout.html`
has no cheap context-injection point independent of each page's own struct — a much larger,
separate mechanical change, not bundled in here." That's the gap this closes: a tenant that
white-labels their workspace should see their own product name and accent color on every
authenticated page, not just the one screen before they log in.

## Decision

**Response-rewriting middleware, not per-handler template fields.** Threading a `branding` field
through every one of the Console UI's ~30 Askama template structs (each already independently
compiled) would touch every page handler for one cosmetic feature. Instead,
`ui/src/branding_middleware.rs`'s `apply_branding` is layered once over the whole router
(`build_router`'s final `.layer(...)`): it lets each page render exactly as it does today, then
— only for `200 OK` `text/html` responses on a request carrying a valid session cookie — fetches
that session's tenant branding and does a plain string replace of the two fixed markers
`layout.html` always emits: the nav header's `<span class="brand-name">Kizashi</span>` and the
`--accent: #22d3ee;` CSS variable declaration.

This trades some fragility (a `layout.html` rewrite that changes that exact markup silently stops
being rewritten — no compile-time link between the middleware's string constants and the
template) for avoiding a much larger, higher-risk change across every existing page. Accepted
explicitly: `accent_color` is validated as a strict hex color server-side before it's ever stored
(ADR-0041), so the middleware doesn't need to re-escape it; `product_name` gets a small manual
HTML-entity escape (`&`, `<`, `>`) since it's free-form admin input landing inside markup.

A tenant with no branding configured (the common case — `product_name`/`accent_color` both
`None`) short-circuits before any string work: the middleware fetches branding once per
authenticated page request but only rewrites the body when there's actually something to change.

## Consequences

- **One extra backend call per authenticated page view** (`GET
  /v1/tenants/id/:id/branding`) for every signed-in request — no caching in this version. Given
  every dashboard-heavy page already fans out to 3-5 backend clients per render (Security
  Overview, the Compliance Report), this is consistent with the existing per-page latency
  profile, not a new order of magnitude. A short-TTL in-memory cache keyed by `tenant_id` is a
  natural follow-up if this becomes measurably slow in practice — not built here since it wasn't
  yet a demonstrated problem.
- **`logo_url` still isn't rendered anywhere post-login** — only `product_name` and
  `accent_color` are wired into the nav header today, since `layout.html`'s nav has no existing
  `<img>` slot for a logo to replace; adding one is a small follow-up, deliberately not bundled
  here to keep this change to the two markers that already existed.
- If `layout.html`'s nav header markup changes (a redesign, a class rename), the middleware's
  fixed string constants (`DEFAULT_BRAND_SPAN`, `ACCENT_VAR_PREFIX`) must be updated in lockstep
  — there's no compiler check tying them together. A future, more robust version could inject an
  Askama-rendered partial instead of raw string matching, if this proves to drift in practice.
- Unauthenticated pages (login, static assets) are untouched by this middleware — they already
  have their own branding mechanism from ADR-0041 (the login page's live `onblur` lookup).
