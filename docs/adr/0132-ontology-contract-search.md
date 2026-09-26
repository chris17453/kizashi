# ADR-0132: Ontology contract search

**Status:** Accepted  
**Date:** 2026-07-25

## Context

Global search and the command palette could find modeled object instances and action invocations,
but not the object-type and governed action-type definitions that explain how those runtime
records are interpreted. An operator with an empty or newly configured workspace therefore had no
discovery path for its ontology contracts.

## Decision

Extend the versioned global search response with bounded `object_types` and `action_types`
collections. Search matches contract names, identifiers, schemas, mappings, preconditions,
effects, and the target object-type name. The command palette renders both categories and links
to the existing Ontology and Action Library surfaces. Existing search categories and response
fields remain unchanged.

## Consequences

Operators can discover the model and governed response vocabulary before runtime data exists.
Ontology remains the source of truth; search performs tenant-scoped reads through the existing
Ontology client and does not create a second catalog. Contract results are capped at 24 per
category, matching the bounded command-surface behavior of other search categories.
