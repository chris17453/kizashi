# 0087. Action Executor live-RabbitMQ integration test

## Context

CLAUDE.md §2 requires every service consuming a bus message to have a real-infra integration
test proving the full publish→consume→process path, not a mock. `normalization-service` and
`trigger-engine` already had this; `action-executor` — which consumes `event.created` in
`main.rs`'s own consumer loop — did not. Its only integration coverage was
`execution_repository_integration_test.rs` (Postgres only) and
`smtp_action_dispatcher_integration_test.rs` (a real SMTP send), neither of which touched
RabbitMQ at all, despite `main.rs`'s production code depending on it for every action it
executes.

## Decision

Added `crates/action-executor/tests/rabbitmq_integration_test.rs`, mirroring the pattern already
proven in `normalization_integration_test.rs`: publish a real `event.created` message to the
same `EVENT_CREATED_EXCHANGE` `main.rs` declares and consumes from, consume it with a test
consumer (proving the message actually round-trips through real RabbitMQ, not a private test
fixture), then call `process_event` directly — the exact function `main.rs`'s consume loop calls
for every acked delivery — against a real Postgres-backed `PostgresExecutionRepository`, a stub
Trigger Engine HTTP server returning a fixed webhook-action trigger, and a stub webhook target.
Asserts the action is dispatched (webhook target reached) and a real `ActionExecution` row lands
in Postgres with the expected `event_id`/`trigger_id`/`status`.

## Consequences

- No production code changed — this closes a test-coverage gap only.
- Same tradeoff already accepted for `normalization-service`'s equivalent test: exercises the
  crate's own processing function directly against real infra rather than spawning the actual
  `main.rs` binary as a subprocess, which stays consistent with every other service's
  integration-test shape in this codebase.
- `action-executor`'s SMTP integration test remains separately gated on `SMTP_TEST_HOST` (a real
  mail server) and continues to skip when unset — unrelated to and unaffected by this addition.
