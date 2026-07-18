# ADR-0001: Trigger condition DSL shape

- **Status:** accepted
- **Date:** 2026-07-18

## Context

Spec §11 flags "Condition DSL for triggers: full expression language vs. fixed condition
'shapes'" as an open item for sprint 0. `TriggerDefinition.condition` (spec §5.4) is
operator-authored config (spec §2 principle 5, "config over code"), evaluated by the
Aggregation/Trigger Engine on every event grouped by `group_key`. CLAUDE.md §2 requires
property/fuzz tests on this evaluator specifically because it takes untrusted config-as-data
and must never panic.

A full expression language (e.g. a boolean/arithmetic grammar with parser) is more flexible
but expands the attack/failure surface enormously: arbitrary operator precedence bugs,
recursion/stack-depth limits, injection-style edge cases, and a much larger fuzz-testing
burden before it can be called enterprise-ready. Fixed condition "shapes" are enumerable,
each one independently and exhaustively testable, and match the two patterns called out
directly in the spec's own example triggers (§1): "3 emails from the same customer" (a count
over a window) and "2 tickets with negative sentiment" (a threshold over a window).

## Decision

v1 ships `TriggerCondition` as a closed, tagged enum with two shapes:

- `CountOverWindow { count }` — fires when at least `count` matching events share a
  `group_key` within `window`.
- `ThresholdOverWindow { field, threshold, direction }` — fires when a numeric payload field
  crosses `threshold` in the given `direction` (`above`/`below`) for any matching event within
  the window.

Implemented in `crates/common/src/trigger_definition.rs`. Serialized with `#[serde(tag =
"shape")]` so the JSON config an operator writes/edits self-describes which shape it is.
`TriggerDefinition::evaluate` is a pure function over pre-aggregated inputs (event count,
extracted field values) — it does no I/O and cannot panic on any input, verified by a
`proptest` property test (`evaluate_never_panics_on_arbitrary_input`) covering the full
`f64`/`u32` input space.

Not a full expression language. If a future requirement needs compound conditions (AND/OR
across shapes, cross-field comparisons), that is a new ADR extending this enum — not a
reason to retrofit a general parser into v1.

## Consequences

- Easier: every condition shape is independently unit- and property-tested; the evaluator has
  no parser, so there is no grammar/precedence class of bugs; new shapes are additive enum
  variants, which is a backward-compatible serde change for existing stored trigger configs.
- Harder: operators cannot express compound/nested boolean logic (e.g. "count >= 3 AND avg
  score < -0.5") in v1 — they must pick one shape per trigger. Multi-condition triggers are a
  known follow-up, tracked as a future ADR when a real use case demands it, not spec'd
  speculatively now.
