// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { useCallback, useEffect, useState } from 'react'
import { Download, FlaskConical, GitBranch, ListChecks, Plus, Trash2 } from 'lucide-react'
import {
  AiRouter,
  RouteDecision,
  RouterReport,
  Trace,
  TraceFilter,
  TracePolicy,
  decideRoute,
  deleteRouter,
  deleteTracePolicy,
  exportTraces,
  listRouters,
  listTracePolicies,
  listTraces,
  promoteVariant,
  routerReport,
  saveRouter,
  saveTracePolicy,
  scoreTrace,
} from '../api/aiImprove'
import { InferenceEndpoint, listEndpoints } from '../api/ai'
import { Card, EmptyState, Modal, PageHeader } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { useToastContext } from '../contexts/ToastContext'
import { usePageLoader } from '../hooks/usePageLoader'
import { toastFailure } from '../utils/toastError'
import {
  autoScoreText,
  buildPolicy,
  formatCost,
  formatPValue,
  formatPercent,
  formatScore,
  lastUserMessage,
  meanLogprob,
  parseRouterJson,
  routerTemplate,
  traceTime,
} from '../utils/aiImprove'

type Tab = 'traces' | 'capture' | 'routers'

const th = 'px-4 py-3'
const headRow = 'text-left text-[var(--zf-muted)] border-b border-[var(--zf-hairline)]'
const row = 'border-b border-[var(--zf-hairline)]'

export default function AiImprove() {
  const toast = useToastContext()
  const [tab, setTab] = useState<Tab>('traces')
  const [endpoints, setEndpoints] = useState<InferenceEndpoint[]>([])
  const [policies, setPolicies] = useState<TracePolicy[]>([])
  const [routers, setRouters] = useState<AiRouter[]>([])
  const [traces, setTraces] = useState<Trace[]>([])
  const [filter, setFilter] = useState<TraceFilter>({ limit: 50 })
  const { loading, loadError, run } = usePageLoader('Failed to load AI traces and routers')

  const load = useCallback(
    () =>
      run(async () => {
        const [e, p, r, t] = await Promise.all([
          listEndpoints(),
          listTracePolicies(),
          listRouters(),
          listTraces(filter),
        ])
        setEndpoints(Array.isArray(e) ? e : [])
        setPolicies(Array.isArray(p) ? p : [])
        setRouters(Array.isArray(r) ? r : [])
        setTraces(Array.isArray(t) ? t : [])
      }),
    [run, filter],
  )

  useEffect(() => {
    void load()
  }, [load])

  const tabs: { id: Tab; label: string; icon: typeof ListChecks }[] = [
    { id: 'traces', label: 'Traces', icon: ListChecks },
    { id: 'capture', label: 'Capture', icon: Download },
    { id: 'routers', label: 'Routers & A/B', icon: GitBranch },
  ]

  return (
    <div>
      <PageHeader
        title="AI Improve"
        description="Beta: record what the gateway serves, route requests across models, and compare arms with A/B reports."
        onRefresh={() => void load()}
        refreshing={loading}
      />
      <PageLoadBanner title="Could not load AI traces" headline={loadError} onRetry={() => void load()} />

      <div className="flex gap-2 mb-4 flex-wrap" role="tablist" aria-label="AI Improve sections">
        {tabs.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={tab === t.id}
            className={`zf-btn zf-btn-sm ${tab === t.id ? 'zf-btn-primary' : 'zf-btn-ghost'}`}
            onClick={() => setTab(t.id)}
          >
            <t.icon className="w-3.5 h-3.5" /> {t.label}
          </button>
        ))}
      </div>

      {tab === 'traces' && (
        <TracesTab
          traces={traces}
          filter={filter}
          setFilter={setFilter}
          routers={routers}
          endpoints={endpoints}
          reload={load}
          onError={(l, e) => toastFailure(toast, l, e)}
        />
      )}
      {tab === 'capture' && (
        <CaptureTab
          endpoints={endpoints}
          policies={policies}
          reload={load}
          onDone={(m) => toast.success(m)}
          onError={(l, e) => toastFailure(toast, l, e)}
        />
      )}
      {tab === 'routers' && (
        <RoutersTab
          endpoints={endpoints}
          routers={routers}
          reload={load}
          onDone={(m) => toast.success(m)}
          onError={(l, e) => toastFailure(toast, l, e)}
        />
      )}
    </div>
  )
}

interface TabCommon {
  reload: () => Promise<unknown>
  onError: (label: string, e: unknown) => void
}

// ---- Traces ----------------------------------------------------------------

function TracesTab({
  traces,
  filter,
  setFilter,
  routers,
  endpoints,
  reload,
  onError,
}: TabCommon & {
  traces: Trace[]
  filter: TraceFilter
  setFilter: (f: TraceFilter) => void
  routers: AiRouter[]
  endpoints: InferenceEndpoint[]
}) {
  const [open, setOpen] = useState<Trace | null>(null)
  const variants = Array.from(
    new Set(routers.flatMap((r) => r.candidates.map((c) => c.name))),
  ).sort()

  const download = async (format: 'raw' | 'sft') => {
    try {
      const text = await exportTraces({ ...filter, format, limit: 10000 })
      const url = URL.createObjectURL(new Blob([text], { type: 'application/x-ndjson' }))
      const a = document.createElement('a')
      a.href = url
      a.download = `traces-${format}.jsonl`
      a.click()
      URL.revokeObjectURL(url)
    } catch (e) {
      onError('Export failed', e)
    }
  }

  const update = (patch: Partial<TraceFilter>) => setFilter({ ...filter, ...patch })

  return (
    <div>
      <div className="flex gap-3 mb-4 flex-wrap items-end">
        <label className="text-xs">
          <span className="block mb-1 text-[var(--zf-muted)]">Endpoint</span>
          <select
            className="zf-input"
            value={filter.endpoint ?? ''}
            onChange={(e) => update({ endpoint: e.target.value || undefined })}
          >
            <option value="">All</option>
            {endpoints.map((e) => (
              <option key={e.name} value={e.name}>
                {e.name}
              </option>
            ))}
          </select>
        </label>
        <label className="text-xs">
          <span className="block mb-1 text-[var(--zf-muted)]">Arm</span>
          <select
            className="zf-input"
            value={filter.variant ?? ''}
            onChange={(e) => update({ variant: e.target.value || undefined })}
          >
            <option value="">All</option>
            {variants.map((v) => (
              <option key={v} value={v}>
                {v}
              </option>
            ))}
          </select>
        </label>
        <label className="text-xs">
          <span className="block mb-1 text-[var(--zf-muted)]">Scored</span>
          <select
            className="zf-input"
            value={filter.scored === undefined ? '' : String(filter.scored)}
            onChange={(e) =>
              update({ scored: e.target.value === '' ? undefined : e.target.value === 'true' })
            }
          >
            <option value="">Any</option>
            <option value="true">Scored</option>
            <option value="false">Unscored</option>
          </select>
        </label>
        <button type="button" className="zf-btn zf-btn-sm zf-btn-ghost" onClick={() => void download('sft')}>
          <Download className="w-3.5 h-3.5" /> Export SFT
        </button>
        <button type="button" className="zf-btn zf-btn-sm zf-btn-ghost" onClick={() => void download('raw')}>
          <Download className="w-3.5 h-3.5" /> Export raw
        </button>
      </div>

      {traces.length === 0 ? (
        <EmptyState
          icon={<ListChecks className="w-8 h-8" />}
          title="No traces"
          description="Turn on capture for an endpoint in the Capture tab, then send traffic through its gateway."
        />
      ) : (
        <Card className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className={headRow}>
                <th className={th}>Time</th>
                <th className={th}>Endpoint</th>
                <th className={th}>Arm</th>
                <th className={th}>Status</th>
                <th className={th}>Latency</th>
                <th className={th}>Tokens</th>
                <th className={th}>Score</th>
                <th className={th}>Prompt</th>
              </tr>
            </thead>
            <tbody>
              {traces.map((t) => (
                <tr key={t.id} className={row}>
                  <td className={`${th} text-xs`}>{traceTime(t.created_unix)}</td>
                  <td className={th}>{t.endpoint}</td>
                  <td className={th}>{t.variant ?? '—'}</td>
                  <td className={th}>{t.status}</td>
                  <td className={th}>{t.latency_ms} ms</td>
                  <td className={`${th} text-xs`}>
                    {t.prompt_tokens} / {t.completion_tokens}
                  </td>
                  <td className={th}>{formatScore(t.score)}</td>
                  <td className={th}>
                    <button
                      type="button"
                      className="text-left underline decoration-dotted max-w-xs truncate block"
                      onClick={() => setOpen(t)}
                    >
                      {t.redacted ? '(redacted)' : lastUserMessage(t).slice(0, 60) || t.id}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}

      <TraceDetail
        trace={open}
        onClose={() => setOpen(null)}
        onScored={(t) => {
          setOpen(t)
          void reload()
        }}
        onError={onError}
      />
    </div>
  )
}

function TraceDetail({
  trace,
  onClose,
  onScored,
  onError,
}: {
  trace: Trace | null
  onClose: () => void
  onScored: (t: Trace) => void
  onError: (label: string, e: unknown) => void
}) {
  const [busy, setBusy] = useState(false)
  if (!trace) return null
  const lp = meanLogprob(trace)
  const score = async (value: number) => {
    setBusy(true)
    try {
      onScored(await scoreTrace(trace.id, value))
    } catch (e) {
      onError('Could not save score', e)
    } finally {
      setBusy(false)
    }
  }
  return (
    <Modal open onClose={onClose} className="max-w-2xl">
      <div className="p-5 space-y-3">
        <h2 className="text-lg font-semibold">Trace</h2>
        <p className="font-mono text-xs break-all">{trace.id}</p>
        <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-sm">
          <dt className="text-[var(--zf-muted)]">Served by</dt>
          <dd>
            {trace.target_endpoint}
            {trace.variant ? ` (arm ${trace.variant})` : ''}
          </dd>
          <dt className="text-[var(--zf-muted)]">Model</dt>
          <dd>{trace.model}</dd>
          <dt className="text-[var(--zf-muted)]">Status / latency</dt>
          <dd>
            {trace.status} · {trace.latency_ms} ms{trace.streamed ? ' · streamed' : ''}
          </dd>
          <dt className="text-[var(--zf-muted)]">Finish reason</dt>
          <dd>{trace.finish_reason ?? '—'}</dd>
          <dt className="text-[var(--zf-muted)]">Token IDs</dt>
          <dd>
            {trace.prompt_token_ids
              ? `${trace.prompt_token_ids.length} prompt, ${trace.completion_token_ids?.length ?? 0} completion`
              : 'not captured'}
            {lp != null ? ` · mean logprob ${lp.toFixed(3)}` : ''}
          </dd>
          <dt className="text-[var(--zf-muted)]">Score</dt>
          <dd>
            {formatScore(trace.score)}
            {trace.score_source ? ` (${trace.score_source})` : ''}
          </dd>
        </dl>
        {trace.redacted ? (
          <p className="text-sm text-[var(--zf-muted)]">
            Redacted: only the request hash and counts were kept. SHA-256 {trace.request_sha256.slice(0, 16)}…
          </p>
        ) : (
          <>
            <div>
              <h3 className="text-xs uppercase text-[var(--zf-muted)] mb-1">Request</h3>
              <pre className="zf-input w-full font-mono text-xs whitespace-pre-wrap max-h-48 overflow-auto">
                {JSON.stringify(trace.request?.messages ?? trace.request ?? null, null, 2)}
              </pre>
            </div>
            <div>
              <h3 className="text-xs uppercase text-[var(--zf-muted)] mb-1">Completion</h3>
              <pre className="zf-input w-full font-mono text-xs whitespace-pre-wrap max-h-48 overflow-auto">
                {trace.completion ?? '—'}
              </pre>
            </div>
          </>
        )}
        <div className="flex gap-2 justify-between items-center">
          <div className="flex gap-2">
            <button type="button" className="zf-btn zf-btn-sm zf-btn-ghost" disabled={busy} onClick={() => void score(1)}>
              Good (1.0)
            </button>
            <button type="button" className="zf-btn zf-btn-sm zf-btn-ghost" disabled={busy} onClick={() => void score(0.5)}>
              Okay (0.5)
            </button>
            <button type="button" className="zf-btn zf-btn-sm zf-btn-ghost" disabled={busy} onClick={() => void score(0)}>
              Bad (0.0)
            </button>
          </div>
          <button type="button" className="zf-btn zf-btn-sm zf-btn-primary" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </Modal>
  )
}

// ---- Capture ---------------------------------------------------------------

function CaptureTab({
  endpoints,
  policies,
  reload,
  onDone,
  onError,
}: TabCommon & {
  endpoints: InferenceEndpoint[]
  policies: TracePolicy[]
  onDone: (m: string) => void
}) {
  const [endpoint, setEndpoint] = useState('')
  const [sampleRate, setSampleRate] = useState('1')
  const [redact, setRedact] = useState(false)
  const [tokenIds, setTokenIds] = useState(false)
  const [autoScore, setAutoScore] = useState('')
  const [formError, setFormError] = useState<string | null>(null)

  const save = async () => {
    const built = buildPolicy({ endpoint, sampleRate, redact, tokenIds, autoScore })
    if ('error' in built) {
      setFormError(built.error)
      return
    }
    setFormError(null)
    try {
      await saveTracePolicy(built.policy)
      onDone(`Capture enabled on ${endpoint}`)
      await reload()
    } catch (e) {
      onError('Could not save capture policy', e)
    }
  }

  const remove = async (name: string) => {
    try {
      await deleteTracePolicy(name)
      onDone(`Capture disabled on ${name}`)
      await reload()
    } catch (e) {
      onError('Could not remove capture policy', e)
    }
  }

  return (
    <div className="space-y-4">
      <Card className="p-4">
        <h2 className="text-sm font-semibold mb-3">Capture traffic on an endpoint</h2>
        <div className="flex gap-3 flex-wrap items-end">
          <label className="text-xs">
            <span className="block mb-1 text-[var(--zf-muted)]">Endpoint</span>
            <select className="zf-input" value={endpoint} onChange={(e) => setEndpoint(e.target.value)}>
              <option value="">Choose…</option>
              {endpoints.map((e) => (
                <option key={e.name} value={e.name}>
                  {e.name}
                </option>
              ))}
            </select>
          </label>
          <label className="text-xs">
            <span className="block mb-1 text-[var(--zf-muted)]">Sample rate (0–1)</span>
            <input
              className="zf-input w-28"
              inputMode="decimal"
              value={sampleRate}
              onChange={(e) => setSampleRate(e.target.value)}
            />
          </label>
          <label className="text-xs">
            <span className="block mb-1 text-[var(--zf-muted)]">Auto-score</span>
            <input
              className="zf-input w-56"
              placeholder="finish_stop, json_valid, contains:…"
              value={autoScore}
              onChange={(e) => setAutoScore(e.target.value)}
            />
          </label>
          <label className="text-xs flex items-center gap-2 pb-2">
            <input type="checkbox" checked={tokenIds} onChange={(e) => setTokenIds(e.target.checked)} />
            Token IDs &amp; logprobs
          </label>
          <label className="text-xs flex items-center gap-2 pb-2">
            <input type="checkbox" checked={redact} onChange={(e) => setRedact(e.target.checked)} />
            Redact content
          </label>
          <button type="button" className="zf-btn zf-btn-sm zf-btn-primary" onClick={() => void save()}>
            <Plus className="w-3.5 h-3.5" /> Enable
          </button>
        </div>
        {formError && (
          <p role="alert" className="mt-2 text-xs text-[var(--zf-danger,#d33)]">
            {formError}
          </p>
        )}
        <p className="mt-3 text-xs text-[var(--zf-muted)]">
          Traces can contain user data. Redact when prompts are sensitive. Token IDs are requested from vLLM and
          are only returned by backends that support <code>return_token_ids</code>.
        </p>
      </Card>

      {policies.length === 0 ? (
        <EmptyState
          icon={<Download className="w-8 h-8" />}
          title="Capture is off everywhere"
          description="fabricctl ai trace enable ENDPOINT --token-ids --auto-score finish_stop"
        />
      ) : (
        <Card className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className={headRow}>
                <th className={th}>Endpoint</th>
                <th className={th}>Sample</th>
                <th className={th}>Token IDs</th>
                <th className={th}>Redact</th>
                <th className={th}>Auto-score</th>
                <th className={th}>
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {policies.map((p) => (
                <tr key={p.endpoint} className={row}>
                  <td className={`${th} font-medium`}>{p.endpoint}</td>
                  <td className={th}>{formatPercent(p.sample_rate)}</td>
                  <td className={th}>{p.capture_token_ids ? 'yes' : 'no'}</td>
                  <td className={th}>{p.redact ? 'yes' : 'no'}</td>
                  <td className={`${th} font-mono text-xs`}>{autoScoreText(p.auto_score) || '—'}</td>
                  <td className={th}>
                    <button
                      type="button"
                      className="zf-btn zf-btn-sm zf-btn-ghost"
                      aria-label={`Disable capture on ${p.endpoint}`}
                      onClick={() => void remove(p.endpoint)}
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </div>
  )
}

// ---- Routers & A/B ---------------------------------------------------------

function RoutersTab({
  endpoints,
  routers,
  reload,
  onDone,
  onError,
}: TabCommon & {
  endpoints: InferenceEndpoint[]
  routers: AiRouter[]
  onDone: (m: string) => void
}) {
  const [editing, setEditing] = useState<string | null>(null)
  const [text, setText] = useState('')
  const [formError, setFormError] = useState<string | null>(null)
  const [selected, setSelected] = useState<string | null>(null)

  const startNew = () => {
    const front = endpoints[0]?.name ?? 'front-door'
    setText(routerTemplate(front, endpoints.slice(1).map((e) => e.name)))
    setFormError(null)
    setEditing('new')
  }
  const startEdit = (r: AiRouter) => {
    setText(JSON.stringify(r, null, 2))
    setFormError(null)
    setEditing(r.endpoint)
  }
  const apply = async () => {
    const parsed = parseRouterJson(text)
    if ('error' in parsed) {
      setFormError(parsed.error)
      return
    }
    try {
      await saveRouter(parsed.router)
      onDone(`Router saved for ${parsed.router.endpoint}`)
      setEditing(null)
      await reload()
    } catch (e) {
      setFormError(e instanceof Error ? e.message : String(e))
    }
  }
  const remove = async (name: string) => {
    try {
      await deleteRouter(name)
      onDone(`Router removed from ${name}`)
      if (selected === name) setSelected(null)
      await reload()
    } catch (e) {
      onError('Could not remove router', e)
    }
  }

  return (
    <div className="space-y-4">
      <div>
        <button type="button" className="zf-btn zf-btn-sm zf-btn-primary" onClick={startNew}>
          <Plus className="w-3.5 h-3.5" /> New router
        </button>
      </div>

      {routers.length === 0 ? (
        <EmptyState
          icon={<GitBranch className="w-8 h-8" />}
          title="No routers"
          description="A router sits behind a front-door endpoint and picks a model arm per request: rules, cost, or an A/B split."
        />
      ) : (
        <Card className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className={headRow}>
                <th className={th}>Front door</th>
                <th className={th}>Strategy</th>
                <th className={th}>Arms</th>
                <th className={th}>Control</th>
                <th className={th}>
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {routers.map((r) => (
                <tr key={r.endpoint} className={row}>
                  <td className={`${th} font-medium`}>{r.endpoint}</td>
                  <td className={`${th} font-mono text-xs`}>{r.strategy}</td>
                  <td className={`${th} text-xs`}>
                    {r.candidates
                      .map((c) => (r.strategy === 'split' ? `${c.name} ×${c.weight ?? 1}` : c.name))
                      .join(', ')}
                  </td>
                  <td className={th}>{r.control ?? r.candidates[0]?.name}</td>
                  <td className={`${th} flex gap-1`}>
                    <button
                      type="button"
                      className="zf-btn zf-btn-sm zf-btn-ghost"
                      onClick={() => setSelected(selected === r.endpoint ? null : r.endpoint)}
                    >
                      <FlaskConical className="w-3.5 h-3.5" /> Report
                    </button>
                    <button type="button" className="zf-btn zf-btn-sm zf-btn-ghost" onClick={() => startEdit(r)}>
                      Edit
                    </button>
                    <button
                      type="button"
                      className="zf-btn zf-btn-sm zf-btn-ghost"
                      aria-label={`Remove router from ${r.endpoint}`}
                      onClick={() => void remove(r.endpoint)}
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}

      {selected && (
        <RouterPanel
          key={selected}
          endpoint={selected}
          reloadRouters={reload}
          onDone={onDone}
          onError={onError}
        />
      )}

      <Modal open={editing !== null} onClose={() => setEditing(null)} className="max-w-2xl">
        <div className="p-5 space-y-3">
          <h2 className="text-lg font-semibold">{editing === 'new' ? 'New router' : `Edit ${editing}`}</h2>
          <p className="text-xs text-[var(--zf-muted)]">
            JSON, same shape as <code>fabricctl ai router apply</code>. Candidates must be existing endpoints.
          </p>
          <textarea
            className="zf-input w-full font-mono text-xs"
            rows={18}
            aria-label="Router JSON"
            value={text}
            onChange={(e) => setText(e.target.value)}
            spellCheck={false}
          />
          {formError && (
            <p role="alert" className="text-xs text-[var(--zf-danger,#d33)]">
              {formError}
            </p>
          )}
          <div className="flex gap-2 justify-end">
            <button type="button" className="zf-btn zf-btn-sm zf-btn-ghost" onClick={() => setEditing(null)}>
              Cancel
            </button>
            <button type="button" className="zf-btn zf-btn-sm zf-btn-primary" onClick={() => void apply()}>
              Save
            </button>
          </div>
        </div>
      </Modal>
    </div>
  )
}

function RouterPanel({
  endpoint,
  reloadRouters,
  onDone,
  onError,
}: {
  endpoint: string
  reloadRouters: () => Promise<unknown>
  onDone: (m: string) => void
  onError: (label: string, e: unknown) => void
}) {
  const [report, setReport] = useState<RouterReport | null>(null)
  const [minSamples, setMinSamples] = useState('30')
  const [probe, setProbe] = useState('{"messages":[{"role":"user","content":"Hello"}]}')
  const [decision, setDecision] = useState<RouteDecision | null>(null)
  const [probeError, setProbeError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      const n = Number(minSamples)
      setReport(await routerReport(endpoint, Number.isInteger(n) && n >= 2 ? n : undefined))
    } catch (e) {
      onError('Could not load report', e)
    }
  }, [endpoint, minSamples, onError])

  useEffect(() => {
    void refresh()
    // refresh only when the router changes; min-samples applies on demand
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [endpoint])

  const promote = async (variant: string) => {
    if (!window.confirm(`Send all traffic on ${endpoint} to "${variant}"?`)) return
    try {
      await promoteVariant(endpoint, variant)
      onDone(`${endpoint} now routes everything to ${variant}`)
      await reloadRouters()
      await refresh()
    } catch (e) {
      onError('Promote failed', e)
    }
  }

  const tryRoute = async () => {
    let body: unknown
    try {
      body = JSON.parse(probe)
    } catch {
      setProbeError('Request is not valid JSON')
      return
    }
    setProbeError(null)
    try {
      setDecision(await decideRoute(endpoint, body))
    } catch (e) {
      onError('Decision failed', e)
    }
  }

  return (
    <Card className="p-4 space-y-4">
      <div className="flex items-end gap-3 flex-wrap">
        <h2 className="text-sm font-semibold mr-auto">A/B report: {endpoint}</h2>
        <label className="text-xs">
          <span className="block mb-1 text-[var(--zf-muted)]">Min scored samples per arm</span>
          <input
            className="zf-input w-24"
            inputMode="numeric"
            value={minSamples}
            onChange={(e) => setMinSamples(e.target.value)}
          />
        </label>
        <button type="button" className="zf-btn zf-btn-sm zf-btn-ghost" onClick={() => void refresh()}>
          Recalculate
        </button>
      </div>

      {report && (
        <>
          <p className="text-sm">
            <strong>Recommendation:</strong> {report.recommendation}
          </p>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className={headRow}>
                  <th className={th}>Arm</th>
                  <th className={th}>Requests</th>
                  <th className={th}>Errors</th>
                  <th className={th}>Mean score</th>
                  <th className={th}>vs control</th>
                  <th className={th}>p50 / p95</th>
                  <th className={th}>Est. cost</th>
                  <th className={th}>
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {report.variants.map((v) => (
                  <tr key={v.variant} className={row}>
                    <td className={`${th} font-medium`}>
                      {v.variant}
                      {v.variant === report.control ? ' (control)' : ''}
                    </td>
                    <td className={th}>{v.requests}</td>
                    <td className={th}>
                      {v.errors} ({formatPercent(v.error_rate)})
                    </td>
                    <td className={th}>
                      {formatScore(v.mean_score)}
                      <span className="text-xs text-[var(--zf-muted)]"> n={v.scored}</span>
                    </td>
                    <td className={`${th} text-xs`}>
                      {v.vs_control
                        ? `${v.vs_control.delta >= 0 ? '+' : ''}${v.vs_control.delta.toFixed(2)}, p${formatPValue(v.vs_control.p_value).startsWith('<') ? '' : '='}${formatPValue(v.vs_control.p_value)}${v.vs_control.significant ? ' ✓' : ''}`
                        : '—'}
                    </td>
                    <td className={`${th} text-xs`}>
                      {v.latency_p50_ms} / {v.latency_p95_ms} ms
                    </td>
                    <td className={th}>{formatCost(v.est_cost_usd)}</td>
                    <td className={th}>
                      <button
                        type="button"
                        className="zf-btn zf-btn-sm zf-btn-ghost"
                        onClick={() => void promote(v.variant)}
                      >
                        Promote
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p className="text-xs text-[var(--zf-muted)]">
            Welch z-test at 5% on scores (normal approximation, needs the minimum samples on both arms). Decide
            the sample size up front rather than stopping when p dips below 0.05. Promote is manual.
          </p>
        </>
      )}

      <div className="border-t border-[var(--zf-hairline)] pt-4 space-y-2">
        <h3 className="text-sm font-semibold">Try a request</h3>
        <textarea
          className="zf-input w-full font-mono text-xs"
          rows={3}
          aria-label="Request body to route"
          value={probe}
          onChange={(e) => setProbe(e.target.value)}
          spellCheck={false}
        />
        <div className="flex items-center gap-3">
          <button type="button" className="zf-btn zf-btn-sm zf-btn-ghost" onClick={() => void tryRoute()}>
            Which arm?
          </button>
          {probeError && (
            <span role="alert" className="text-xs text-[var(--zf-danger,#d33)]">
              {probeError}
            </span>
          )}
          {decision && (
            <span className="text-sm">
              <strong>{decision.chosen}</strong> — {decision.reason} (≈{decision.prompt_tokens_estimate} prompt
              tokens{decision.hard ? ', hard' : ''}; fallback order {decision.order.join(' → ')})
            </span>
          )}
        </div>
      </div>
    </Card>
  )
}
