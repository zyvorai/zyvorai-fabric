// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router'
import { Activity, Trash2 } from 'lucide-react'
import {
  SessionView,
  deleteSession,
  getSession,
  listSessions,
  sessionAction,
} from '../api/agents'
import { PageHeader, EmptyState, Card } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { useToastContext } from '../contexts/ToastContext'
import { toastFailure } from '../utils/toastError'

export default function Sessions() {
  const toast = useToastContext()
  const navigate = useNavigate()
  const { id } = useParams<{ id?: string }>()
  const [sessions, setSessions] = useState<SessionView[]>([])
  const [detail, setDetail] = useState<SessionView | null>(null)
  const { loading, loadError, run } = usePageLoader('Failed to load sessions')

  const load = useCallback(() => {
    return run(async () => {
      if (id) {
        setDetail(await getSession(id))
        setSessions([])
      } else {
        const res = await listSessions()
        setSessions(res.items ?? [])
        setDetail(null)
      }
    })
  }, [run, id])

  useEffect(() => {
    void load()
  }, [load])

  // Session creation returns as soon as the sandbox is admitted; provisioning
  // (guest boot, health checks, bundle push) continues in the background and
  // only shows up here as a later status change, so poll while it's in
  // flight instead of leaving the page showing a stale "creating" forever.
  useEffect(() => {
    if (!id || !detail) return
    const terminal = ['completed', 'failed', 'cancelled', 'expired']
    if (terminal.includes(detail.status)) return
    const timer = setInterval(() => void load(), 2000)
    return () => clearInterval(timer)
  }, [id, detail, load])

  const act = async (sessionId: string, action: 'cancel' | 'hibernate' | 'resume') => {
    try {
      await sessionAction(sessionId, action)
      toast.success(`${action} accepted`)
      void load()
    } catch (e) {
      toastFailure(toast, `Failed to ${action}`, e)
    }
  }

  const remove = async (sessionId: string) => {
    try {
      await deleteSession(sessionId)
      toast.success('Session deleted')
      if (id) navigate('/app/sessions')
      else void load()
    } catch (e) {
      toastFailure(toast, 'Failed to delete session', e)
    }
  }

  if (id && detail) {
    return (
      <div>
        <PageHeader
          title={`Session ${detail.id}`}
          description={`${detail.agent} · ${detail.status}`}
          onRefresh={() => void load()}
          refreshing={loading}
          actions={
            <Link to="/app/sessions" className="zf-btn zf-btn-ghost">
              All sessions
            </Link>
          }
        />
        <PageLoadBanner title="Could not load session" headline={loadError} onRetry={() => void load()} />
        <Card className="p-4 space-y-3 text-sm">
          <div>Status: <strong>{detail.status}</strong></div>
          <div>Sandbox: <code className="font-mono text-xs">{detail.sandbox_id}</code></div>
          <div>Updated: {detail.updated_at}</div>
          {detail.error && <div className="text-red-700">Error: {detail.error}</div>}
          <div className="flex flex-wrap gap-2 pt-2">
            <Link to={`/app/merion/${detail.id}`} className="zf-btn zf-btn-primary zf-btn-sm">
              Merion view
            </Link>
            <button type="button" className="zf-btn zf-btn-ghost zf-btn-sm" onClick={() => void act(detail.id, 'hibernate')}>
              Hibernate
            </button>
            <button type="button" className="zf-btn zf-btn-ghost zf-btn-sm" onClick={() => void act(detail.id, 'resume')}>
              Resume
            </button>
            <button type="button" className="zf-btn zf-btn-ghost zf-btn-sm" onClick={() => void act(detail.id, 'cancel')}>
              Cancel
            </button>
            <button type="button" className="zf-btn zf-btn-danger zf-btn-sm" onClick={() => void remove(detail.id)}>
              <Trash2 className="w-3.5 h-3.5" /> Delete
            </button>
          </div>
        </Card>
      </div>
    )
  }

  return (
    <div>
      <PageHeader
        title="Sessions"
        description="Agent-runtime sessions backed by FluxVM sandboxes"
        onRefresh={() => void load()}
        refreshing={loading}
        actions={
          <Link to="/app/agents" className="zf-btn zf-btn-ghost">
            Agents
          </Link>
        }
      />
      <PageLoadBanner title="Could not load sessions" headline={loadError} onRetry={() => void load()} />
      {sessions.length === 0 && !loadError ? (
        <EmptyState
          icon={<Activity className="w-8 h-8" />}
          title="No sessions"
          description="Start a session from the Agents page."
        />
      ) : (
        <Card className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-[var(--zf-muted)] border-b border-[var(--zf-hairline)]">
                <th className="px-4 py-3 font-medium">ID</th>
                <th className="px-4 py-3 font-medium">Agent</th>
                <th className="px-4 py-3 font-medium">Status</th>
                <th className="px-4 py-3 font-medium">Updated</th>
                <th className="px-4 py-3 font-medium"></th>
              </tr>
            </thead>
            <tbody>
              {sessions.map((s) => (
                <tr key={s.id} className="border-b border-[var(--zf-hairline)] last:border-0">
                  <td className="px-4 py-3">
                    <Link className="text-[var(--zf-link)] hover:underline font-mono text-xs" to={`/app/sessions/${s.id}`}>
                      {s.id}
                    </Link>
                  </td>
                  <td className="px-4 py-3">{s.agent}</td>
                  <td className="px-4 py-3">{s.status}</td>
                  <td className="px-4 py-3 text-[var(--zf-muted)]">{s.updated_at}</td>
                  <td className="px-4 py-3 text-right">
                    <button type="button" className="zf-btn zf-btn-danger zf-btn-sm" onClick={() => void remove(s.id)} aria-label={`Delete ${s.id}`} title={`Delete ${s.id}`}>
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
