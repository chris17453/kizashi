# ADR-0120: Shared command-center layouts

## Status

Accepted

## Context

The command center supported personal widget ordering and visibility, but operational teams could not establish a common workspace view. Sharing by accident is particularly undesirable for an operational dashboard because a layout can hide critical decision or pipeline widgets.

## Decision

Extend the existing dashboard layout contract with an explicit `scope`: `personal` or `workspace`. Personal layouts remain user-owned. Workspace layouts are tenant-scoped, owned by the logical `workspace` principal, and can be created, changed, or removed only by operators. Viewers may continue to consume the workspace layout through the read path, while unauthorized writes fail closed.

Older layouts without a scope remain personal for backward compatibility. The Console lets operators switch scope while customizing the dashboard, and the versioned API supports the same scope on GET, PUT, and DELETE.

## Consequences

- Teams can standardize an operational command-center view without copying widget settings between users.
- Scope is explicit in storage and API responses, reducing accidental visibility changes.
- A workspace layout is tenant-wide rather than tied to a separate team directory; team-specific ownership can be layered on later without changing the layout contract.
