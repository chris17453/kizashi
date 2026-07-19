# ADR-0035: Bounded concurrency for OpenAI-compatible analysis calls

## Status

Accepted

## Context

ADR-0031 shipped `OpenAiCompatibleAnalysisClient` making one sequential HTTP call per record
(justified there because chat-completions isn't a batch API). Observed live against a real
backlog: reprocessing 631 real emails through a local `qwen3:8b` Ollama model at concurrency 1
processed roughly 1-3 records per minute — a multi-hour wait for what should be a routine
catch-up sweep, purely because each request waited for the previous one to fully finish
(network round trip + the model's own reasoning/generation time) before starting the next.

## Decision

`OpenAiCompatibleAnalysisClient::analyze_batch` now runs up to `concurrency` requests in flight
at once (`futures_util::stream::iter(...).buffered(concurrency)`), default 4, configurable per
process via `ANALYSIS_OPENAI_CONCURRENCY` (threaded through `AnalysisDeps` so it's testable
without an env var, same pattern as `ANALYSIS_BATCH_SIZE`/`ANALYSIS_BATCH_MAX_WAIT_MS`).
`buffered` (not `buffer_unordered`) is deliberate: it preserves the order results are yielded
in relative to the order the futures were created, so `results[i]` still corresponds to
`records[i]` with no separate re-sort step — `process_batch`'s `records.into_iter().zip(results)`
pairing stays correct unchanged.

Payloads are cloned into owned values before building the futures (rather than borrowing from
`records`), which was necessary to satisfy the borrow checker across the closure boundary
`stream::iter(...).map(...)` requires — a mechanical consequence of async closures capturing
borrowed iterator items, not a design choice with its own tradeoffs worth re-litigating.

### Why not increase batch size instead

`ANALYSIS_BATCH_SIZE` controls how many `record.normalized` messages one `process_batch` call
covers, not how they're processed once collected — a bigger batch without concurrency would
still process every record in that larger batch strictly one-at-a-time, making the problem
worse (a longer serial queue), not better.

### Why 4 as the default

Conservative starting point for a locally-hosted model that may itself have limited parallel
inference capacity (Ollama's own `OLLAMA_NUM_PARALLEL` setting bounds how many requests it
actually processes concurrently regardless of how many this client sends). An operator running
against a hosted API (OpenAI, Azure OpenAI) with higher rate limits and more backend capacity
can raise `ANALYSIS_OPENAI_CONCURRENCY` accordingly; one running a small local model on modest
hardware may need to lower it. Not tuned against a specific production workload — a reasonable
default, not a measured optimum.

## Consequences

- A tenant's `OpenAiCompatibleAnalysisClient` batches now complete in roughly
  `ceil(batch_size / concurrency)` sequential round trips instead of `batch_size` — the real
  backlog-processing time improvement this ADR exists for.
- `FoundryAnalysisClient` (the Azure AI Foundry platform-default client) is unaffected — it
  already sends the whole batch as one HTTP call, this ADR only concerns the per-record
  OpenAI-compatible path.
- Higher concurrency means more simultaneous load on whatever backend a tenant configured — an
  operator raising this value is responsible for confirming their backend (local Ollama
  instance or hosted API) can actually sustain it; this client does not auto-detect or
  rate-limit against the backend's real capacity.
