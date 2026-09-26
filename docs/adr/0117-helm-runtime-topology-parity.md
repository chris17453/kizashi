# ADR-0117: Helm Runtime Topology Parity

- **Status:** accepted
- **Date:** 2026-07-24

## Context

The Compose deployment included Incident Service, Ontology Service, and Report Scheduler, but
the Helm values omitted them. The chart therefore could not run incident workflows, ontology
backed actions, or scheduled reports, and existing query/action services lacked their ontology
service URL in Kubernetes.

## Decision

Render the three missing application Deployments and Services from the chart values. Configure
Query Gateway and Action Executor to use the chart-local Ontology Service, configure the Console
to use Incident Service, and include all three services in the Observability registry.

External infrastructure remains bring-your-own as documented by the chart; the shared Secret and
ConfigMap continue to provide database, RabbitMQ, and internal-secret settings.

## Consequences

The Helm chart now covers the same application workflow surface as Compose. Operators must
provide the existing shared Postgres/RabbitMQ dependencies and ensure migrations run at service
startup, just as they do in Compose.
