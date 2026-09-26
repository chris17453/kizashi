# ADR-0114: Native PDF and XLSX response artifacts

## Context

Action templates can now describe reusable responses, but executive and operational handoffs also
need portable artifacts. Sending a PDF/XLSX label to an arbitrary webhook would leave generation
to every downstream integration and would not provide a consistent audit boundary.

## Decision

Add `GeneratePdf` and `GenerateXlsx` action types. Action Executor generates a bounded, event-scoped
artifact from the action's optional `body_template`, then posts the bytes to the configured
delivery URL through the existing tenant-aware Egress boundary. PDF uses a minimal valid PDF
document; XLSX uses Excel-compatible SpreadsheetML so no managed or native office dependency is
required. The dispatch result records format and byte count in the existing append-only action
execution detail.

## Consequences

- Trigger and template authoring can select artifact actions without a separate worker.
- Delivery targets still own storage and distribution; Kizashi does not leak raw artifact URLs.
- Payload size and line count are bounded to keep event-driven execution predictable.
- A future artifact-storage service can replace delivery URLs without changing the action contract.
