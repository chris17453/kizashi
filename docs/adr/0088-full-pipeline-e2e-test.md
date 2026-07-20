# 0088. Full-pipeline e2e test

## Context

CLAUDE.md §2 has required, since day one, "Ingestion → Normalization → Analysis → Trigger →
Action chain gets end-to-end integration tests using the real docker-compose stack... not
mocks." Every individual hop already had its own real-infra integration test
(`normalization_integration_test.rs`, `trigger_integration_test.rs`, and this session's own
`action-executor/tests/rabbitmq_integration_test.rs`), but nothing exercised all five stages
back-to-back and proved a single `RawRecord` actually survives the whole chain to a dispatched
action.

## Decision

New crate `crates/e2e-tests`, holding a single test:
`tests/full_pipeline_test.rs::a_raw_record_flows_all_the_way_from_ingestion_to_a_dispatched_action`.
It depends on `ingestion-service`, `normalization-service`, `analysis-service`,
`trigger-engine`, and `action-executor` as libraries and chains them via real message-bus round
trips: publish with the upstream stage's own real publisher (declaring the same exchange
`main.rs` declares), consume with a queue bound to that real exchange, then call that stage's own
processing function (`process_normalization`, `process_batch`, `process_analyzed_record`,
`process_event`) against real Postgres/RabbitMQ/ClickHouse — the same shape every per-service
integration test in this codebase already uses, just run back-to-back instead of in isolation.
Fixture rows (a `NormalizationMapping`, an enabled `TriggerDefinition`) are inserted directly via
each stage's own repository, the same "insert the row this stage's repository reads" convention
every per-service test already follows, rather than going through config-admin-service's HTTP
API and change-event propagation (that propagation path is covered separately by
config-admin-service's own tests — this test's scope is the processing chain, not config
distribution).

Two seams are stubbed rather than run for real, consistent with how the per-service tests already
scope themselves: the AI/ML analysis call (no real Azure AI Foundry endpoint exists in any test
environment) is an in-process `AnalysisClient` stub returning a fixed
`{"sentiment_spike": 1}`, and Action Executor's Trigger Engine lookup is a stub HTTP server
returning the fixed trigger this test creates — identical to the stub
`action-executor/tests/rabbitmq_integration_test.rs` already uses for the same dependency.

## Consequences

- No production code changed.
- Verified stable across repeated local runs (3 consecutive passes) before merging, per CLAUDE.md
  §0's "run it, read the actual output" bar — flaky e2e tests erode trust faster than missing
  ones.
- The two stubbed seams mean this test doesn't catch a real Azure AI Foundry contract drift or a
  real Trigger Engine HTTP contract drift — those remain the job of `analysis-service`'s own
  `FoundryAnalysisClient` tests and `trigger-engine`'s own API tests, respectively. This test's
  job is proving the chain's own wiring (exchanges, consumers, processing functions) works
  end-to-end, not re-verifying each hop's own already-tested external contract.
