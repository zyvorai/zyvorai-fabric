# OpenTelemetry export

Built (tested against a local fake collector; not yet run against a real collector). Code: `agent-runtime/src/otlp.rs`.

## What it does

- Off unless `OTEL_EXPORTER_OTLP_ENDPOINT` is set. The URL must be https, or http on loopback. `OTEL_EXPORTER_OTLP_HEADERS`
  (`k=v,k2=v2`) and `OTEL_SERVICE_NAME` (default `zyvor-merion-runtime`) are read. `OTEL_EXPORTER_OTLP_PROTOCOL`, if set, must be
  `http/json`; anything else turns export off with a warning.
- **Logs** (`/v1/logs`): one record per journal row, made in `Audit::append`, the only place rows are written, so it matches the
  journal and the OCSF export row for row.
- **Traces** (`/v1/traces`): one span per decided approval, from request to decision. All records of a session share a trace id
  (the session id), so a collector can group them.
- Records go through a bounded queue (1024) and are sent every 2 seconds or 100 records. A full queue or a dead collector drops
  records; the request that caused them never waits.

## What leaves the host

The action, phase, sequence number, hash and previous hash, session id, and these detail fields when they are a string, number or
boolean: `approval_id`, `changeset_id`, `device_id`, `decision`, `exit_code`, `channel`, `push_kind`, `format`, `method`, `host`,
`kind`. Strings are cut to 200 characters and stripped of control characters.

It never sends the `error` or `tail` text, the approval prompt or comment, a preview, a request body, nested values, or the
`subject` (which can be a recipient). `ZYVOR_AGENT_OTLP_INCLUDE_SUBJECT=1` adds the subject. The OCSF export is wider; that is
for your own SIEM, while an OTLP endpoint is often shared.

## Not done

gRPC and protobuf, metrics, spans for egress decisions and speculate/apply (they appear as log records), retry of a dropped
batch, and a check against a real collector.
