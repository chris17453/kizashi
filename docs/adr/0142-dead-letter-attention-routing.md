# ADR-0142: Dead-Letter Attention Routing

- Status: Accepted
- Date: 2026-07-25

## Context

The Overview and Actions surfaces exposed messages waiting in execution
dead-letter queues, but the global command-center Attention rail did not.
Operators could therefore see a healthy-looking shell while recoverable work
was waiting elsewhere in the Console.

## Decision

Include the aggregate dead-letter message count in the tenant-scoped attention
read model. The shell attention popover and `GET /api/v1/attention` expose a
dedicated recovery route to `/actions#pipeline-recovery`. A non-zero aggregate
contributes one independent attention signal, while the displayed value keeps
the actual message count.

## Consequences

Command-center navigation now reflects the same recovery posture as Overview
and Actions. The aggregation remains tenant/session scoped and treats an
unavailable queue read as zero, matching the existing resilient attention
behavior.
