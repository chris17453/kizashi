# ADR-0127: Corrected pipeline graph flow

## Status

Accepted

## Context

The typed Pipeline Map graph exposed all of the right operational nodes, but its final edges were
misordered: Action Executor pointed back to Signal aggregation before reaching Governed response.
That contradicted the actual `event.created` flow and could lead operators to infer a feedback loop.

## Decision

Represent the final boundary in the graph as Trigger Engine → Signal aggregation → Action Executor
→ Governed response. The `event.created` queue remains the telemetry-bearing edge into the signal
aggregation node, followed by explicit dispatch and response-recording edges. The physical queue
consumer remains unchanged; this is a correction to the operator-facing graph model.

## Consequences

- The live topology now reads in the same direction as the platform's operating chain.
- Queue counts and service deep links remain intact.
- Signal aggregation is no longer shown as downstream of action execution.
