# ADR-0188: Console command frame and explicit query bars

## Status

Accepted — 2026-07-25

## Context

The server-rendered console had accumulated page-local layout rules and reused
the `inline` form class for incompatible jobs. Its base style renders labels and
controls vertically at full width, while incident, audit, and action pages used
it for multi-field filters. The result was oversized, sprawling controls and
inconsistent panel widths. Page headers also exposed related controls as an
unstructured sequence of buttons.

## Decision

Introduce a shared command frame: a bounded content canvas, consistently sized
panels, compact KPI cards, and visible action clusters. Introduce an explicit,
responsive `query-bar` contract made of `query-field` groups and a
`query-actions` rail. Filter pages opt into this contract rather than relying
on the legacy `inline` form behavior. Actions remain visible and grouped by
purpose; no generic overflow or “more” menu is used to conceal page controls.

## Consequences

Audit, Incident Queue, and governed Action history filters retain every filter
while becoming scannable on desktop and stacking predictably on narrow screens.
The legacy `inline` class remains available for existing mutation forms, so
further page migrations can be deliberate instead of risking unrelated form
behavior.
