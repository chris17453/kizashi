# ADR-0121: Governed action-template contract validation

## Status

Accepted

## Context

Action templates are reusable response contracts selected by triggers. Previously, Config/Admin accepted any JSON object and the first meaningful validation happened only when Action Executor attempted delivery. That made authoring errors look like runtime delivery failures and weakened operator confidence in governed automation.

## Decision

Validate the minimum provider contract in the shared `common` crate and invoke it from both Config/Admin and the Console form path. HTTP-shaped providers and artifact actions require a non-empty endpoint. Email accepts one of the supported HTTP relay, SMTP, or Microsoft Graph shapes and requires the corresponding sender and recipient fields for SMTP/Graph delivery. Provider-specific optional fields remain extensible through advanced JSON.

The Console presents structured fields for the common provider contract, synchronizes them into the persisted JSON, and retains an advanced JSON editor. The server remains authoritative and rejects invalid contracts regardless of the authoring surface.

## Consequences

- Invalid action templates fail before they can be selected by a trigger.
- The same contract rules apply to browser and API authors.
- Provider-specific extensions remain possible without a schema migration.
- Secrets may still be present only in the existing protected configuration paths; the structured editor does not expose or persist them outside the submitted provider contract.
