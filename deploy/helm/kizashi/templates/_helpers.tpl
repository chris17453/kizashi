{{/*
Standard chart name/label helpers.
*/}}

{{- define "kizashi.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Full release-qualified name, used as a prefix for every object and every in-cluster DNS
reference in values.yaml's templated env strings (e.g. INGESTION_SERVICE_URL). Truncated to 63
chars (k8s object name limit) minus room for the longest per-service suffix this chart appends
(e.g. "-normalization-service", 22 chars) so generated names never get silently truncated mid
service-name.
*/}}
{{- define "kizashi.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 41 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 41 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 41 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "kizashi.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "kizashi.labels" -}}
helm.sh/chart: {{ include "kizashi.chart" . }}
{{ include "kizashi.selectorLabels" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{- define "kizashi.selectorLabels" -}}
app.kubernetes.io/name: {{ include "kizashi.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{/*
Name of the shared Secret every app pod's envFrom references. Set `secretName` in values.yaml
to point at an operator-managed Secret instead of the one this chart renders.
*/}}
{{- define "kizashi.secretName" -}}
{{- default (printf "%s-secrets" (include "kizashi.fullname" .)) .Values.secretName -}}
{{- end -}}

{{/*
Name of the shared ConfigMap every app pod's envFrom references.
*/}}
{{- define "kizashi.configMapName" -}}
{{- printf "%s-config" (include "kizashi.fullname" .) -}}
{{- end -}}
