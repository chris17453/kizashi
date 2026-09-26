# ADR-0170: Object 360 valid relationship forms

## Status

Accepted

## Context

The dynamic Object 360 relationship editor rendered an administrator delete
form inside the relationship edit form. Browsers repair nested forms
inconsistently, and the previous client-side MutationObserver workaround made
the correctness of the control depend on post-render DOM mutation.

## Decision

Render the delete form as a sibling of the edit form inside the relationship
details disclosure. Each relationship mutation therefore has an independent,
standards-compliant submission boundary, while both controls retain the same
relationship context and return route.

## Consequences

Relationship editing and deletion behave consistently across browsers and after
Object 360 refreshes. The UI no longer needs a DOM repair observer, reducing
mutation churn and keeping the authorization boundary explicit in the rendered
markup.
