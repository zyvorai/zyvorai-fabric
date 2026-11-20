{{- define "zyvor-merion.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "zyvor-merion.fullname" -}}
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

{{- define "zyvor-merion.selectorLabels" -}}
app.kubernetes.io/name: {{ include "zyvor-merion.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "zyvor-merion.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{ include "zyvor-merion.selectorLabels" . }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{- define "zyvor-merion.secretName" -}}
{{- .Values.security.existingSecret | default (printf "%s-secrets" (include "zyvor-merion.fullname" .)) }}
{{- end }}

{{- define "zyvor-merion.image" -}}
{{- if .Values.global.imageRegistry }}
{{- printf "%s/%s:%s" .Values.global.imageRegistry .Values.runtime.image.repository .Values.runtime.image.tag }}
{{- else }}
{{- printf "%s:%s" .Values.runtime.image.repository .Values.runtime.image.tag }}
{{- end }}
{{- end }}

{{/* Fail early on a Keep-mode install that the runtime would refuse to start. */}}
{{- define "zyvor-merion.validate" -}}
{{- if .Values.merion.mode }}
{{- if not .Values.merion.trustedSigners }}
{{- fail "merion.mode is true but merion.trustedSigners is empty: the runtime refuses to start without a trusted signer. Add a hex Ed25519 public key, or set merion.mode=false for a lab." }}
{{- end }}
{{- range .Values.merion.trustedSigners }}
{{- if not (regexMatch "^[0-9a-fA-F]{64}$" (toString .)) }}
{{- fail (printf "merion.trustedSigners entry %q must be 64 hex characters (a 32-byte Ed25519 public key)" (toString .)) }}
{{- end }}
{{- end }}
{{- end }}
{{- end }}
