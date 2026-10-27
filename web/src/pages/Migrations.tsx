// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react'
import { ArrowRightLeft, Plus, XCircle } from 'lucide-react'
import {
  listMigrations,
  startMigration,
  cancelMigration,
  getRuntimeCapabilities,
  startNativeMigration,
  getNativeMigrationStatus,
  cancelNativeMigration,
  MigrationStatus,
  MigrationRequest,
  MigrationType,
  RuntimeCapabilities,
  NativeMigrationStatus,
} from '../api/migrations'
import { listVMs, VM } from '../api/vm'
import { useToastContext } from '../contexts/ToastContext'
import ErrorBanner from '../components/ErrorBanner'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import { PageHeader } from '../components/ui'

const NATIVE_TRACK_KEY = 'zyvor.nativeMigrations'

function loadTrackedNativeVms(): string[] {
  try {
    const raw = sessionStorage.getItem(NATIVE_TRACK_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === 'string') : []
  } catch {
    return []
  }
}

function saveTrackedNativeVms(names: string[]) {
  sessionStorage.setItem(NATIVE_TRACK_KEY, JSON.stringify([...new Set(names)]))
}

function isNativeTerminal(phase: string) {
  return ['completed', 'failed', 'cancelled', 'none'].includes(phase)
}

export default function Migrations() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [migrations, setMigrations] = useState<MigrationStatus[]>([])
  const [nativeStatuses, setNativeStatuses] = useState<NativeMigrationStatus[]>([])
  const [capabilities, setCapabilities] = useState<RuntimeCapabilities | null>(null)
  const [capsError, setCapsError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [showStartDialog, setShowStartDialog] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)

  useEffect(() => {
    void loadAll(false)
    const interval = setInterval(() => void loadAll(true), 5000)
    return () => clearInterval(interval)
  }, [])

  const loadNative = async () => {
    const tracked = loadTrackedNativeVms()
    if (tracked.length === 0) {
      setNativeStatuses([])
      return
    }
    const results: NativeMigrationStatus[] = []
    await Promise.all(
      tracked.map(async (name) => {
        try {
          results.push(await getNativeMigrationStatus(name))
        } catch {
          // Drop VMs with no migration session.
        }
      }),
    )
    setNativeStatuses(results)
    saveTrackedNativeVms(
      results.filter((r) => !isNativeTerminal(r.phase)).map((r) => r.vm_name),
    )
  }

  const loadAll = async (silent = false) => {
    if (!silent) setLoadError(null)
    try {
      const [data, caps] = await Promise.all([
        listMigrations(),
        getRuntimeCapabilities().catch((e) => {
          setCapsError(formatUserError(e))
          return null
        }),
      ])
      setMigrations(data)
      if (caps) {
        setCapabilities(caps)
        setCapsError(null)
      }
      await loadNative()
      setLoadError(null)
    } catch (error) {
      const msg = formatUserError(error)
      if (!silent || migrations.length === 0) {
        setLoadError(msg)
        if (!silent) toastFailure(toast, 'Failed to load migrations', error)
      }
    } finally {
      if (!silent) setLoading(false)
    }
  }

  const handleCancel = async (id: string) => {
    if (!await confirm('Cancel Migration', 'Cancel this migration in progress? It may leave the VM in a partially-migrated state.', { variant: 'danger', confirmLabel: 'Cancel Migration' })) return
    try {
      await cancelMigration(id)
      toast.success('Migration cancelled')
      void loadAll(true)
    } catch (error) {
      toastFailure(toast, 'Failed to cancel migration', error)
    }
  }

  const handleCancelNative = async (vmName: string) => {
    if (!await confirm('Cancel Native Migration', `Cancel FluxVM migration transport for '${vmName}'?`, { variant: 'danger', confirmLabel: 'Cancel Migration' })) return
    try {
      await cancelNativeMigration(vmName)
      toast.success('Native migration cancelled')
      void loadAll(true)
    } catch (error) {
      toastFailure(toast, 'Failed to cancel native migration', error)
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-[var(--zf-ink)]"></div>
      </div>
    )
  }

  const activeMigrations = migrations.filter(m =>
    !['completed', 'failed', 'cancelled'].includes(m.state)
  )
  const completedMigrations = migrations.filter(m =>
    ['completed', 'failed', 'cancelled'].includes(m.state)
  )
  const activeNative = nativeStatuses.filter((s) => !isNativeTerminal(s.phase))
  const liveCap = capabilities?.migration.find((m) => m.live)

  return (
    <div className="space-y-6">
      {loadError && (
        <ErrorBanner
          title="Could not load migrations"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={() => void loadAll()}
        />
      )}
      <PageHeader
        title="Migrations"
        description="Move VMs between hosts for load balancing or maintenance"
        onRefresh={() => void loadAll()}
        primaryAction={
          <button
            onClick={() => setShowStartDialog(true)}
            className="zf-btn zf-btn-primary zf-btn-sm"
          >
            <Plus className="w-4 h-4" />
            Start Migration
          </button>
        }
      />

      <div className="bg-[var(--zf-surface)] rounded-lg border border-[var(--zf-hairline)] p-4 space-y-2">
        <h2 className="text-sm font-semibold text-[var(--zf-ink)]">FluxVM runtime capabilities</h2>
        {capsError && !capabilities && (
          <p className="text-sm text-red-700">{capsError}</p>
        )}
        {capabilities && (
          <div className="text-sm text-[var(--zf-muted)] space-y-1">
            <p>
              Contract <span className="font-mono text-[var(--zf-ink)]">{capabilities.apiVersion}</span>
              {' '}· scope <span className="font-mono">{capabilities.scope}</span>
              {' '}· orchestrator <span className="font-mono">{capabilities.orchestrationOwner}</span>
            </p>
            {liveCap ? (
              <p>
                Live migration: {liveCap.backend}
                {liveCap.preCopy ? ' · pre-copy' : ''}
                {liveCap.postCopy ? ' · post-copy' : ''}
                {liveCap.multifd ? ' · multifd' : ''}
                {' · transports '}
                <span className="font-mono">{liveCap.transports.join(', ') || 'none'}</span>
                {liveCap.requiresSharedStorage ? ' · requires shared storage' : ''}
              </p>
            ) : (
              <p>No live migration capability advertised by this FluxVM node.</p>
            )}
            <p className="text-xs">
              Source-side transport only — target receiver arming is Phase 2; not marketed as GA.
            </p>
          </div>
        )}
      </div>

      {activeNative.length > 0 && (
        <div className="space-y-4">
          <h2 className="text-xl font-semibold">Active native (FluxVM) migrations</h2>
          {activeNative.map((status) => (
            <NativeMigrationCard
              key={status.vm_name}
              status={status}
              onCancel={() => void handleCancelNative(status.vm_name)}
            />
          ))}
        </div>
      )}

      {activeMigrations.length > 0 && (
        <div className="space-y-4">
          <h2 className="text-xl font-semibold">Active Migrations</h2>
          {activeMigrations.map(migration => (
            <MigrationCard
              key={migration.id}
              migration={migration}
              onCancel={() => void handleCancel(migration.id)}
            />
          ))}
        </div>
      )}

      <div className="space-y-4">
        <h2 className="text-xl font-semibold">Migration History</h2>
        {completedMigrations.length === 0 && activeMigrations.length === 0 && activeNative.length === 0 ? (
          <div className="text-center py-12 bg-[var(--zf-surface)] rounded-lg border border-[var(--zf-hairline)]">
            <ArrowRightLeft className="w-16 h-16 mx-auto mb-4 text-[var(--zf-muted)]" />
            <p className="text-xl text-[var(--zf-muted)] mb-4">No migrations yet</p>
            <p className="text-[var(--zf-muted)] mb-6">Migrate VMs between hosts for load balancing or maintenance</p>
            <button
              onClick={() => setShowStartDialog(true)}
              className="zf-btn zf-btn-primary"
            >
              Start Migration
            </button>
          </div>
        ) : (
          <div className="bg-[var(--zf-surface)] rounded-lg border border-[var(--zf-hairline)]">
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead className="bg-[var(--zf-surface)]">
                  <tr>
                    <th className="text-left p-4 font-medium text-[var(--zf-ink)]">VM</th>
                    <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Target Host</th>
                    <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Type</th>
                    <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Status</th>
                    <th className="text-left p-4 font-medium text-[var(--zf-ink)]">Started</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-[var(--zf-hairline)]">
                  {completedMigrations.map(migration => (
                    <tr key={migration.id} className="hover:bg-black/[0.03] transition">
                      <td className="p-4 font-medium">{migration.vm_name}</td>
                      <td className="p-4 font-mono text-sm text-[var(--zf-muted)]">{migration.target_host}</td>
                      <td className="p-4 capitalize">{migration.migration_type}</td>
                      <td className="p-4">
                        <StatusBadge state={migration.state} />
                      </td>
                      <td className="p-4 text-sm text-[var(--zf-muted)]">
                        {new Date(migration.started).toLocaleString()}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>

      {showStartDialog && (
        <StartMigrationDialog
          capabilities={capabilities}
          onClose={() => setShowStartDialog(false)}
          onSuccess={(nativeVm?: string) => {
            toast.success(nativeVm ? 'Native migration started' : 'Migration started')
            setShowStartDialog(false)
            if (nativeVm) {
              const tracked = loadTrackedNativeVms()
              saveTrackedNativeVms([...tracked, nativeVm])
            }
            void loadAll(true)
          }}
        />
      )}

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

function StatusBadge({ state }: { state: string }) {
  const styles: Record<string, string> = {
    pending: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
    precheck: 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
    syncing: 'text-amber-800 bg-amber-50 border-amber-200',
    switching: 'text-amber-800 bg-amber-50 border-amber-200',
    active: 'text-amber-800 bg-amber-50 border-amber-200',
    setup: 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
    'postcopy-active': 'text-amber-800 bg-amber-50 border-amber-200',
    completed: 'text-emerald-700 bg-emerald-50 border-emerald-200',
    failed: 'text-red-700 bg-red-50 border-red-200',
    cancelled: 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]',
  }

  return (
    <span className={`px-3 py-1 rounded-full text-xs font-medium border ${styles[state] || styles.pending}`}>
      {state}
    </span>
  )
}

function MigrationCard({
  migration,
  onCancel,
}: {
  migration: MigrationStatus
  onCancel: () => void
}) {
  return (
    <div className="bg-[var(--zf-surface)] rounded-lg p-6 border border-[var(--zf-hairline)]">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h3 className="text-lg font-bold">{migration.vm_name}</h3>
          <p className="text-sm text-[var(--zf-muted)]">
            To: <span className="font-mono">{migration.target_host}</span>
            {' '}&middot;{' '}
            <span className="capitalize">{migration.migration_type}</span> migration
          </p>
        </div>
        <div className="flex items-center gap-3">
          <StatusBadge state={migration.state} />
          <button
            onClick={onCancel}
            className="zf-btn zf-btn-danger zf-btn-sm"
          >
            <XCircle className="w-4 h-4" />
            Cancel
          </button>
        </div>
      </div>

      <div className="w-full bg-[var(--zf-canvas)] rounded-full h-2 mb-2">
        <div
          className="bg-[var(--zf-link-fill)] h-2 rounded-full transition-all duration-500"
          style={{ width: `${migration.progress_percent}%` }}
        />
      </div>
      <div className="flex justify-between text-xs text-[var(--zf-muted)]">
        <span>{migration.progress_percent}% complete</span>
        {migration.bytes_transferred > 0 && (
          <span>{(migration.bytes_transferred / (1024 * 1024)).toFixed(1)} MB transferred</span>
        )}
      </div>

      {migration.error && (
        <div className="mt-3 p-3 bg-red-50 border border-red-200 rounded text-sm text-red-700">
          {migration.error}
        </div>
      )}
    </div>
  )
}

function NativeMigrationCard({
  status,
  onCancel,
}: {
  status: NativeMigrationStatus
  onCancel: () => void
}) {
  const transferred = status.ram_transferred ?? 0
  return (
    <div className="bg-[var(--zf-surface)] rounded-lg p-6 border border-[var(--zf-hairline)]">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h3 className="text-lg font-bold">{status.vm_name}</h3>
          <p className="text-sm text-[var(--zf-muted)]">
            Native FluxVM transport · {status.status}
          </p>
        </div>
        <div className="flex items-center gap-3">
          <StatusBadge state={status.phase} />
          <button onClick={onCancel} className="zf-btn zf-btn-danger zf-btn-sm">
            <XCircle className="w-4 h-4" />
            Cancel
          </button>
        </div>
      </div>
      <div className="w-full bg-[var(--zf-canvas)] rounded-full h-2 mb-2">
        <div
          className="bg-[var(--zf-link-fill)] h-2 rounded-full transition-all duration-500"
          style={{ width: `${status.progress_percent}%` }}
        />
      </div>
      <div className="flex justify-between text-xs text-[var(--zf-muted)]">
        <span>{status.progress_percent}% complete</span>
        {transferred > 0 && (
          <span>{(transferred / (1024 * 1024)).toFixed(1)} MB RAM transferred</span>
        )}
      </div>
      {status.error && (
        <div className="mt-3 p-3 bg-red-50 border border-red-200 rounded text-sm text-red-700">
          {status.error}
        </div>
      )}
    </div>
  )
}

function StartMigrationDialog({
  capabilities,
  onClose,
  onSuccess,
}: {
  capabilities: RuntimeCapabilities | null
  onClose: () => void
  onSuccess: (nativeVm?: string) => void
}) {
  const toast = useToastContext()
  const [vms, setVMs] = useState<VM[]>([])
  const [vmName, setVmName] = useState('')
  const [targetHost, setTargetHost] = useState('')
  const [targetUri, setTargetUri] = useState('')
  const [migrationType, setMigrationType] = useState<MigrationType>('offline')
  const [mode, setMode] = useState('pre-copy')
  const [sharedStorage, setSharedStorage] = useState(false)
  const [isStarting, setIsStarting] = useState(false)

  const liveCap = capabilities?.migration.find((m) => m.live)
  const needsShared = !!liveCap?.requiresSharedStorage

  useEffect(() => {
    listVMs()
      .then(setVMs)
      .catch((e) => toastFailure(toast, 'Failed to load VMs', e))
  }, [toast])

  const handleStart = async () => {
    if (!vmName) {
      toast.error('Please select a VM')
      return
    }

    setIsStarting(true)
    try {
      if (migrationType === 'live') {
        if (!targetUri.trim()) {
          toast.error('Enter a migration URI (e.g. tcp:10.0.0.2:4444)')
          return
        }
        if (needsShared && !sharedStorage) {
          toast.error('Confirm shared storage before starting live migration')
          return
        }
        await startNativeMigration(vmName, {
          target_uri: targetUri.trim(),
          mode,
          shared_storage_confirmed: sharedStorage,
          multifd_channels: liveCap?.multifd ? 4 : 1,
        })
        onSuccess(vmName)
      } else {
        if (!targetHost.trim()) {
          toast.error('Please enter a target host')
          return
        }
        const req: MigrationRequest = {
          vm_name: vmName,
          target_host: targetHost,
          migration_type: migrationType,
        }
        await startMigration(req)
        onSuccess()
      }
    } catch (error) {
      toastFailure(toast, 'Failed to start migration', error)
    } finally {
      setIsStarting(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="bg-[var(--zf-surface)] rounded-lg border border-[var(--zf-hairline)] w-full max-w-md">
        <div className="flex items-center justify-between p-6 border-b border-[var(--zf-hairline)]">
          <h2 className="text-xl font-bold">Start Migration</h2>
          <button onClick={onClose} className="p-2 hover:bg-black/[0.04] rounded transition">
            <span className="text-2xl">&times;</span>
          </button>
        </div>

        <div className="p-6 space-y-4">
          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">VM</label>
            <select
              value={vmName}
              onChange={(e) => setVmName(e.target.value)}
              className="input-field"
            >
              <option value="">Select a VM</option>
              {vms.map(vm => (
                <option key={vm.name} value={vm.name}>{vm.name}</option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Migration Type</label>
            <select
              value={migrationType}
              onChange={(e) => setMigrationType(e.target.value as MigrationType)}
              className="input-field"
            >
              <option value="offline">Offline - Stop VM, copy data, start on target</option>
              <option value="live">Live - FluxVM native transport (prepared target)</option>
              <option value="storage">Storage - Migrate storage volumes only</option>
            </select>
          </div>

          {migrationType === 'live' ? (
            <>
              <div>
                <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">
                  Target URI
                </label>
                <input
                  type="text"
                  value={targetUri}
                  onChange={(e) => setTargetUri(e.target.value)}
                  placeholder="tcp:10.0.0.2:4444"
                  className="input-field font-mono"
                />
                {liveCap && (
                  <p className="mt-1 text-xs text-[var(--zf-muted)]">
                    Transports: {liveCap.transports.join(', ') || 'none'}
                  </p>
                )}
              </div>
              <div>
                <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Mode</label>
                <select
                  value={mode}
                  onChange={(e) => setMode(e.target.value)}
                  className="input-field"
                >
                  <option value="pre-copy">Pre-copy</option>
                  {liveCap?.postCopy && <option value="post-copy">Post-copy</option>}
                </select>
              </div>
              <label className="flex items-start gap-2 text-sm text-[var(--zf-ink)]">
                <input
                  type="checkbox"
                  className="mt-1"
                  checked={sharedStorage}
                  onChange={(e) => setSharedStorage(e.target.checked)}
                />
                <span>
                  Shared storage confirmed
                  {needsShared ? ' (required by this FluxVM node)' : ' (optional)'}
                </span>
              </label>
            </>
          ) : (
            <div>
              <label className="block text-sm font-medium text-[var(--zf-ink)] mb-2">Target Host</label>
              <input
                type="text"
                value={targetHost}
                onChange={(e) => setTargetHost(e.target.value)}
                placeholder="hostname or IP address"
                className="input-field"
              />
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 p-6 border-t border-[var(--zf-hairline)]">
          <button
            onClick={onClose}
            disabled={isStarting}
            className="zf-btn zf-btn-ghost"
          >
            Cancel
          </button>
          <button
            onClick={() => void handleStart()}
            disabled={isStarting}
            className="zf-btn zf-btn-primary"
          >
            {isStarting ? 'Starting...' : 'Start Migration'}
          </button>
        </div>
      </div>
    </div>
  )
}
