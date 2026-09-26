# ADR-0154: Object 360 Typed Action Parameters

- Status: Accepted
- Date: 2026-07-25

## Context

Object 360 could execute available governed actions, but the first execution
surface exposed only a raw JSON parameter editor. That made the contract less
usable and left operators to translate the action schema manually.

## Decision

Render action fields from each contract's JSON Schema in the Object 360
workspace. String, numeric, boolean, array, and object properties receive
appropriate controls; required fields and defaults are honored. Before submit,
the browser serializes those typed controls into the existing `parameters`
payload, while the governed backend remains authoritative for validation.

## Consequences

Object 360 action execution now matches the typed contract experience of the
Action Center without duplicating server-side mutation semantics. Complex
schemas remain representable through array/object JSON controls, and invalid
values are caught before a governed invocation is submitted.
