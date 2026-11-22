// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useEffect, useState, Fragment } from 'react'
import { Plus, Trash2, Save, RotateCcw, Boxes, FileText } from 'lucide-react'
import {
  listContainerGroups,
  deleteContainerGroup,
  applyContainerGroup,
  listContainerGroupEvents,
  listContainerGroupBackups,
  createContainerGroupBackup,
  deleteContainerGroupBackup,
  restoreContainerGroupBackup,
  getContainerGroupStatus,
  ContainerGroupSpec,
  ContainerGroupEvent,
  ContainerGroupBackup,
  ContainerGroupLiveStatus,
} from '../api/containerGroups'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import RelativeTime from '../components/RelativeTime'
import { PageHeader, EmptyState, Card, CardBody, Modal } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { toastFailure } from '../utils/toastError'

const EXAMPLE_SPEC = `{
  "name": "web",
  "replicas": 2,
  "containers": [
    {
      "name": "app",
      "image": "nginx:latest",
      "resources": { "cpus": 1, "memory": "512M" }
    }
  ]
}`

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return (bytes / Math.pow(1024, i)).toFixed(1) + ' ' + units[i]
}

function eventTypeStyle(t: ContainerGroupEvent['event_type']): string {
  if (t === 'created' || t === 'applied') return 'text-emerald-700 bg-emerald-50 border-emerald-200'
  if (t === 'deleted') return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
  return 'text-red-700 bg-red-50 border-red-200' // placement_failed, quota_exceeded
}

function ApplySpecDialog({ onClose, onSuccess }: { onClose: () => void; onSuccess: () => void }) {
  const toast = useToastContext()
  const [text, setText] = useState(EXAMPLE_SPEC)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleSubmit = async () => {
    let spec: ContainerGroupSpec
    try {
      spec = JSON.parse(text)
    } catch {
      setError('Not valid JSON')
      return
    }
    if (!spec.name || !Array.isArray(spec.containers) || spec.containers.length === 0) {
      setError('Spec needs at least "name" and a non-empty "containers" array')
      return
    }

    setSubmitting(true)
    setError(null)
    try {
      const result = await applyContainerGroup(spec)
      toast.success(`Applied '${result.name}' (${result.replicas_created} replica(s) created)`)
      if (result.warnings.length > 0) {
        toast.error(`Applied with warnings: ${result.warnings.join('; ')}`)
      }
      onSuccess()
      onClose()
    } catch (err) {
      toastFailure(toast, 'Failed to apply ContainerGroup', err)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Modal open onClose={onClose} className="max-w-2xl">
      <div className="p-6">
        <h2 className="text-xl font-semibold text-[var(--zf-ink)] mb-1">Apply ContainerGroup Spec</h2>
        <p className="text-sm text-[var(--zf-muted)] mb-4">
          Paste or edit a JSON spec, the same shape <code>fabricctl container-group apply -f</code> accepts.
        </p>
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          rows={16}
          spellCheck={false}
          className="w-full font-mono text-sm p-3 rounded border border-[var(--zf-hairline)] bg-[var(--zf-canvas)] text-[var(--zf-ink)]"
        />
        {error && <p className="text-sm text-[var(--zf-danger)] mt-2">{error}</p>}
        <div className="flex justify-end gap-2 mt-4">
          <button onClick={onClose} className="zf-btn zf-btn-ghost" disabled={submitting}>
            Cancel
          </button>
          <button onClick={() => void handleSubmit()} className="zf-btn zf-btn-primary" disabled={submitting}>
            {submitting ? 'Applying…' : 'Apply'}
          </button>
        </div>
      </div>
    </Modal>
  )
}

export default function ContainerGroups() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [groups, setGroups] = useState<ContainerGroupSpec[]>([])
  const [events, setEvents] = useState<ContainerGroupEvent[]>([])
  const [backups, setBackups] = useState<ContainerGroupBackup[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load ContainerGroups')
  const [showApplyDialog, setShowApplyDialog] = useState(false)
  const [statusByName, setStatusByName] = useState<Record<string, ContainerGroupLiveStatus>>({})
  const [expanded, setExpanded] = useState<string | null>(null)

  const loadData = useCallback(() => {
    return run(async () => {
      const [groupsData, eventsData, backupsData] = await Promise.all([
        listContainerGroups(),
        listContainerGroupEvents(),
        listContainerGroupBackups(),
      ])
      setGroups(groupsData)
      setEvents(eventsData)
      setBackups(backupsData)
      const statuses = await Promise.all(
        groupsData.map(async (g) => {
          try {
            return [g.name, await getContainerGroupStatus(g.name)] as const
          } catch {
            return null
          }
        }),
      )
      const next: Record<string, ContainerGroupLiveStatus> = {}
      for (const row of statuses) {
        if (row) next[row[0]] = row[1]
      }
      setStatusByName(next)
    })
  }, [run])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleDelete = async (name: string) => {
    const ok = await confirm('Delete ContainerGroup', `Delete '${name}'? This stops and removes its Pod(s).`, {
      variant: 'danger',
      confirmLabel: 'Delete',
    })
    if (!ok) return
    try {
      await deleteContainerGroup(name)
      toast.success(`Deleted '${name}'`)
      loadData()
    } catch (error) {
      toastFailure(toast, 'Failed to delete ContainerGroup', error)
    }
  }

  const handleBackup = async (name: string) => {
    try {
      await createContainerGroupBackup(name)
      toast.success(`Backup created for '${name}'`)
      loadData()
    } catch (error) {
      toastFailure(toast, 'Failed to create backup', error)
    }
  }

  const handleRestoreBackup = async (backup: ContainerGroupBackup) => {
    const ok = await confirm(
      'Restore Backup',
      `Restore this backup onto '${backup.container_group_name}''s original volume paths? Files there will be overwritten.`,
      { variant: 'warning', confirmLabel: 'Restore' },
    )
    if (!ok) return
    try {
      const result = await restoreContainerGroupBackup(backup.id)
      if (result.warnings.length > 0) {
        toast.error(`Restored with warnings: ${result.warnings.join('; ')}`)
      } else {
        toast.success(`Restored ${result.restored_paths.length} volume(s)`)
      }
    } catch (error) {
      toastFailure(toast, 'Failed to restore backup', error)
    }
  }

  const handleDeleteBackup = async (backup: ContainerGroupBackup) => {
    const ok = await confirm('Delete Backup', 'Delete this backup and its archive file?', {
      variant: 'danger',
      confirmLabel: 'Delete',
    })
    if (!ok) return
    try {
      await deleteContainerGroupBackup(backup.id)
      toast.success('Backup deleted')
      loadData()
    } catch (error) {
      toastFailure(toast, 'Failed to delete backup', error)
    }
  }

  return (
    <div>
      <PageHeader
        onRefresh={() => void loadData()}
        refreshing={loading}
        title="Container Groups"
        description="ContainerGroup workloads scheduled by fabric onto FluxVM Secure Containers"
        actions={
          <button onClick={() => setShowApplyDialog(true)} className="zf-btn zf-btn-primary">
            <Plus className="w-4 h-4" />
            Apply Spec
          </button>
        }
      />

      <PageLoadBanner title="Could not load ContainerGroups" headline={loadError} onRetry={() => void loadData()} />

      {groups.length === 0 ? (
        <EmptyState
          icon={<Boxes className="w-8 h-8" />}
          title="No ContainerGroups yet"
          description="Apply a spec to schedule your first ContainerGroup onto a Secure-Containers-capable host."
          action={
            <button onClick={() => setShowApplyDialog(true)} className="zf-btn zf-btn-primary">
              <Plus className="w-4 h-4" />
              Apply Spec
            </button>
          }
        />
      ) : (
        <Card className="overflow-x-auto mb-8">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-[var(--zf-muted)] border-b border-[var(--zf-hairline)]">
                <th className="px-4 py-3 font-medium">Name</th>
                <th className="px-4 py-3 font-medium">Tenant</th>
                <th className="px-4 py-3 font-medium">Host</th>
                <th className="px-4 py-3 font-medium">Phase</th>
                <th className="px-4 py-3 font-medium">Replicas</th>
                <th className="px-4 py-3 font-medium">Restart Policy</th>
                <th className="px-4 py-3 font-medium">Containers</th>
                <th className="px-4 py-3 font-medium">Network Policy</th>
                <th className="px-4 py-3 font-medium"></th>
              </tr>
            </thead>
            <tbody>
              {groups.map((g) => {
                const st = statusByName[g.name]
                const phases = st?.pods.map((p) => p.phase).filter(Boolean) as string[] | undefined
                const phaseSummary =
                  !phases || phases.length === 0
                    ? '—'
                    : [...new Set(phases)].join(', ')
                return (
                <Fragment key={g.name}>
                <tr className="border-b border-[var(--zf-hairline)] last:border-0">
                  <td className="px-4 py-3 font-medium text-[var(--zf-ink)]">
                    <button
                      type="button"
                      className="text-left hover:underline"
                      onClick={() => setExpanded(expanded === g.name ? null : g.name)}
                    >
                      {g.name}
                    </button>
                  </td>
                  <td className="px-4 py-3 text-[var(--zf-muted)]">{g.tenant ?? '—'}</td>
                  <td className="px-4 py-3 text-[var(--zf-muted)]">{st?.host_name ?? '—'}</td>
                  <td className="px-4 py-3 text-[var(--zf-muted)]">{phaseSummary}</td>
                  <td className="px-4 py-3">{g.replicas ?? 1}</td>
                  <td className="px-4 py-3">{g.restart_policy ?? 'Always'}</td>
                  <td className="px-4 py-3">{g.containers.map((c) => c.image).join(', ')}</td>
                  <td className="px-4 py-3">
                    {g.network_policy ? (
                      <span className="px-2 py-0.5 rounded text-xs border text-[var(--zf-link)] bg-[var(--zf-link)]/10 border-[var(--zf-link)]/20">
                        isolated
                      </span>
                    ) : (
                      <span className="text-[var(--zf-muted)]">—</span>
                    )}
                  </td>
                  <td className="px-4 py-3">
                    <div className="flex justify-end gap-2">
                      <button
                        onClick={() => void handleBackup(g.name)}
                        className="zf-btn zf-btn-ghost zf-btn-sm"
                        title="Backup volumes"
                      >
                        <Save className="w-3.5 h-3.5" />
                      </button>
                      <button
                        onClick={() => void handleDelete(g.name)}
                        className="zf-btn zf-btn-danger zf-btn-sm"
                        title="Delete"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </td>
                </tr>
                {expanded === g.name && st && (
                  <tr key={`${g.name}-detail`} className="bg-[var(--zf-canvas)]/60">
                    <td colSpan={9} className="px-4 py-3 text-sm text-[var(--zf-muted)]">
                      {st.pods.length === 0 ? (
                        <span>No live Pod status yet.</span>
                      ) : (
                        <ul className="space-y-1">
                          {st.pods.map((p) => (
                            <li key={p.name} className="font-mono text-xs">
                              {p.name} · phase={p.phase ?? '—'} · node={p.node_name ?? '—'} · ip=
                              {p.pod_ip ?? '—'}
                            </li>
                          ))}
                        </ul>
                      )}
                      {events.filter((e) => e.container_group_name === g.name).length > 0 && (
                        <div className="mt-2">
                          <div className="text-xs font-medium text-[var(--zf-ink)] mb-1">Recent events</div>
                          <ul className="space-y-0.5">
                            {events
                              .filter((e) => e.container_group_name === g.name)
                              .slice(0, 5)
                              .map((e) => (
                                <li key={e.id} className="text-xs">
                                  {e.event_type}
                                  {e.detail ? `: ${e.detail}` : ''}
                                </li>
                              ))}
                          </ul>
                        </div>
                      )}
                    </td>
                  </tr>
                )}
                </Fragment>
                )
              })}
            </tbody>
          </table>
        </Card>
      )}

      <h2 className="text-xl font-semibold text-[var(--zf-ink)] mb-4">Volume Backups</h2>
      {backups.length === 0 ? (
        <Card className="mb-8">
          <CardBody className="text-center text-[var(--zf-muted)]">No backups yet</CardBody>
        </Card>
      ) : (
        <Card className="overflow-x-auto mb-8">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-[var(--zf-muted)] border-b border-[var(--zf-hairline)]">
                <th className="px-4 py-3 font-medium">ContainerGroup</th>
                <th className="px-4 py-3 font-medium">Status</th>
                <th className="px-4 py-3 font-medium">Size</th>
                <th className="px-4 py-3 font-medium">Created</th>
                <th className="px-4 py-3 font-medium">Expires</th>
                <th className="px-4 py-3 font-medium"></th>
              </tr>
            </thead>
            <tbody>
              {backups.map((b) => (
                <tr key={b.id} className="border-b border-[var(--zf-hairline)] last:border-0">
                  <td className="px-4 py-3 font-medium text-[var(--zf-ink)]">{b.container_group_name}</td>
                  <td className="px-4 py-3">
                    <span
                      className={`px-2 py-0.5 rounded text-xs border ${
                        b.status === 'completed'
                          ? 'text-emerald-700 bg-emerald-50 border-emerald-200'
                          : 'text-red-700 bg-red-50 border-red-200'
                      }`}
                    >
                      {b.status}
                    </span>
                  </td>
                  <td className="px-4 py-3">{formatBytes(b.size_bytes)}</td>
                  <td className="px-4 py-3">
                    <RelativeTime date={b.created} />
                  </td>
                  <td className="px-4 py-3">
                    <RelativeTime date={b.expires_at} />
                  </td>
                  <td className="px-4 py-3">
                    <div className="flex justify-end gap-2">
                      <button
                        onClick={() => void handleRestoreBackup(b)}
                        className="zf-btn zf-btn-ghost zf-btn-sm"
                        title="Restore"
                      >
                        <RotateCcw className="w-3.5 h-3.5" />
                      </button>
                      <button
                        onClick={() => void handleDeleteBackup(b)}
                        className="zf-btn zf-btn-danger zf-btn-sm"
                        title="Delete"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}

      <h2 className="text-xl font-semibold text-[var(--zf-ink)] mb-4">Recent Events</h2>
      {events.length === 0 ? (
        <Card>
          <CardBody className="text-center text-[var(--zf-muted)]">No events yet</CardBody>
        </Card>
      ) : (
        <Card className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-[var(--zf-muted)] border-b border-[var(--zf-hairline)]">
                <th className="px-4 py-3 font-medium">Type</th>
                <th className="px-4 py-3 font-medium">ContainerGroup</th>
                <th className="px-4 py-3 font-medium">Actor</th>
                <th className="px-4 py-3 font-medium">Detail</th>
                <th className="px-4 py-3 font-medium">When</th>
              </tr>
            </thead>
            <tbody>
              {events.slice(0, 50).map((e) => (
                <tr key={e.id} className="border-b border-[var(--zf-hairline)] last:border-0">
                  <td className="px-4 py-3">
                    <span className={`px-2 py-0.5 rounded text-xs border ${eventTypeStyle(e.event_type)}`}>
                      {e.event_type}
                    </span>
                  </td>
                  <td className="px-4 py-3 font-medium text-[var(--zf-ink)]">{e.container_group_name}</td>
                  <td className="px-4 py-3 text-[var(--zf-muted)]">{e.actor}</td>
                  <td className="px-4 py-3 text-[var(--zf-muted)]">
                    {e.detail ? (
                      <span className="inline-flex items-center gap-1">
                        <FileText className="w-3.5 h-3.5" />
                        {e.detail}
                      </span>
                    ) : (
                      '—'
                    )}
                  </td>
                  <td className="px-4 py-3">
                    <RelativeTime date={e.timestamp} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}

      {showApplyDialog && <ApplySpecDialog onClose={() => setShowApplyDialog(false)} onSuccess={loadData} />}

      {confirmState && (
        <ConfirmDialog
          title={confirmState.title}
          message={confirmState.message}
          confirmLabel={confirmState.confirmLabel}
          variant={confirmState.variant}
          onConfirm={confirmState.onConfirm}
          onCancel={cancel}
        />
      )}
    </div>
  )
}
