# ADR-0041: Tenant white-label branding (product name, logo, accent color)

## Status

Accepted.

## Context

The spec states Kizashi "is designed to be white-labelable and multi-tenant" (§1), but nothing
in the platform ever implemented that — no branding configuration existed anywhere, backend or
UI. Scoped down deliberately (per direct guidance): branding is CSS styling and a logo/icon,
nothing more elaborate — no theming engine, no per-page customization, no email-template
branding. Three fields: product name, logo URL, accent color.

## Decision

- **Storage**: three nullable columns added directly to `auth-service`'s existing `tenants`
  table (`product_name`, `logo_url`, `accent_color`), not a separate table — one optional row of
  metadata per tenant, no join needed, and `auth-service` already owns tenant identity. `NULL`
  means "use the platform default," so a tenant that never touches branding renders identically
  to today — no migration-day surprise, no "unset and broken" state.
- **Read is by workspace name** (`GET /v1/tenants/:name/branding`), **deliberately
  unauthenticated**: the one caller that needs this before anything else is the login page,
  which hasn't authenticated anyone yet. Branding isn't sensitive — workspace names are already
  visible in any URL a customer's operators share with each other. A second endpoint
  (`GET /v1/tenants/id/:id/branding`) serves the authenticated Settings page, which only ever
  has a `tenant_id` from the session, never the name.
- **Write is admin-only** (`PUT /v1/tenants/:id/branding`), audit-logged with the real actor
  (ADR-0039) — a workspace-wide identity change, not a per-user preference.
- **`accent_color` is validated server-side** against a strict hex-color pattern
  (`#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa`). This isn't pedantry: the Console UI renders it directly
  into a `<style>` block on the unauthenticated login page (`.login-card { --accent: ...; }`).
  Unvalidated, an admin-supplied value there is a CSS injection vector on a page every visitor
  who knows a workspace name can reach — CSS can't execute script, but it can break layout or
  attempt attribute-selector-based data exfiltration. Rejecting anything that isn't a real hex
  color closes this off entirely rather than trusting the renderer's escaping to be sufficient.
- **Login page loads branding live**: the workspace name field's `onblur` reloads
  `/login?tenant_name=...`, which looks up and applies that workspace's branding before the
  operator even submits credentials — "loaded based on login," not a separate preview step.
  Lookup failure (unknown workspace, backend down) silently falls back to platform defaults
  rather than erroring, since this fires while the user may still be mid-typing.
- **Scope explicitly stops at the login page for this PR.** Applying branding to every
  authenticated page (the nav header, etc.) would require threading a branding fetch through
  every single page handler's template struct, since Askama's shared `layout.html` has no cheap
  context-injection point independent of each page's own struct — a much larger, separate
  mechanical change, not bundled in here.

## Consequences

- A tenant can now genuinely white-label their login experience — real product requirement,
  previously entirely unimplemented despite being named in the spec's first paragraph.
- Nav-wide branding (post-login pages) is deferred, tracked, not done — see "Scope" above.
- `logo_url` has no equivalent format validation (unlike `accent_color`) — it renders into an
  `<img src>` attribute, which Askama HTML-escapes by default (handles quote-breakout); a
  malicious `javascript:`/`data:` URL there fails to load as an image in modern browsers rather
  than executing, a materially lower risk than the CSS injection `accent_color` posed, which is
  why only the latter got a strict allow-list.
