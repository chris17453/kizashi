# ADR-0158: Record Journey Object 360 handoffs

- Status: Accepted
- Date: 2026-07-25

## Context

Record Journey is the evidence-to-action lineage surface. Its modeled-entity
and governed-decision target links still emitted legacy Ontology URLs and did
not carry the shared investigation-focus metadata, leaving that important
lineage path dependent on shell-side URL rewriting.

## Decision

Record Journey emits canonical Object 360 links for modeled entities and
governed decision targets. Entity links carry Object investigation metadata;
decision-record links carry Decision metadata. The existing evidence, action,
and lineage routes remain unchanged.

## Consequences

Evidence investigations now enter the same canonical Object 360 context as
search, cases, events, and command-palette results. The shared focus rail can
follow both the modeled target and the decision record directly from the
lineage page.
