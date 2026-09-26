# ADR-0185: Non-disruptive live monitoring for the audit feed

## Status

Accepted — 2026-07-25

## Context

The audit feed is an operator evidence surface, but its server-rendered rows
remain static after navigation. During incident response or access review,
operators need to know that new immutable activity has arrived without losing
the current search, day, service, or change-type scope.

## Decision

Add an opt-in live mode to `/audit-log`. Every 30 seconds it polls the
authenticated versioned audit API with the current filters, compares the
newest `changed_at` value to the page baseline, and reports new activity
without replacing the table. Refreshing the page remains explicit so an
operator never loses an open evidence diff or filter context. The preference
is stored locally per browser, and transient poll failures are visible in the
live status label.

## Consequences

The audit surface can remain open as a low-noise compliance watch while
preserving investigation focus. The browser performs no write and the API
continues enforcing tenant/session boundaries. Operators still choose when to
reload the immutable evidence view after a change is detected.
