# ADR-0036: "Sensor" for deployable connector-pollers, "Agent" reserved for AI profiles

## Status

Accepted. Rollout in stages (see below) — this ADR records the decision and plan; it does not
by itself complete the rename.

## Context

`common::Agent` (a registered connector-poller instance — tenant, connector_type, name, config,
enabled, scheduling state), the `agent-scheduler` service that invokes them, `AgentRepository`,
`AgentChangeEvent`, and the Console UI's "Agents" page all use "Agent" for what is really a
**Sensor**: something that watches a source system and reports data in, with no AI/reasoning
involved in the watching itself.

Once per-tenant AI provider configuration (ADR-0031) landed, Kizashi gained a second, unrelated
concept that also deserves the word "agent" in its ordinary sense: an AI/LLM analysis profile
run against ingested data. Having both concepts coexist under the same name is a real, ongoing
source of confusion — flagged directly by the user while reviewing this session's work.

## Decision

- **Sensor** = a registered connector-poller instance. Everywhere `common::Agent`,
  `agent-scheduler`, `AgentRepository`, `AgentChangeEvent`, `agents.*` DB objects, and the
  Console UI's "Agents" nav/page/routes exist today, they become "Sensor" equivalents.
- **Agent** (if used at all going forward) is reserved for AI/LLM-driven analysis profiles —
  today that's `AnalysisConfig` and the `analysis-service`/Ollama/OpenAI-compatible provider
  work (ADR-0031, ADR-0035). No rename of `AnalysisConfig` is implied by this ADR; it just
  reserves the vocabulary so a future "Agent" concept (e.g. a named, reusable AI analysis
  profile) doesn't collide with the connector-poller meaning ever again.

### Staged rollout, not one PR

This is a large mechanical rename with real blast radius: `common::Agent` and its repository/
handler/client layers touch `config-admin-service`, `agent-scheduler`, and `kizashi-ui`; the
service name change affects `docker-compose.yml`, Docker image names, and (if the DB schema is
renamed too) a migration against a live table. Critically, **a real production `agent-scheduler`
container is actively polling a real customer mailbox while this ADR is being written** — a
rushed, single-shot rename spanning the running service's own identity is exactly the kind of
change CLAUDE.md's safety guidance calls for caution on, not a reason to skip the rename, but a
reason to sequence it:

1. **Stage 1 (this PR or shortly after): Console UI labels only.** Nav item, page headings,
   button text, route *labels* (not necessarily URL paths yet) — "Agents" → "Sensors" wherever
   an operator reads the word, with zero backend/schema changes. Lowest risk, most visible
   payoff (this is what a human actually sees and gets confused by).
2. **Stage 2: `common::Agent` → `common::Sensor` and friends** (`AgentRepository` →
   `SensorRepository`, `AgentChangeEvent` → `SensorChangeEvent`, HTTP routes) — a type/API
   rename across `config-admin-service`, `agent-scheduler`, `kizashi-ui`, with full TDD +
   live-verification per CLAUDE.md §2, done when no live customer poll is in flight or with
   explicit care around it.
3. **Stage 3: Service/infra rename** (`agent-scheduler` → `sensor-scheduler` service name,
   Docker image, `docker-compose.yml` entry, DB schema name if any) — highest blast radius,
   done last, deliberately, with the real running system's state accounted for before touching
   container identity.

## Consequences

- Until Stage 2/3 land, the codebase will have a visible mismatch (UI says "Sensors", code
  still says `Agent`) — an accepted, temporary, documented state, not an oversight.
- Every future PR touching this area should move it one stage further, not introduce new
  "Agent"-named connector-poller code that will need re-renaming later.
- No urgency to invent the actual AI-analysis-profile "Agent" concept this ADR reserves the
  name for — this ADR is about not re-colliding with it later, not a commitment to build it now.
