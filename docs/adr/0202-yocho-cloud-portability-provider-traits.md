# ADR-0202: Cloud portability via provider traits (keys, models, training)

- **Status:** proposed (default adopted for Phase 0; reversible)
- **Date:** 2026-09-26

## Context

Kizashi spec §2.3: no cloud-native-only managed services in the core architecture; everything
must run on Azure or AWS. Yochō's plan uses Azure Key Vault, Azure ML, Foundry with managed
identity, and Exchange RBAC for Applications.

Today `analysis-service` already has an `AnalysisClient` trait with Foundry and
OpenAI-compatible implementations (API-key auth) and a fallback provider. Field encryption uses
one static AES-256-GCM key from an environment variable; there is no secret-provider trait.

## Decision

Azure services are allowed as **implementations behind traits**, never as hard dependencies:

| Trait (crate) | First implementation | Also required |
| --- | --- | --- |
| `KeyProvider` (`yocho-crypto`) | Azure Key Vault | Local provider (keys wrapped by a master key from env) for dev, CI and non-Azure deploys |
| `ModelProvider` (`yocho-model`) | Foundry router: PTU first, spill to pay-as-you-go, Batch API for backfill | Mock HTTP endpoint for dev/CI; OpenAI-compatible provider |
| `LocalModelRuntime` (`yocho-model`) | `ort` (ONNX Runtime) | none; ONNX is portable |
| `TrainingBackend` (`yocho-model`) | Azure ML job submission | none required in Phase 0–2 |

- `ModelProvider` generalises the existing `AnalysisClient`; analysis-service moves onto it
  later rather than keeping two LLM abstractions. That migration is its own PR.
- Auth to Azure uses workload or managed identity through the Azure identity SDK when running in
  Azure; API keys remain supported for other providers.
- Exchange RBAC for Applications is a tenant-admin configuration, not a code dependency; the
  Graph Sensor works with any `Mail.Read` grant and only *documents* RBAC scoping.

## Consequences

- Yochō runs fully on a laptop with the local key provider and the mock model endpoint, which is
  what integration tests use.
- An AWS deployment works without Azure for everything except the Foundry and Azure ML
  implementations; it would need its own `ModelProvider`/`TrainingBackend` implementation.
