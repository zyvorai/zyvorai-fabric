// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useCallback } from 'react'
import { Terminal, Send, Clock, Trash2, ChevronRight, Loader2, Copy, Check } from 'lucide-react'
import { apiFetch } from '../api/client'
import ErrorBanner from '../components/ErrorBanner'
import { PageHeader, Card } from '../components/ui'
import { formatUserError } from '../utils/apiError'
import { hintsForError } from '../utils/daemonHints'
import { AppleTerminalFrame, TerminalTextarea } from '../components/AppleTerminalFrame'
import { AnsiText } from '../components/AnsiText'

type HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE'

interface HistoryEntry {
  id: number
  method: HttpMethod
  url: string
  status: number
  timestamp: string
}

interface ApiResponse {
  status: number
  statusText: string
  headers: Record<string, string>
  body: string
  duration: number
}

const presetEndpoints: { method: HttpMethod; path: string; label: string }[] = [
  { method: 'GET', path: '/health', label: 'Health Check' },
  { method: 'GET', path: '/api/vms', label: 'List VMs' },
  { method: 'GET', path: '/api/system/cpu/topology', label: 'CPU Topology' },
  { method: 'GET', path: '/api/system/memory', label: 'System Memory' },
  { method: 'GET', path: '/api/system/numa/topology', label: 'NUMA Topology' },
  { method: 'GET', path: '/api/networkd/links', label: 'Network Links' },
  { method: 'GET', path: '/api/storage/pools', label: 'Storage Pools' },
  { method: 'GET', path: '/api/templates', label: 'Templates' },
  { method: 'GET', path: '/api/backups', label: 'Backups' },
  { method: 'GET', path: '/api/audit/logs', label: 'Audit Logs' },
  { method: 'GET', path: '/api/certificates', label: 'Certificates' },
  { method: 'GET', path: '/api/quotas', label: 'Quotas' },
  { method: 'GET', path: '/api/schedules', label: 'Schedules' },
  { method: 'GET', path: '/api/webhooks', label: 'Webhooks' },
  { method: 'GET', path: '/api/system/optimization/recommendations', label: 'Optimizations' },
]

const methodColors: Record<HttpMethod, string> = {
  GET: 'text-emerald-700 bg-emerald-50',
  POST: 'text-[var(--zf-link)] bg-[var(--zf-link)]/10',
  PUT: 'text-amber-800 bg-amber-50',
  DELETE: 'text-red-700 bg-red-50',
}

function statusColor(status: number): string {
  if (status < 300) return 'bg-emerald-50 text-emerald-700'
  if (status < 400) return 'bg-amber-50 text-amber-800'
  return 'bg-red-50 text-red-700'
}

let historyIdCounter = 0

export default function APIPlayground() {
  const [method, setMethod] = useState<HttpMethod>('GET')
  const [url, setUrl] = useState('/api/')
  const [body, setBody] = useState('')
  const [response, setResponse] = useState<ApiResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [history, setHistory] = useState<HistoryEntry[]>([])
  const [copied, setCopied] = useState(false)

  const handleSend = useCallback(async () => {
    if (!url.trim()) return
    setLoading(true)
    setError('')
    setResponse(null)

    const start = performance.now()
    try {
      const opts: RequestInit = { method }
      if ((method === 'POST' || method === 'PUT') && body.trim()) {
        opts.headers = { 'Content-Type': 'application/json' }
        opts.body = body
      }

      const res = await apiFetch(url, opts)
      const duration = Math.round(performance.now() - start)
      const text = await res.text()

      const headers: Record<string, string> = {}
      res.headers.forEach((v, k) => { headers[k] = v })

      let formatted = text
      try {
        formatted = JSON.stringify(JSON.parse(text), null, 2)
      } catch {
        // Not JSON
      }

      setResponse({ status: res.status, statusText: res.statusText, headers, body: formatted, duration })

      setHistory((prev) => [
        { id: ++historyIdCounter, method, url, status: res.status, timestamp: new Date().toLocaleTimeString() },
        ...prev.slice(0, 19),
      ])
    } catch (err) {
      setError(formatUserError(err))
    } finally {
      setLoading(false)
    }
  }, [method, url, body])

  const handlePreset = (preset: typeof presetEndpoints[0]) => {
    setMethod(preset.method)
    setUrl(preset.path)
    if (preset.method === 'GET') setBody('')
  }

  const handleCopy = () => {
    if (response) {
      navigator.clipboard.writeText(response.body)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    }
  }

  return (
    <div className="max-w-6xl mx-auto space-y-6">
      <PageHeader title="API Playground" description="Send ad-hoc requests to the Zyvor Fabric API" />

      <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
        <div className="lg:col-span-1 space-y-4">
          <Card><div className="p-4">
            <h3 className="text-xs font-semibold text-[var(--zf-muted)] uppercase tracking-wider mb-3">Endpoints</h3>
            <div className="space-y-1 max-h-[300px] overflow-y-auto">
              {presetEndpoints.map((ep, idx) => (
                <button
                  key={idx}
                  onClick={() => handlePreset(ep)}
                  className="flex items-center gap-2 w-full px-2 py-1.5 text-xs text-[var(--zf-ink)] hover:bg-black/[0.04] rounded-lg transition-colors text-left"
                >
                  <span className={`font-mono font-bold px-1.5 py-0.5 rounded text-xs ${methodColors[ep.method]}`}>
                    {ep.method}
                  </span>
                  <span className="truncate">{ep.label}</span>
                </button>
              ))}
            </div>
          </div></Card>

          {history.length > 0 && (
            <Card><div className="p-4">
              <div className="flex items-center justify-between mb-3">
                <h3 className="text-xs font-semibold text-[var(--zf-muted)] uppercase tracking-wider">History</h3>
                <button
                  onClick={() => setHistory([])}
                  className="p-1 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition-colors"
                  title="Clear history"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              </div>
              <div className="space-y-1 max-h-[200px] overflow-y-auto">
                {history.map((h) => (
                  <button
                    key={h.id}
                    onClick={() => { setMethod(h.method); setUrl(h.url) }}
                    className="flex items-center gap-2 w-full px-2 py-1.5 text-xs text-[var(--zf-muted)] hover:bg-black/[0.04] rounded-lg transition-colors text-left"
                  >
                    <span className={`font-mono font-bold px-1 py-0.5 rounded text-xs ${methodColors[h.method]}`}>
                      {h.method}
                    </span>
                    <span className="truncate flex-1 font-mono">{h.url}</span>
                    <span className={`px-1.5 py-0.5 rounded text-xs ${statusColor(h.status)}`}>{h.status}</span>
                  </button>
                ))}
              </div>
            </div></Card>
          )}
        </div>

        <div className="lg:col-span-3 space-y-4">
          <div className="flex items-center gap-2">
            <select
              value={method}
              onChange={(e) => setMethod(e.target.value as HttpMethod)}
              aria-label="HTTP method"
              className={`px-3 py-2.5 text-sm font-bold font-mono rounded-lg border border-[var(--zf-hairline)] bg-[var(--zf-surface)] focus:outline-none ${methodColors[method]}`}
            >
              {(['GET', 'POST', 'PUT', 'DELETE'] as HttpMethod[]).map((m) => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
            <input
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') handleSend() }}
              aria-label="Request URL"
              className="input-field flex-1 font-mono"
              placeholder="/api/..."
            />
            <button
              onClick={handleSend}
              disabled={loading || !url.trim()}
              className="zf-btn zf-btn-primary"
            >
              {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : <Send className="w-4 h-4" />}
              Send
            </button>
          </div>

          {(method === 'POST' || method === 'PUT') && (
            <div>
              <label className="block text-xs font-medium text-[var(--zf-muted)] mb-1.5">Request Body (JSON)</label>
              <TerminalTextarea
                title="request — JSON body"
                value={body}
                onChange={(e) => setBody(e.target.value)}
                rows={6}
                placeholder='{"key": "value"}'
              />
            </div>
          )}

          {error && (
            <ErrorBanner
              title="Request failed"
              headline={error}
              hints={hintsForError(error)}
              onRetry={handleSend}
            />
          )}

          {response && (
            <Card className="overflow-hidden">
              <div className="flex items-center justify-between px-4 py-3 border-b border-[var(--zf-hairline)]">
                <div className="flex items-center gap-3">
                  <span className={`px-2 py-1 rounded text-xs font-bold ${statusColor(response.status)}`}>
                    {response.status} {response.statusText}
                  </span>
                  <span className="flex items-center gap-1 text-xs text-[var(--zf-muted)]">
                    <Clock className="w-3.5 h-3.5" />
                    {response.duration}ms
                  </span>
                </div>
                <button
                  onClick={handleCopy}
                  className="zf-btn zf-btn-ghost zf-btn-sm"
                >
                  {copied ? <Check className="w-3.5 h-3.5 text-emerald-700" /> : <Copy className="w-3.5 h-3.5" />}
                  {copied ? 'Copied' : 'Copy'}
                </button>
              </div>

              <details className="border-b border-[var(--zf-hairline)]">
                <summary className="px-4 py-2 text-xs font-medium text-[var(--zf-muted)] cursor-pointer hover:text-[var(--zf-ink)] flex items-center gap-1">
                  <ChevronRight className="w-3.5 h-3.5" />
                  Response Headers ({Object.keys(response.headers).length})
                </summary>
                <div className="px-4 pb-3">
                  <AppleTerminalFrame
                    title="response — headers"
                    bodyClassName="max-h-40 overflow-y-auto px-3 py-2"
                  >
                    {Object.entries(response.headers).map(([k, v]) => (
                      <div key={k}>
                        <span className="text-[#64d2ff]">{k}</span>
                        <span className="text-white/40">: </span>
                        <span>{v}</span>
                      </div>
                    ))}
                  </AppleTerminalFrame>
                </div>
              </details>

              <div className="p-4">
                <AppleTerminalFrame
                  title={`${response.status} ${response.statusText} — ${response.duration}ms`}
                  bodyClassName="max-h-[400px] overflow-y-auto px-3 py-2 whitespace-pre-wrap break-words"
                >
                  <AnsiText text={response.body || '(empty body)'} />
                </AppleTerminalFrame>
              </div>
            </Card>
          )}

          {!response && !error && !loading && (
            <div className="text-center py-16 text-[var(--zf-muted)]">
              <Terminal className="w-10 h-10 mx-auto mb-3 opacity-50" />
              <p className="text-sm">Select an endpoint or enter a URL and click Send.</p>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
