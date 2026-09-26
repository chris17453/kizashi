# ADR-0190: Rich ontology property contract

## Status

Accepted — 2026-07-25

## Context

ADR-0189 requires a shared, extensible property vocabulary for operational
models. The existing ontology property schema only validates JSON primitives.
That is insufficient for identity, temporal, monetary, file-evidence, spatial,
vector, nested, and relationship values, and it gives modelers no consistent
place to record source ownership or governed write intent.

Existing tenants already use primitive `property_schema` definitions and may
project additive source fields before a model definition catches up. The new
contract therefore cannot invalidate those existing definitions or reject
undeclared properties by default.

## Decision

Introduce a shared validation contract at ontology object and relationship
write boundaries. It retains `string`, `number`, `integer`, `boolean`,
`object`, and `array`, and adds `uuid`, `float`, `decimal`, `money`, `date`,
`timestamp`, `file`, `blob`, `image`, `vector`, `coordinate`, `enum`, and
`reference`.

Rich values use lossless and inspectable JSON shapes: decimals are strings;
money has decimal `amount` and ISO-style uppercase `currency`; media values
are object-storage references with `uri`, `sha256`, and `content_type`; vectors
are numeric arrays with an optional fixed dimension; coordinates have bounded
latitude and longitude. Arrays can declare item contracts, and objects can
declare nested property contracts.

Definitions may carry `display`, `sensitivity`, `provenance`,
`source_ownership`, and `write_policy` metadata. The initial validator confirms
that these metadata values have safe structural forms; policy enforcement and
authoritative write-back routing are separate data/workflow-plane slices.
Unknown properties remain additive, preserving current source-projection
behavior. A malformed declared definition or a declared value that does not
satisfy its contract is rejected with `400 Bad Request`.

## Consequences

Models and relationship types now have a stable value vocabulary that future
Build Studio, pipeline, evidence, and app surfaces can share. The contract is
centralized rather than being reimplemented in every handler, validated before
model definitions are persisted, and tested independently from HTTP routing.
Future work must add richer constraint vocabulary, source-specific provenance
capture, and enforcement for source ownership/write policy before enabling
external write-back.
