{{/*
Expand the name of the chart.
*/}}
{{- define "rtdb.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
If release name contains chart name it will be used as a full name.
*/}}
{{- define "rtdb.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "rtdb.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "rtdb.labels" -}}
helm.sh/chart: {{ include "rtdb.chart" . }}
{{ include "rtdb.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/component: vector-database
app.kubernetes.io/part-of: rtdb
{{- end }}

{{/*
Selector labels
*/}}
{{- define "rtdb.selectorLabels" -}}
app.kubernetes.io/name: {{ include "rtdb.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "rtdb.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "rtdb.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Create the image name
*/}}
{{- define "rtdb.image" -}}
{{- $registry := .Values.image.registry -}}
{{- $repository := .Values.image.repository -}}
{{- $tag := .Values.image.tag | default .Chart.AppVersion -}}
{{- if .Values.global.imageRegistry -}}
{{- $registry = .Values.global.imageRegistry -}}
{{- end -}}
{{- printf "%s/%s:%s" $registry $repository $tag }}
{{- end }}

{{/*
Create storage class name
*/}}
{{- define "rtdb.storageClass" -}}
{{- if .Values.global.storageClass -}}
{{- .Values.global.storageClass -}}
{{- else if .Values.persistence.storageClass -}}
{{- .Values.persistence.storageClass -}}
{{- end -}}
{{- end }}

{{/*
Validate SIMDX configuration
*/}}
{{- define "rtdb.validateSIMDX" -}}
{{- if .Values.rtdb.simdx.enabled -}}
{{- if and .Values.rtdb.simdx.forceInstructionSet (not (has .Values.rtdb.simdx.forceInstructionSet (list "avx512" "avx2" "sse2" "neon"))) -}}
{{- fail "Invalid SIMDX instruction set. Must be one of: avx512, avx2, sse2, neon" -}}
{{- end -}}
{{- end -}}
{{- end }}

{{/*
Generate cluster peers list
*/}}
{{- define "rtdb.clusterPeers" -}}
{{- $fullname := include "rtdb.fullname" . -}}
{{- $namespace := .Release.Namespace -}}
{{- $port := .Values.service.grpcPort -}}
{{- range $i := until (int .Values.replicaCount) -}}
{{- if gt $i 0 }},{{ end -}}
{{ $fullname }}-{{ $i }}.{{ $fullname }}-headless.{{ $namespace }}.svc.cluster.local:{{ $port }}
{{- end -}}
{{- end }}

{{/*
Resource limits validation
*/}}
{{- define "rtdb.validateResources" -}}
{{- if .Values.rtdb.performance.memory.hugePagesEnabled -}}
{{- if not .Values.resources.limits }}
{{- fail "Resource limits must be specified when huge pages are enabled" -}}
{{- end -}}
{{- if not (hasKey .Values.resources.limits "hugepages-2Mi") -}}
{{- fail "hugepages-2Mi limit must be specified when huge pages are enabled" -}}
{{- end -}}
{{- end -}}
{{- end }}