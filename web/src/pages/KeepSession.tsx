// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useParams } from 'react-router'
import { Shield } from 'lucide-react'
import {
  BrowserScreenshot,
  BrowserView,
  KeepCockpit,
  decideApproval,
  getSession,
  getSessionBrowserScreenshot,
  getSessionBrowserView,
  getSessionCockpit,
  SessionView,
} from '../api/agents'
import { getToken } from '../api/client'
import { PageHeader, Card, EmptyState } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { useToastContext } from '../contexts/ToastContext'
import { toastFailure } from '../utils/toastError'
import { useKeepText } from '../i18n/useKeepText'
import { changeCount, summarizeChangeset } from '../lib/changeset'

const ALLOWED_BROWSER_SHOT_MIMES = new Set(['image/jpeg', 'image/png', 'image/webp'])

function browserShotMime(mime: string | undefined): string {
  if (mime && ALLOWED_BROWSER_SHOT_MIMES.has(mime)) return mime
  return 'image/jpeg'
}

function browserShotDataUrl(mime: string | undefined, imageBase64: string): string {
  return `data:${browserShotMime(mime)};base64,${imageBase64}`
}

function ChangesetReview({ planned }: { planned: unknown }) {
  const cs = summarizeChangeset(planned)
  if (!cs) return null
  const row = (sign: string, color: string, paths: string[]) =>
    paths.map((p) => (
      <li key={sign + p} className={color}>
        {sign} {p}
      </li>
    ))
  return (
    <div className="mt-2 space-y-1 text-xs" data-testid="changeset-review">
      <div className="font-mono text-[var(--zf-muted)]">
        $ {cs.command || '(command not shown)'}
        {cs.exitCode !== null ? ` · exit ${cs.exitCode}` : ''}
      </div>
      {changeCount(cs) === 0 ? (
        <p className="text-[var(--zf-muted)]">No file changes.</p>
      ) : (
        <ul className="font-mono">
          {row('+', 'text-green-600', cs.added)}
          {row('~', 'text-amber-600', cs.modified)}
          {row('−', 'text-red-600', cs.deleted)}
        </ul>
      )}
      {cs.unstaged.length > 0 && (
        <p className="text-amber-600">Not captured, so not applied: {cs.unstaged.join(', ')}</p>
      )}
      {cs.nonReplayable.length > 0 && (
        <p className="text-amber-600">Cannot be replayed: {cs.nonReplayable.join('; ')}</p>
      )}
      <p className="text-[var(--zf-muted)]">
        Nothing reaches the sandbox until you approve.{cs.egress ? ` Network: ${cs.egress}.` : ''}
      </p>
    </div>
  )
}

/** One Keep view: goal → current task → evidence → approval → outcome. */
export default function KeepSession() {
  const { sessionId } = useParams<{ sessionId: string }>()
  const toast = useToastContext()
  const { t, toggle } = useKeepText()
  const [session, setSession] = useState<SessionView | null>(null)
  const [cockpit, setCockpit] = useState<KeepCockpit | null>(null)
  const [browser, setBrowser] = useState<BrowserView | null>(null)
  const [shot, setShot] = useState<BrowserScreenshot | null>(null)
  const [casting, setCasting] = useState(false)
  const castWs = useRef<WebSocket | null>(null)
  const { loading, loadError, run } = usePageLoader('Failed to load Keep session')

  const load = useCallback(() => {
    if (!sessionId) return Promise.resolve()
    return run(async () => {
      const [s, c] = await Promise.all([
        getSession(sessionId),
        getSessionCockpit(sessionId),
      ])
      setSession(s)
      setCockpit(c)
      try {
        const b = await getSessionBrowserView(sessionId)
        setBrowser(b)
      } catch {
        setBrowser(null)
      }
      try {
        const frame = await getSessionBrowserScreenshot(sessionId)
        setShot(
          frame
            ? {
                ...frame,
                mime: browserShotMime(frame.mime),
              }
            : null,
        )
      } catch {
        setShot(null)
      }
    })
  }, [run, sessionId])

  useEffect(() => {
    void load()
  }, [load])

  useEffect(() => {
    if (!sessionId || !session) return
    const terminal = ['completed', 'failed', 'cancelled', 'expired']
    if (terminal.includes(session.status) && !(cockpit?.pending_approvals?.length)) return
    const timer = setInterval(() => void load(), 2500)
    return () => clearInterval(timer)
  }, [sessionId, session, cockpit?.pending_approvals?.length, load])

  const decide = async (approvalId: string, decision: 'approved' | 'denied') => {
    try {
      await decideApproval(approvalId, { decision, comment: `console ${decision}` })
      toast.success(decision === 'approved' ? t('session.approved') : t('session.denied'))
      void load()
    } catch (e) {
      toastFailure(toast, 'Approval decision failed', e)
    }
  }

  const stopCast = useCallback(() => {
    castWs.current?.close()
    castWs.current = null
    setCasting(false)
  }, [])

  useEffect(() => () => stopCast(), [stopCast])

  const toggleCast = () => {
    if (!sessionId) return
    if (casting) {
      stopCast()
      return
    }
    const token = getToken()
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const u = new URL(`${proto}//${window.location.host}/ws/sessions/${sessionId}/browser/screencast`)
    if (token) u.searchParams.set('token', token)
    const ws = new WebSocket(u.toString())
    castWs.current = ws
    setCasting(true)
    ws.onmessage = (ev) => {
      try {
        const m = JSON.parse(String(ev.data)) as {
          type?: string
          mime?: string
          image_base64?: string
          honesty?: string
        }
        if (m.type === 'frame' && m.image_base64) {
          setShot({
            session_id: sessionId,
            mime: browserShotMime(m.mime),
            image_base64: m.image_base64,
            title: 'screencast',
            url: '',
            honesty: m.honesty,
          })
        }
      } catch {
        /* ignore non-JSON */
      }
    }
    ws.onerror = () => {
      toastFailure(toast, 'Screencast failed', new Error('WebSocket error'))
      stopCast()
    }
    ws.onclose = () => {
      castWs.current = null
      setCasting(false)
    }
  }

  if (!sessionId) {
    return (
      <EmptyState
        icon={<Shield className="w-8 h-8" />}
        title={t('session.missing')}
        description={t('session.missingHint')}
      />
    )
  }

  const goal = cockpit?.active_goal
  const currentStep = goal?.plan?.find((s) => s.status === 'running' || s.status === 'blocked')
    ?? goal?.plan?.find((s) => s.status === 'pending')
  const pending = cockpit?.pending_approvals ?? []
  const lastOutcome = (cockpit?.last_decisions ?? []).slice(-3).reverse()

  return (
    <div>
      <PageHeader
        title={t('keep.title')}
        description={
          session
            ? `${session.agent} · ${session.status} · evidence ${cockpit?.evidence_class ?? 'software-test'}`
            : 'Goal, evidence, approval, outcome'
        }
        onRefresh={() => void load()}
        refreshing={loading}
        actions={
          <>
            <Link to={`/app/sessions/${sessionId}`} className="zf-btn zf-btn-ghost">
              {t('session.detail')}
            </Link>
            <button type="button" className="zf-btn zf-btn-ghost" onClick={toggle}>
              {t('keep.language')}
            </button>
          </>
        }
      />
      <PageLoadBanner title={t('session.loadFailed')} headline={loadError} onRetry={() => void load()} />

      <div className="space-y-4 max-w-3xl">
        <Card className="p-4 space-y-2 text-sm">
          <h2 className="text-base font-semibold text-[var(--zf-ink)]">Goal</h2>
          {goal ? (
            <>
              <div className="font-medium">{goal.title}</div>
              <div className="text-[var(--zf-muted)]">Status: {goal.status}</div>
            </>
          ) : (
            <p className="text-[var(--zf-muted)]">No active goal linked to this session.</p>
          )}
        </Card>

        <Card className="p-4 space-y-2 text-sm">
          <h2 className="text-base font-semibold text-[var(--zf-ink)]">Current task</h2>
          {currentStep ? (
            <>
              <div className="font-medium">{currentStep.title}</div>
              <div className="text-[var(--zf-muted)]">
                {currentStep.id} · {currentStep.status}
                {currentStep.requires_approval ? ' · requires approval' : ''}
              </div>
            </>
          ) : (
            <p className="text-[var(--zf-muted)]">No plan step in progress.</p>
          )}
        </Card>

        <Card className="p-4 space-y-2 text-sm">
          <h2 className="text-base font-semibold text-[var(--zf-ink)]">Attestation receipt</h2>
          {(() => {
            const a = cockpit?.attestation
            const evidence = a?.evidence_class ?? cockpit?.evidence_class ?? 'software-test'
            const canRead = a?.operator_can_read ?? evidence === 'software-test'
            return (
              <>
                <div className="font-medium">
                  {evidence}
                  {canRead ? ' · host can read' : ' · launch verified'}
                </div>
                <div className="text-[var(--zf-muted)] text-xs space-y-1">
                  <div>profile: {a?.security_profile ?? cockpit?.security_profile ?? '—'}</div>
                  <div>image: {a?.image_hash ?? '—'}</div>
                  <div>
                    snp_verified={String(a?.snp_launch_verified ?? false)} · tdx_verified=
                    {String(a?.tdx_launch_verified ?? false)}
                  </div>
                  <div>
                    host recover:{' '}
                    {a?.host_recover_allowed
                      ? 'allowed (dual-key break-glass on measured/standard)'
                      : 'forbidden (confidential)'}
                  </div>
                </div>
                <p className="text-[12px] text-[var(--zf-muted)] pt-2 border-t border-[var(--zf-hairline)]">
                  {a?.honesty ?? cockpit?.honesty ??
                    'Evidence class software-test: the host can still see this VM.'}
                </p>
              </>
            )
          })()}
        </Card>

        <Card className="p-4 space-y-2 text-sm">
          <h2 className="text-base font-semibold text-[var(--zf-ink)]">Egress proof</h2>
          <div className="font-medium">
            CONNECT: {cockpit?.egress_connects ?? '—'}
            {cockpit?.agent_paused_reason ? ` · paused=${cockpit.agent_paused_reason}` : ''}
          </div>
          <p className="text-[12px] text-[var(--zf-muted)]">
            Count from Keep audit (`egress.connect` / `ebpf.*`). FluxVM `drop_reasons` when dataplane
            is attached — no PacketWolf required.
          </p>
        </Card>

        <Card className="p-4 space-y-2 text-sm">
          <h2 className="text-base font-semibold text-[var(--zf-ink)]">Honesty badge</h2>
          {(() => {
            const badge = cockpit?.badge ?? cockpit?.browser?.badge
            if (!badge) {
              return <p className="text-[var(--zf-muted)]">No badge yet.</p>
            }
            return (
              <code className="block text-xs font-mono text-[var(--zf-ink)] break-all">
                evidence={badge.evidence ?? 'software-test'} · operator_can_read=
                {String(badge.operator_can_read ?? true)} · host_recover=
                {badge.host_recover ?? 'allowed'} · browser={badge.browser ?? 'a11y-only'} ·
                proxy={badge.proxy ?? 'strict'}
                {cockpit?.agent_paused_reason
                  ? ` · paused=${cockpit.agent_paused_reason}`
                  : ''}
              </code>
            )
          })()}
        </Card>

        <Card className="p-4 space-y-2 text-sm">
          <h2 className="text-base font-semibold text-[var(--zf-ink)]">Browser capability</h2>
          {(() => {
            const b = cockpit?.browser
            if (!b) {
              return (
                <p className="text-[var(--zf-muted)]">
                  No browser capability (agent needs browser_port + confinement:strict).
                </p>
              )
            }
            return (
              <>
                <div className="font-medium">
                  Browser: {b.ready ? 'ready' : 'not ready'} · CDP {b.cdp ? 'up' : 'off'} · proxy{' '}
                  {b.confined ? 'confined' : 'not confined'}
                  {b.agent_paused_reason ? ` · paused=${b.agent_paused_reason}` : ''}
                </div>
                <div className="text-[var(--zf-muted)] text-xs">
                  taint={(b.tainted_by?.length ? b.tainted_by.join(', ') : '—')} · evidence=
                  {b.evidence_class ?? 'software-test'} · tools=
                  {b.tools_allowed ? 'allowed' : 'blocked'}
                  {b.network_identity ? ` · id=${b.network_identity}` : ''}
                </div>
                <p className="text-[12px] text-[var(--zf-muted)] pt-2 border-t border-[var(--zf-hairline)]">
                  {typeof b.honesty === 'string' ? b.honesty : b.badge?.honesty}
                </p>
              </>
            )
          })()}
        </Card>

        <Card className="p-4 space-y-2 text-sm">
          <h2 className="text-base font-semibold text-[var(--zf-ink)]">Evidence</h2>
          {(cockpit?.recent_artifacts?.length ?? 0) === 0 ? (
            <p className="text-[var(--zf-muted)]">No artifacts yet.</p>
          ) : (
            <ul className="space-y-2">
              {cockpit!.recent_artifacts!.map((a) => (
                <li key={a.id} className="flex justify-between gap-2">
                  <span>
                    <span className="font-medium">{a.title}</span>
                    <span className="text-[var(--zf-muted)]"> · {a.kind}</span>
                  </span>
                  <code className="font-mono text-xs text-[var(--zf-muted)]">{a.id.slice(0, 8)}</code>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card className="p-4 space-y-2 text-sm">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-base font-semibold text-[var(--zf-ink)]">Browser</h2>
            <button
              type="button"
              className="text-xs text-[var(--zf-muted)] underline"
              onClick={toggleCast}
            >
              {casting ? t('session.stopCast') : t('session.startCast')}
            </button>
          </div>
          {!browser ? (
            <p className="text-[var(--zf-muted)]">
              Tab listing unavailable (session needs a running browser_port agent).
              Screenshot needs the same; input takeover is not implemented.
            </p>
          ) : (browser.tabs?.length ?? 0) === 0 ? (
            <p className="text-[var(--zf-muted)]">No open tabs reported.</p>
          ) : (
            <ul className="space-y-2">
              {browser.tabs.map((t) => (
                <li key={t.id} className="truncate">
                  <span className="font-medium">{t.title || '(untitled)'}</span>
                  <span className="text-[var(--zf-muted)]"> · {t.url}</span>
                </li>
              ))}
            </ul>
          )}
          {shot?.image_base64 && (
            <div className="pt-2 space-y-1">
              <img
                alt={shot.title || 'browser screenshot'}
                className="w-full max-h-80 object-contain rounded border border-[var(--zf-hairline)] bg-black"
                src={browserShotDataUrl(shot.mime, shot.image_base64)}
              />
              <p className="text-[12px] text-[var(--zf-muted)] truncate">
                {shot.title} · {shot.url}
              </p>
            </div>
          )}
          {(shot?.honesty || browser?.honesty || cockpit?.vault?.honesty) && (
            <p className="text-[12px] text-[var(--zf-muted)] pt-2 border-t border-[var(--zf-hairline)]">
              {shot?.honesty ?? browser?.honesty ?? cockpit?.vault?.honesty}
            </p>
          )}
        </Card>

        <Card className="p-4 space-y-3 text-sm">
          <h2 className="text-base font-semibold text-[var(--zf-ink)]">Approval</h2>
          {pending.length === 0 ? (
            <p className="text-[var(--zf-muted)]">No pending approvals.</p>
          ) : (
            pending.map((a) => (
              <div key={a.id} className="border-t border-[var(--zf-hairline)] pt-3 first:border-0 first:pt-0">
                <div className="font-medium">{a.prompt}</div>
                <div className="text-[var(--zf-muted)] text-xs mt-1">
                  {a.kind ?? 'custom'} {a.subject ? `· ${a.subject}` : ''}
                </div>
                <ChangesetReview planned={a.planned_action} />
                <div className="flex gap-2 mt-2">
                  <button
                    type="button"
                    className="zf-btn zf-btn-primary zf-btn-sm"
                    onClick={() => void decide(a.id, 'approved')}
                  >
                    {t('session.approve')}
                  </button>
                  <button
                    type="button"
                    className="zf-btn zf-btn-danger zf-btn-sm"
                    onClick={() => void decide(a.id, 'denied')}
                  >
                    {t('session.deny')}
                  </button>
                </div>
              </div>
            ))
          )}
        </Card>

        <Card className="p-4 space-y-2 text-sm">
          <h2 className="text-base font-semibold text-[var(--zf-ink)]">Outcome</h2>
          {lastOutcome.length === 0 ? (
            <p className="text-[var(--zf-muted)]">No audit decisions yet.</p>
          ) : (
            <ul className="space-y-1 font-mono text-xs">
              {lastOutcome.map((d, i) => (
                <li key={i}>
                  {d.phase ?? '—'} · {d.action ?? '—'}
                </li>
              ))}
            </ul>
          )}
          {cockpit?.honesty && (
            <p className="text-[12px] text-[var(--zf-muted)] pt-2 border-t border-[var(--zf-hairline)]">
              {cockpit.honesty}
            </p>
          )}
        </Card>
      </div>
    </div>
  )
}
