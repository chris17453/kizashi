# 0074. Pipeline Map edge severity gets a visible text label

## Context

A fresh accessibility audit found the Pipeline Map (and its compact preview on the Overview
dashboard) conveys queue backlog severity — empty / building / critical — purely through the
`edge-line` element's background color (`edge-ok`/`edge-warn`/`edge-critical` CSS classes). The
visible text next to it only shows the numeric queue count (`"12 queued"` or `"n/a"`), never the
severity word itself. A color-blind user, or anyone viewing a contrast-stripped rendering,
cannot tell "building" from "critical" without cross-referencing the color legend at the bottom
of the page against each edge's rendered color by eye.

## Decision

Every topology edge (`TopologyItem::Edge`) now carries a `severity_label: &'static str` field —
`"empty"`, `"building"`, `"critical"`, or `"unknown"` — computed in Rust
(`topology::severity_label`) from the existing `severity` value, rather than compared as a
string inside the Askama template (Askama's `==` against a `&'static str` field doesn't
type-check cleanly here; computing it once in Rust sidesteps that entirely). Both templates that
render the topology (`pipeline.html`'s full map and `overview.html`'s dashboard preview) now
show `({{ severity_label }})` next to the edge, in muted text, using the same wording as the
existing color legend.

## Consequences

- Purely additive: the color-coded line and legend are unchanged, this just adds a redundant
  text channel so severity doesn't depend on color perception alone.
- `severity_label` is a small, separately-tested pure function (`topology_test.rs`), not string
  comparison inlined in two templates that could drift out of sync with each other.
