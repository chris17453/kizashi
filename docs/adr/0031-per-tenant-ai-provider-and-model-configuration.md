# ADR-0031: Per-tenant AI provider and model configuration

- **Status:** accepted
- **Date:** 2026-07-19

## Context

`analysis-service` has exactly one `AnalysisClient` implementation, `FoundryAnalysisClient`,
hardcoded to Azure AI Foundry's bespoke batch-analysis contract (`api-key` header,
`{tenant_id, inputs, prompt}` → `{results}`), wired from required env vars
(`AZURE_AI_FOUNDRY_ENDPOINT`/`AZURE_AI_FOUNDRY_API_KEY`) at process startup — one provider,
platform-wide, no per-tenant choice. The only tenant-configurable piece today is a free-text
`prompt` (`AnalysisConfig`, ADR-0019).

Kizashi is resold to other companies (CLAUDE.md's framing throughout), each of whom likely has
their own AI infrastructure preferences and constraints — some already on Azure AI Foundry,
some wanting a local/self-hosted model (Ollama) for data-residency reasons, some on a generic
OpenAI-compatible endpoint. A single platform-wide provider forces every tenant onto whatever
the operator happened to configure at deploy time, which doesn't fit a multi-tenant resold
product's reality. Per-tenant configuration (each tenant's `AnalysisConfig` picks its own
provider/model/endpoint) fits that story better than a platform-wide switch — and is the
smallest reversible extension of the existing per-tenant `prompt` field, not a new concept.

## Decision

**Extend `AnalysisConfig`** with four new optional fields: `provider` (`"azure_foundry"` |
`"openai_compatible"`, defaults to `azure_foundry` when unset — today's existing global
behavior is unchanged for every tenant that never sets these), `model` (string, required only
for `openai_compatible`), `endpoint` (string, overrides the platform default), `api_key`
(string, per-tenant credential). All additive — a `None`/default config behaves exactly as
before this ADR.

**New `OpenAiCompatibleAnalysisClient`**, a second `AnalysisClient` implementation targeting the
standard `/v1/chat/completions` shape — the one API surface Ollama, OpenAI, and Azure OpenAI
(in OpenAI-compatible mode) all actually implement, so one client covers all three rather than
one bespoke client per vendor. Unlike `FoundryAnalysisClient`'s single batched call, this
issues one chat-completion request per record: a batch endpoint isn't part of the standard
chat-completions contract, and asking a general chat model to return "N JSON results for N
inputs" in one response is unreliable (models truncate, drop entries, or malform the array) —
sequential per-record calls trade some latency for correctness, which the analysis pipeline
already tolerates (it's async off the ingestion path, not user-facing-synchronous). Each
record's raw/normalized payload plus the tenant's `prompt` become the user message; the
assistant's reply is parsed as JSON if possible, wrapped as `{"text": "<raw reply>"}` otherwise
— never panics or fails the whole batch over one model returning prose instead of JSON.

**`process_batch` resolves the client per tenant**, not once at startup: it reads the full
`AnalysisConfig` (not just `prompt` as before), and for `provider: openai_compatible` builds an
`OpenAiCompatibleAnalysisClient` from that config's `endpoint`/`api_key`/`model`; otherwise it
uses the existing platform-wide `deps.analysis_client` (still `FoundryAnalysisClient` by
default). This is genuinely built per call rather than cached, since a misconfigured or
rotated per-tenant credential must take effect on the very next batch, not after a restart.

**Secret storage**: a tenant's `api_key` is stored in plaintext in `analysis_configs`, same as
every other config-as-data field in this table, not a vault/KMS-backed secret. This matches the
security posture already in place for `EGRESS_PROXY`-adjacent config elsewhere in the codebase
(no secrets-manager integration exists anywhere in this repo yet) — documented here as a known,
accepted interim posture for a resold enterprise product, not silently glossed over. A proper
secrets-manager integration is real follow-up work, not blocking this ADR.

## Consequences

- Easier: `AnalysisClient` was already a trait, so a second implementation is genuinely
  additive — zero changes to `FoundryAnalysisClient`, zero changes to any tenant that doesn't
  opt in. A third provider (a real dedicated Azure OpenAI or Anthropic client, if the OpenAI-
  compatible shape ever proves insufficient for one of them) is the same shape of change again.
- Harder: per-record sequential calls mean an `openai_compatible` tenant's batch latency scales
  with batch size, unlike Foundry's one-call-per-batch. If that becomes a real bottleneck,
  concurrent per-record calls (bounded parallelism) is the natural follow-up — not built ahead
  of a demonstrated need. Storing `api_key` in plaintext is a real compliance gap for a product
  audited by customers' compliance teams (CLAUDE.md §5); tracked here explicitly as follow-up
  work (a secrets-manager-backed field), not something this ADR pretends is already solved.
