// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { apiDelete, apiFetch, apiGet, apiPost } from './client'
import type { UseCaseSpec } from '../lib/useCaseSpec'
import { formatHttpErrorBody } from '../utils/apiError'
import { parseJsonResponse } from '../utils/parseJsonResponse'

export interface AgentManifest {
  template: string
  credentials?: string[]
  egress_allow_hosts?: string[]
  allow_private_networks?: boolean
  runtime_port?: number
  ttl_seconds?: number
  max_concurrent_sessions?: number
  idle_hibernate_seconds?: number
  warm_pool_size?: number
  /** Per-user home disk — sessions must pass user_id (multi-tenant Keep). */
  home_volume?: { per_user?: boolean; name?: string }
  /** node runs worker.mjs. claude, codex, and gemini run that CLI inside the template. */
  runtime?: 'node' | 'claude' | 'codex' | 'gemini'
}

export interface AgentRecord {
  name: string
  version: string
  digest_sha256: string
  manifest: AgentManifest
  created_at: string
}

export interface SessionView {
  id: string
  agent: string
  agent_version: string
  sandbox_id: string
  status: string
  input?: unknown
  created_at: string
  updated_at: string
  last_event_seq: number
  request_id?: string | null
  error?: string | null
  parent_session_id?: string | null
  user_id?: string | null
}

export async function listAgents(): Promise<{ items: AgentRecord[] }> {
  return apiGet('/api/agents')
}

export async function getAgent(name: string): Promise<AgentRecord> {
  return apiGet(`/api/agents/${encodeURIComponent(name)}`)
}

export async function deployAgent(body: {
  name: string
  bundle_base64: string
  manifest: AgentManifest
}): Promise<AgentRecord> {
  return apiPost('/api/agents', body)
}

export async function listSessions(): Promise<{ items: SessionView[] }> {
  return apiGet('/api/sessions')
}

export async function getSession(id: string): Promise<SessionView> {
  return apiGet(`/api/sessions/${encodeURIComponent(id)}`)
}

export interface KeepCockpit {
  session_id: string
  agent: string
  status: string
  tainted_by?: string[]
  taint_visible?: boolean
  /** Count of egress.connect / ebpf.* audit rows for this session. */
  egress_connects?: number
  drop_reasons?: unknown
  pending_approvals?: Array<{
    id: string
    prompt: string
    status: string
    kind?: string
    subject?: string | null
    /** For a `changeset` approval: the diff to review (see lib/changeset.ts). */
    planned_action?: unknown
  }>
  last_decisions?: Array<{
    action?: string
    phase?: string
    at?: string
    detail?: unknown
  }>
  active_goal?: {
    id: string
    title: string
    status: string
    href?: string
    plan?: Array<{
      id: string
      title: string
      status: string
      requires_approval?: boolean
    }>
  } | null
  recent_artifacts?: Array<{
    id: string
    kind: string
    title: string
    href?: string
    created_at?: string
  }>
  evidence_class?: string
  honesty?: string
  security_profile?: string | null
  browser_page?: string
  browser_view?: string
  browser_screenshot?: string
  browser_screencast?: string
  browser?: {
    ready?: boolean
    cdp?: boolean
    confined?: boolean
    enabled?: boolean
    tools_allowed?: boolean
    tainted_by?: string[]
    evidence_class?: string
    honesty?: string
    agent_paused_reason?: string | null
    network_identity?: string
    badge?: {
      evidence?: string
      operator_can_read?: boolean
      host_recover?: string
      browser?: string
      proxy?: string
      agent_paused_reason?: string | null
      network_identity?: string | null
      honesty?: string
    }
  }
  agent_paused_reason?: string | null
  browse?: {
    goal_id?: string | null
    steps?: number
    network_identity?: string | null
    cookie_jar?: string
    origins?: string[]
  }
  badge?: {
    evidence?: string
    operator_can_read?: boolean
    host_recover?: string
    browser?: string
    proxy?: string
    honesty?: string
  }
  attestation?: {
    security_profile?: string | null
    image_hash?: string | null
    evidence_class: string
    snp_launch_verified: boolean
    tdx_launch_verified: boolean
    host_recover_allowed: boolean
    operator_can_read: boolean
    honesty: string
  }
  vault?: {
    secret_backend?: string
    unwrap_required?: boolean
    unlocked?: boolean
    honesty?: string
  }
}

export interface DemoInfo {
  id: string
  title: string
  description: string
  /** Lower-case file extensions without the dot. */
  accepts: string[]
  max_bytes: number
  /** False for a use case a user deployed. */
  builtin?: boolean
  has_sample?: boolean
  /** Set when the use case sends the extracted text to a model endpoint. */
  model?: { host: string; model: string } | null
}

export interface DemoResult {
  demo?: string
  session_id: string
  agent?: string
  goal_id?: string
  artifact_id?: string
  artifact_title?: string
  artifacts?: Array<{ id: string; title: string; kind: string }>
  egress_connects?: number
  honesty?: string
  /** `sealed: false` (evidence `simulated`) means the local simulator ran it: no VM, no network policy. */
  badge?: { evidence?: string; sealed?: boolean }
  /** 0 or 1: extracted text sent to a model by the host (the cell itself stays offline). */
  model_calls?: number
  model?: { host: string; model: string; first_use_approved: boolean } | null
  filename?: string
  extract_chars?: number
  error?: string
}

/** Several files in one call: one cell per file, grouped under one batch id. */
export interface BatchResult {
  batch_id: string
  demo: string
  count: number
  ok: number
  failed: number
  egress_connects: number
  results: Array<{
    filename: string
    ok: boolean
    skipped?: boolean
    status?: number
    error?: string
    result?: DemoResult
  }>
}

/** Kept for existing imports; the PDF brief is one demo among several. */
export type PdfBriefDemoResult = DemoResult

/** The one-click Keep demos this runtime can run. */
export async function listDemos(): Promise<DemoInfo[]> {
  const out = await apiGet<{ demos: DemoInfo[] }>('/api/demos')
  return out.demos ?? []
}

/** Create or replace a user-defined use case (declarative: no code). */
export async function saveDemo(
  spec: UseCaseSpec,
): Promise<{ id: string; builtin: boolean; replaced: boolean }> {
  return apiPost('/api/demos', spec)
}

/** Remove a user-defined use case. Built-ins are refused by the runtime. */
export async function deleteDemo(id: string): Promise<void> {
  return apiDelete(`/api/demos/${encodeURIComponent(id)}`)
}

export interface KeepStatus {
  keep_mode: boolean
  signature_required: boolean
  trusted_signers: number
  fluxvm: { ready: boolean; error?: string | null }
  demo_template: string
  demos: { builtin: number; custom: number }
}

/** Keep readiness: Keep mode, trusted signers, FluxVM, demo counts. */
export function keepStatus(): Promise<KeepStatus> {
  return apiGet<KeepStatus>('/api/keep/status')
}

/**
 * Deploy a signed `.keeppack.json` (admin). The file text is sent untouched:
 * its `deploy_json` is the exact string that was signed.
 */
export async function deployPackFile(
  text: string,
): Promise<{ name: string; policy: string | null; signed: boolean }> {
  const res = await apiFetch('/api/packs', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: text,
  })
  if (!res.ok) {
    const body = await res.text().catch(() => '')
    throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
  }
  return parseJsonResponse(res)
}

/** Run one demo (multipart `file`). Omit the file to use the demo's built-in sample. */
export async function runDemo(id: string, file?: File | null): Promise<DemoResult> {
  const fd = new FormData()
  if (file) {
    fd.append('file', file, file.name || 'input')
  }
  const res = await apiFetch(`/api/demos/${encodeURIComponent(id)}`, {
    method: 'POST',
    body: fd,
  })
  if (!res.ok) {
    const body = await res.text().catch(() => '')
    throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
  }
  return parseJsonResponse<DemoResult>(res)
}

/** Run a use case on several files. A 207 (some files failed) still returns the report. */
export async function runDemoBatch(id: string, files: File[]): Promise<BatchResult> {
  const fd = new FormData()
  for (const file of files) fd.append('file', file, file.name || 'input')
  const res = await apiFetch(`/api/demos/${encodeURIComponent(id)}`, { method: 'POST', body: fd })
  if (!res.ok) {
    const body = await res.text().catch(() => '')
    throw new Error(formatHttpErrorBody(res.status, res.statusText, body))
  }
  return parseJsonResponse<BatchResult>(res)
}

/** One-click PDF → brief.md (multipart). Omit file to use the lab sample. */
export function demoPdfBrief(file?: File | null): Promise<DemoResult> {
  return runDemo('pdf-brief', file)
}

export interface BrowserView {
  mode: string
  honesty?: string
  note?: string
  screenshot?: string
  tabs: Array<{ id: string; title: string; url: string; type?: string }>
}

export interface BrowserScreenshot {
  session_id: string
  mime: string
  image_base64: string
  title?: string
  url?: string
  mode?: string
  honesty?: string
}

export async function getSessionCockpit(id: string): Promise<KeepCockpit> {
  return apiGet(`/api/sessions/${encodeURIComponent(id)}/cockpit`)
}

export async function getSessionBrowserView(id: string): Promise<BrowserView> {
  return apiGet(`/api/sessions/${encodeURIComponent(id)}/browser/view`)
}

export async function getSessionBrowserScreenshot(id: string): Promise<BrowserScreenshot> {
  return apiGet(`/api/sessions/${encodeURIComponent(id)}/browser/screenshot`)
}

export async function decideApproval(
  id: string,
  body: { decision: 'approved' | 'denied'; comment?: string },
): Promise<unknown> {
  return apiPost(`/api/approvals/${encodeURIComponent(id)}`, body)
}

export async function createSession(body: {
  agent: string
  input?: unknown
  ttl_seconds?: number
  request_id?: string
  /** Stamped by fabricd from JWT for non-admins; admins may set explicitly. */
  user_id?: string
}): Promise<SessionView> {
  return apiPost('/api/sessions', body)
}

export async function deleteSession(id: string): Promise<void> {
  return apiDelete(`/api/sessions/${encodeURIComponent(id)}`)
}

export async function sessionAction(
  id: string,
  action: 'steer' | 'cancel' | 'hibernate' | 'resume',
  body?: unknown,
): Promise<unknown> {
  return apiPost(`/api/sessions/${encodeURIComponent(id)}/${action}`, body ?? {})
}

export interface ArtifactItem {
  id: string
  kind: string
  title: string
  created_at: string
  session_id?: string | null
  agent?: string | null
  expires_at?: string | null
  metadata?: { demo?: string } | null
}

export type DiffLine = { op: 'same' | 'add' | 'del'; line: string }

export interface ArtifactDiff {
  a: { id: string; title: string; created_at: string }
  b: { id: string; title: string; created_at: string }
  summary: { added: number; removed: number; unchanged: number }
  lines: DiffLine[]
}

/** Run history across sessions (admin). Newest first. */
export async function listArtifacts(params: {
  use_case?: string
  since?: string
  limit?: number
} = {}): Promise<ArtifactItem[]> {
  const qs = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) if (v !== undefined && v !== '') qs.set(k, String(v))
  const suffix = qs.toString() ? `?${qs}` : ''
  return (await apiGet<{ items: ArtifactItem[] }>(`/api/artifacts${suffix}`)).items
}

export async function diffArtifacts(older: string, newer: string): Promise<ArtifactDiff> {
  return apiGet(`/api/artifacts/${encodeURIComponent(older)}/diff/${encodeURIComponent(newer)}`)
}

export interface ApprovalItem {
  id: string
  session_id: string
  kind: string
  subject?: string | null
  prompt: string
  status: 'pending' | 'approved' | 'denied' | string
  created_at: string
  decided_at?: string | null
}

export async function listApprovals(): Promise<ApprovalItem[]> {
  return (await apiGet<{ items: ApprovalItem[] }>('/api/approvals')).items
}

export interface AuditRow {
  seq: number
  at: string
  session_id?: string | null
  phase: string
  action: string
  subject?: string | null
  detail?: unknown
  hash: string
}

export interface AuditPage {
  items: AuditRow[]
  chain: { entries: number; chain_ok: boolean; broken_at?: number | null }
}

export async function listAudit(limit = 100, sessionId?: string): Promise<AuditPage> {
  const qs = new URLSearchParams({ limit: String(limit) })
  if (sessionId) qs.set('session_id', sessionId)
  return apiGet(`/api/audit/agent-actions?${qs}`)
}

export interface TriggerItem {
  id: string
  use_case: string
  kind: 'webhook' | 'folder'
  enabled: boolean
  runs: number
  last_run_at?: string | null
  last_error?: string | null
  dir?: string | null
  interval_seconds?: number | null
  cron?: string | null
  hook?: string | null
}

export async function listTriggers(): Promise<{ items: TriggerItem[]; watch_root_configured: boolean }> {
  return apiGet('/api/triggers')
}

/** A webhook trigger's `secret` comes back once, here, and is never listed again. */
export async function createTrigger(body: {
  use_case: string
  kind: 'webhook' | 'folder'
  dir?: string
  interval_seconds?: number
}): Promise<TriggerItem & { secret?: string }> {
  return apiPost('/api/triggers', body)
}

export async function deleteTrigger(id: string): Promise<void> {
  return apiDelete(`/api/triggers/${encodeURIComponent(id)}`)
}
