# 0089. Kubernetes Helm chart

## Context

`docs/kizashi-spec.md` §10 states the intended deployment path is docker-compose → Container
Apps → Kubernetes/Helm. Docker-compose deployability was already closed out (a shared root
`Dockerfile` with `--build-arg BIN=<name>`, and `docker-compose.yml` wiring every application
service), but no Kubernetes manifests or Helm chart existed — a confirmed, standing gap in the
"deployability" phase of the platform's own roadmap.

## Decision

New chart at `deploy/helm/kizashi/`, scoped deliberately as a **basic** v1 translation of
docker-compose.yml's existing topology — one Deployment + Service per long-running app service
(driven by a single templated `deployment.yaml`/`service.yaml` pair iterating
`values.yaml`'s `services` map, not 16 hand-written near-duplicate files), a shared ConfigMap
for non-secret env, a Secret for credentials (rendered from placeholder values by default, or
`secret.create: false` to defer to an operator-managed Secret), and CronJobs for the seven
one-shot connector pollers under `crates/connectors` (matching docker-compose's own `connectors`
profile shape — `docker compose run --rm`).

Notable judgment calls, documented in full in `deploy/helm/kizashi/README.md`:

- `retention-sweep-scheduler`/`backup-scheduler` ship as low-replica Deployments, not CronJobs —
  docker-compose already runs them as internal `while/sleep` loops, so a Deployment is the
  faithful translation; converting to a true one-shot CronJob is a separate follow-up.
- `agent-scheduler`'s ADR-0020 Docker-socket-shelling pattern is carried over via a `hostPath`
  mount + root securityContext, flagged as a real limitation (breaks on containerd-only nodes,
  grants effective node-root) with a documented Kubernetes-Jobs-API-native follow-up.
- Postgres/RabbitMQ/ClickHouse/MinIO are intentionally **not** given custom manifests — the
  README directs a production install at mature existing charts/operators (Bitnami
  postgresql/rabbitmq, clickhouse-operator, MinIO Operator) or managed equivalents, consistent
  with the spec's "no vendor lock-in, self-hosted deps" principle without reinventing
  already-solved infrastructure.
- HPA, PodDisruptionBudget, NetworkPolicy, and Ingress/TLS are explicitly out of scope for this
  v1 chart and called out in the README as documented follow-ups, not silently missing.

Every application service in `docker-compose.yml` (all 25: 16 long-running services + 2
scheduler sidecars + 7 connectors) has a corresponding chart entry — verified by diffing the
chart's `services`/`schedulers`/`connectors` map keys against `docker-compose.yml`'s service
list.

## Consequences

- No application code changed — this is new, additive deployment tooling only.
- Verified with `helm lint` (0 failures) and `helm template` (43 objects render cleanly: 18
  Deployments, 16 Services, 7 CronJobs, 1 ConfigMap, 1 Secret), plus `kubeconform` validation of
  the rendered manifests against the Kubernetes 1.29 OpenAPI schema (43 valid, 0 invalid, 0
  errors).
- `docker-compose.yml`/`Dockerfile` remain the source of truth for the service list, build args,
  and env wiring; the README states explicitly that if they drift from this chart, the chart
  should be updated to match, not the reverse.
