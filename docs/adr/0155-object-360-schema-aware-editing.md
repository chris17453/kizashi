# ADR-0155: Object 360 schema-aware editing

- Status: Accepted
- Date: 2026-07-25

## Context

Object 360 already exposed governed editing through the normal ontology update
path, but its form presented every modeled property as a raw JSON document. That
made a typed investigation workspace unnecessarily difficult to use and did not
match the schema-driven controls available in the main Ontology workbench.

## Decision

Object 360 renders declared object properties from the returned object-type
property schema. Strings, numbers, integers, booleans, arrays, and objects use
appropriate controls; required declarations remain required. On submit, the
controls merge into the full properties payload, preserving unknown/source fields
through the advanced JSON editor. The existing governed update endpoint remains
authoritative for authorization, schema validation, attribution, and history.

## Consequences

Operators can correct modeled state directly from an Object 360 investigation
with less syntax overhead while retaining an explicit escape hatch for fields
outside the declared contract. No new persistence or mutation path is created.
