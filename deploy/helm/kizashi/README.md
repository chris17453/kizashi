# Kizashi Helm chart

A **basic** chart mirroring the app-service topology already defined in `docker-compose.yml`
at the repo root: one Deployment + Service per long-running app service, a ConfigMap for
shared non-secret env, a Secret for credentials, and CronJobs for the one-shot connector
pollers. It is the "Container Apps -> Kubernetes/Helm" step from `docs/kizashi-spec.md` §10 —
not a rewrite of the deployment model, a translation of the existing one.

`docker-compose.yml` and `Dockerfile` at the repo root remain the source of truth for the
service list, build args, ports, and env wiring. If those drift from this chart, they win —
update this chart to match, not the other way around.

## Install

```sh
helm install kizashi deploy/helm/kizashi -f my-values.yaml
```

At minimum, `my-values.yaml` must override:

- `image.registry` — where your CI publishes the ~20 per-service images (each built via
  `docker build --build-arg BIN=<bin> .`, one image per `bin` named in `values.yaml`'s
  `services`/`connectors` maps).
- `secrets.*` (or set `secret.create: false` and pre-create a Secret named
  `<release>-kizashi-secrets` yourself) — every value in `values.yaml`'s `secrets:` block is a
  non-functional placeholder (`changeme`, empty string). **Never commit real credentials into a
  values file** — use `--set-file`, a values file kept out of git, or an external
  secret-management pipeline (External Secrets Operator, sealed-secrets, Vault injector, ...).
- `sharedEnv.S3_ENDPOINT_URL` / `AWS_*` if you're pointing retention-service/backup-service at
  real S3 instead of an in-cluster MinIO.
- `sharedEnv.INGESTION_GATEWAY_PUBLIC_URL` — must be the real browser-facing URL once you add
  an Ingress (see "Not included" below); the chart default is only correct for
  `kubectl port-forward`.

## What this chart deploys

| Kind | Count | What |
|---|---|---|
| Deployment | 16 | Long-running HTTP app services (`values.yaml` → `services`) — ingestion-service, normalization-service, analysis-service, trigger-engine, action-executor, agent-scheduler, dashboard-api, egress-gateway, config-admin-service, ingestion-gateway, query-gateway, retention-service, backup-service, auth-service, observability, kizashi-ui |
| Deployment | 2 | `retention-sweep-scheduler` / `backup-scheduler` — see judgment call 1 below |
| Service | 16 | ClusterIP, one per app-service Deployment, port 8080 (`egress-gateway` also exposes 3128 for its CONNECT proxy) |
| CronJob | 7 | One per connector under `crates/connectors` — generic, sql, zendesk, graph-mail, graph-teams, fabric, imap |
| ConfigMap | 1 | Shared non-secret env (`sharedEnv` in `values.yaml`), injected into every app/scheduler pod via `envFrom` |
| Secret | 0 or 1 | Rendered only if `secret.create: true` (default); holds `secrets` placeholders |

Total with default values: 18 Deployments, 16 Services, 7 CronJobs, 1 ConfigMap, 1 Secret — 43
objects, matching `helm template kizashi deploy/helm/kizashi | grep -c '^kind:'`.

Postgres, RabbitMQ, ClickHouse, and MinIO are **not** deployed by this chart — see below.

## Judgment calls made while writing this chart (please review)

1. **`retention-sweep-scheduler` and `backup-scheduler` are Deployments, not CronJobs**, despite
   the task/ADR framing them as "the Kubernetes CronJob equivalent." In `docker-compose.yml`
   both are already `alpine:3.20` + `while true; do curl ...; sleep N; done` loops — long-running
   processes, not one-shot invocations. Converting them to a real CronJob would mean rewriting
   the command to a one-shot `curl` and letting Kubernetes own the interval via `schedule:`,
   which is a bigger behavioral change than "translate docker-compose to k8s" — so this chart
   ships them as 1-replica Deployments running the *same* loop script, via
   `templates/deployment-schedulers.yaml`. If/when ADR-0011's "external scheduling (a Kubernetes
   CronJob or equivalent)" language should mean an actual CronJob, that's a small follow-up
   (drop the `while`/`sleep`, add `schedule:`), not a redesign — flagging it here rather than
   silently picking one interpretation.
2. **Connectors (`templates/cronjob-connectors.yaml`) genuinely are CronJobs** — each run is a
   bounded, one-shot process against one tenant's source, matching `docker-compose.yml`'s
   `connectors` profile (`docker compose run --rm ...`) exactly. All seven ship `suspend: true`
   by default since none has real per-tenant `TENANT_ID`/credentials/schedule yet — enabling one
   is an explicit per-tenant `values.yaml` override (see the comment above `connectors:` in
   `values.yaml`).
3. **`agent-scheduler`'s Docker-socket mount is carried over as-is** (ADR-0020: it shells out to
   `docker run` against the host's Docker socket). `templates/deployment.yaml` hostPath-mounts
   `/var/run/docker.sock` and runs the pod as root when `services.agent-scheduler.runAsRoot`/
   `dockerSocket` are true (the defaults). This is a real, known limitation, not an oversight:
   - It only works on nodes actually running `dockerd` (not containerd-only nodes, which most
     managed Kubernetes offerings default to today).
   - It grants the pod effective node-root via the socket.
   - `DOCKER_IMAGE_PREFIX`/`DOCKER_NETWORK` (in `sharedEnv`) are docker-compose concepts with no
     direct Kubernetes equivalent; they're passed through unchanged so agent-scheduler's existing
     binary doesn't need code changes, but they won't mean anything useful in-cluster.

     The honest fix is a follow-up: give agent-scheduler a Kubernetes-native invocation path
     (create a Job per due connector via the Jobs API, using a ServiceAccount scoped to
     `create`/`get`/`list`/`watch` on `jobs` in its namespace) instead of shelling to `docker
     run`. Until that lands, don't enable `agent-scheduler` against a containerd-only cluster —
     use the manually-triggered CronJobs above, or run scheduled connectors from outside the
     cluster.
4. **Every app pod gets the *entire* shared Secret and ConfigMap via `envFrom`**, not a
   per-service subset. Simpler for a v1 "basic" chart; not least-privilege (e.g.
   `dashboard-api`, which only needs `CLICKHOUSE_URL`, also receives `AWS_SECRET_ACCESS_KEY` as
   an env var it never reads). A follow-up could switch to per-service explicit
   `env: valueFrom: secretKeyRef` lists (the chart's `services.<name>.env` map already supports
   individual `env:` entries — extending that pattern to secret refs is straightforward) if an
   auditor flags the blast radius.
5. **No release-name collision guard beyond Helm's own.** Service DNS names are
   `<fullname>-<service>` (e.g. `kizashi-ingestion-service`), computed once via
   `_helpers.tpl`'s `kizashi.fullname` and reused consistently in every inter-service URL in
   `values.yaml` (via Helm's `tpl` function against the chart's root context). Installing two
   releases of this chart into the same namespace works because Helm object names are already
   release-qualified — but see point 4: they'd still share the *shape* of over-broad env
   injection, just via two separate Secrets/ConfigMaps.

## Not included (intentionally, for a v1 "basic" chart)

- **HorizontalPodAutoscaler** — every Deployment is a fixed `replicas:` from `values.yaml`
  (default 1). Add HPAs per-service once you have real load/latency data to size against.
- **PodDisruptionBudget** — nothing here survives a careless node drain gracefully at >1
  replica. Add once replica counts are actually >1 for services that need it.
- **NetworkPolicy** — every pod can currently talk to every other pod/namespace; there's no
  default-deny. Add once the in-cluster topology (namespaces, ingress path) is finalized.
- **Ingress / TLS** — nothing here is externally reachable except via `kubectl port-forward` or
  a manually-created Service of type LoadBalancer. `kizashi-ui` (the Console) and
  `ingestion-gateway` (external connector/webhook entry point) are the two services an operator
  will want fronted by a real Ingress + cert-manager-issued TLS; that's a follow-up, not
  silently missing infrastructure — `sharedEnv.INGESTION_GATEWAY_PUBLIC_URL` and
  `COOKIE_SECURE` in `values.yaml` are the two settings that need to change together once you
  add one.
- **A Kubernetes-native path for `agent-scheduler`** — see judgment call 3 above.
- **Postgres / RabbitMQ / ClickHouse / MinIO manifests** — see below.

## Infra dependencies: bring your own, on purpose

Per the project's "no vendor lock-in, self-hosted deps" principle, this chart does not hand-roll
custom manifests for Postgres, RabbitMQ, ClickHouse, or MinIO — those are already
well-solved problems with mature, widely-used charts/operators, and reinventing them here would
be the opposite of that principle. Point a production install at:

- **PostgreSQL** — the [Bitnami `postgresql`
  chart](https://github.com/bitnami/charts/tree/main/bitnami/postgresql) (or a managed service —
  RDS, Cloud SQL, Azure Database for PostgreSQL) as a separate release. Feed its connection
  string into this chart's `secrets.DATABASE_URL`.
- **RabbitMQ** — the [Bitnami `rabbitmq`
  chart](https://github.com/bitnami/charts/tree/main/bitnami/rabbitmq) or a managed AMQP
  provider. Feed its connection string into `secrets.RABBITMQ_URL` /
  `secrets.RABBITMQ_MANAGEMENT_URL`.
- **ClickHouse** — the [Altinity `clickhouse-operator`](https://github.com/Altinity/clickhouse-operator)
  (or ClickHouse Cloud). Feed its HTTP endpoint into `secrets.CLICKHOUSE_URL`.
- **MinIO / S3** — the [MinIO Operator](https://github.com/minio/operator) for self-hosted
  object storage, or real S3/Azure Blob (S3-compatible) in production. Feed the endpoint/bucket
  into `sharedEnv.S3_ENDPOINT_URL` / `sharedEnv.AWS_S3_BUCKET` / `sharedEnv.BACKUP_S3_BUCKET`
  and credentials into `secrets.AWS_ACCESS_KEY_ID` / `secrets.AWS_SECRET_ACCESS_KEY`.

For local/dev clusters, the fastest path is still `docker-compose.yml`'s four infra services
run outside the cluster with their ports reachable from it (e.g. via a `kubectl port-forward` or
a `Service` of type `ExternalName`) — not a "toy" in-chart Postgres/RabbitMQ/ClickHouse/MinIO
that would only diverge further from what production actually runs.

## Chart layout

```
deploy/helm/kizashi/
  Chart.yaml
  values.yaml                        # image, resources, secrets/sharedEnv placeholders,
                                      # per-service `services`/`schedulers`/`connectors` maps
  templates/
    _helpers.tpl                     # name/label/secret-name/configmap-name helpers
    configmap.yaml                   # shared non-secret env -> <fullname>-config
    secret.yaml                      # shared secret env -> <fullname>-secrets (if secret.create)
    deployment.yaml                  # one Deployment per entry in values.yaml `services`
    service.yaml                     # one ClusterIP Service per entry in values.yaml `services`
    deployment-schedulers.yaml       # retention-sweep-scheduler / backup-scheduler (Deployments
                                      # — see judgment call 1 above, not CronJobs)
    cronjob-connectors.yaml          # one CronJob per entry in values.yaml `connectors`
```

## Validating changes to this chart

```sh
helm lint deploy/helm/kizashi
helm template kizashi deploy/helm/kizashi | kubeconform -summary -ignore-missing-schemas -
```

Both were run against this chart as written; see the PR/commit that introduced it for the
actual output.
