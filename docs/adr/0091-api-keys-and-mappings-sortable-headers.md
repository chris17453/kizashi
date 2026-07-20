# 0091. Sortable headers for API Keys and Field Mappings

## Context

The sixth UI audit pass found `api_keys.html` and `normalization_mappings.html` had working
`?q=` search but plain, non-sortable `<th>` headers, unlike every peer list page (Sensors,
Users, Sessions, Triggers) which all use clickable sort-header links since ADR-0070 established
the pattern. Same "peer-page inconsistency" bug class earlier passes already fixed for search,
pagination, and RBAC visibility.

## Decision

Both pages gained `sort`/`dir` query fields and an in-handler `sort_rows` helper, applied after
the existing search filter on the already-fetched full list (neither page paginates, so no
"only sorts the current page" caveat applies here, unlike the server-paginated list pages) — API
Keys sorts by `label`/`created_at`, Field Mappings sorts by `source_type`/`version`. Both
templates gained clickable sort-header `<a>` links in the exact shape every peer page already
uses, and `sort`/`dir` hidden inputs on the search form so search and sort compose.

## Consequences

- No backend change — both `ApiKeysClient::list_api_keys` and
  `NormalizationMappingsClient::list_mappings` already return the full list; sorting is
  client-request-scoped, same as every other in-handler-filter page.
- `api_keys_handler_test.rs` (527 lines) exceeded the 500-line limit after adding a sort test —
  split into GET (`api_keys_handler_test.rs`) and mutation (`api_keys_handler_mutations_test.rs`)
  files, same split shape ADR-0090 already applied to `sensors_handler_test.rs`/
  `users_handler_test.rs`.
