// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useEffect, useState, useCallback } from 'react'
import { Plus, Edit, Trash2, Power, PowerOff, AlertTriangle } from 'lucide-react'
import {
  listQuotas,
  ResourceQuota,
  deleteQuota,
  enableQuota,
  disableQuota,
  QuotaUsage,
  getAllQuotaUsage
} from '../api/quota'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import QuotaDialog from '../components/QuotaDialog'
import { PageHeader } from '../components/ui'
import PageLoadBanner from '../components/PageLoadBanner'
import { usePageLoader } from '../hooks/usePageLoader'
import { toastFailure } from '../utils/toastError'

export default function Quotas() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [quotas, setQuotas] = useState<ResourceQuota[]>([])
  const [usage, setUsage] = useState<QuotaUsage[]>([])
  const { loading, loadError, run } = usePageLoader('Failed to load quotas')
  const [showCreateDialog, setShowCreateDialog] = useState(false)
  const [editingQuota, setEditingQuota] = useState<ResourceQuota | null>(null)

  const loadData = useCallback(() => {
    return run(async () => {
      const [quotasData, usageData] = await Promise.all([listQuotas(), getAllQuotaUsage()])
      setQuotas(quotasData)
      setUsage(usageData)
    })
  }, [run])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const handleDelete = async (id: string) => {
    const ok = await confirm('Delete Quota', 'Delete this quota? This action cannot be undone.', { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return

    try {
      await deleteQuota(id)
      toast.success('Quota deleted successfully')
      loadData()
    } catch (error) {
      toastFailure(toast, 'Failed to delete quota', error)
    }
  }

  const handleToggleEnabled = async (quota: ResourceQuota) => {
    try {
      if (quota.enabled) {
        await disableQuota(quota.id)
        toast.success('Quota disabled')
      } else {
        await enableQuota(quota.id)
        toast.success('Quota enabled')
      }
      loadData()
    } catch (error) {
      toastFailure(toast, `Failed to ${quota.enabled ? 'disable' : 'enable'} quota`, error)
    }
  }

  const getUsageForQuota = (quotaId: string): QuotaUsage | undefined => {
    return usage.find(u => u.quota_id === quotaId)
  }

  const getUsageColor = (percent: number): string => {
    if (percent >= 90) return 'bg-[var(--zf-danger)]'
    if (percent >= 75) return 'bg-[var(--zf-warning)]'
    return 'bg-[var(--zf-success)]'
  }


  return (
    <div>
      <PageHeader
        onRefresh={() => void loadData()}
        refreshing={loading}
        title="Resource Quotas"
        description="Manage resource limits and allocations"
        actions={
          <button
            onClick={() => setShowCreateDialog(true)}
            className="zf-btn zf-btn-primary"
          >
            <Plus className="w-4 h-4" />
            Create Quota
          </button>
        }
      />

      <PageLoadBanner title="Could not load quotas" headline={loadError} onRetry={() => void loadData()} />

      {quotas.length === 0 ? (
        <div className="text-center py-12 zf-panel">
          <p className="text-xl text-[var(--zf-muted)] mb-4">No quotas configured</p>
          <p className="text-[var(--zf-muted)] mb-6">Create quotas to limit resource usage</p>
          <button
            onClick={() => setShowCreateDialog(true)}
            className="zf-btn zf-btn-primary"
          >
            <Plus className="w-4 h-4" />
            Create First Quota
          </button>
        </div>
      ) : (
        <div className="space-y-6">
          {quotas.map((quota) => {
            const quotaUsage = getUsageForQuota(quota.id)
            const isExceeded = quotaUsage?.is_exceeded || false

            return (
              <div
                key={quota.id}
                className={`bg-[var(--zf-surface)] rounded-[var(--zf-radius)] border ${
                  isExceeded ? 'border-[var(--zf-danger)]' : 'border-[var(--zf-hairline)]'
                } p-6`}
              >
                {/* Header */}
                <div className="flex items-start justify-between mb-6">
                  <div className="flex-1">
                    <div className="flex items-center gap-3 mb-2">
                      <h3 className="text-xl font-bold text-[var(--zf-ink)]">{quota.name}</h3>
                      <span
                        className={`px-2 py-1 rounded text-xs font-medium border ${
                          quota.enabled
                            ? 'text-emerald-700 bg-emerald-50 border-emerald-200'
                            : 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
                        }`}
                      >
                        {quota.enabled ? 'Enabled' : 'Disabled'}
                      </span>
                      {isExceeded && (
                        <span className="flex items-center gap-1 px-2 py-1 rounded text-xs font-medium border text-red-700 bg-red-50 border-red-200">
                          <AlertTriangle className="w-3.5 h-3.5" />
                          Exceeded
                        </span>
                      )}
                    </div>
                    {quota.tags && quota.tags.length > 0 && (
                      <div className="flex gap-2 mb-2">
                        <span className="text-sm text-[var(--zf-muted)]">Tags:</span>
                        {quota.tags.map((tag) => (
                          <span
                            key={tag}
                            className="px-2 py-0.5 rounded text-xs border text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]"
                          >
                            {tag}
                          </span>
                        ))}
                      </div>
                    )}
                    <p className="text-sm text-[var(--zf-muted)]">
                      Created: {new Date(quota.created).toLocaleDateString()}
                    </p>
                  </div>

                  <div className="flex gap-2">
                    <button
                      onClick={() => handleToggleEnabled(quota)}
                      className={`p-2 rounded border transition ${
                        quota.enabled
                          ? 'text-amber-800 bg-amber-50 border-amber-200 hover:bg-amber-100'
                          : 'text-emerald-700 bg-emerald-50 border-emerald-200 hover:bg-emerald-100'
                      }`}
                      title={quota.enabled ? 'Disable' : 'Enable'}
                    >
                      {quota.enabled ? (
                        <PowerOff className="w-4 h-4" />
                      ) : (
                        <Power className="w-4 h-4" />
                      )}
                    </button>
                    <button
                      onClick={() => setEditingQuota(quota)}
                      className="zf-btn zf-btn-ghost zf-btn-sm"
                      title="Edit"
                    >
                      <Edit className="w-3.5 h-3.5" />
                    </button>
                    <button
                      onClick={() => handleDelete(quota.id)}
                      className="zf-btn zf-btn-danger zf-btn-sm"
                      title="Delete"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>

                {/* Resource Limits */}
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
                  {/* CPUs */}
                  <div>
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm text-[var(--zf-muted)]">CPUs</span>
                      <span className="text-sm font-medium text-[var(--zf-ink)]">
                        {quota.used_cpus} / {quota.max_cpus}
                      </span>
                    </div>
                    <div className="h-2 bg-[var(--zf-canvas)] rounded-full overflow-hidden">
                      <div
                        className={`h-full ${getUsageColor(quotaUsage?.cpu_percent || 0)}`}
                        style={{
                          width: `${Math.min((quota.used_cpus / quota.max_cpus) * 100, 100)}%`,
                        }}
                      />
                    </div>
                    {quotaUsage && (
                      <p className="text-xs text-[var(--zf-muted)] mt-1">
                        {quotaUsage.cpu_percent.toFixed(1)}% used
                      </p>
                    )}
                  </div>

                  {/* Memory */}
                  <div>
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm text-[var(--zf-muted)]">Memory</span>
                      <span className="text-sm font-medium text-[var(--zf-ink)]">
                        {quota.used_memory}MB / {quota.max_memory}MB
                      </span>
                    </div>
                    <div className="h-2 bg-[var(--zf-canvas)] rounded-full overflow-hidden">
                      <div
                        className={`h-full ${getUsageColor(quotaUsage?.memory_percent || 0)}`}
                        style={{
                          width: `${Math.min((quota.used_memory / quota.max_memory) * 100, 100)}%`,
                        }}
                      />
                    </div>
                    {quotaUsage && (
                      <p className="text-xs text-[var(--zf-muted)] mt-1">
                        {quotaUsage.memory_percent.toFixed(1)}% used
                      </p>
                    )}
                  </div>

                  {/* Disk */}
                  <div>
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm text-[var(--zf-muted)]">Disk</span>
                      <span className="text-sm font-medium text-[var(--zf-ink)]">
                        {quota.used_disk}GB / {quota.max_disk}GB
                      </span>
                    </div>
                    <div className="h-2 bg-[var(--zf-canvas)] rounded-full overflow-hidden">
                      <div
                        className={`h-full ${getUsageColor(quotaUsage?.disk_percent || 0)}`}
                        style={{
                          width: `${Math.min((quota.used_disk / quota.max_disk) * 100, 100)}%`,
                        }}
                      />
                    </div>
                    {quotaUsage && (
                      <p className="text-xs text-[var(--zf-muted)] mt-1">
                        {quotaUsage.disk_percent.toFixed(1)}% used
                      </p>
                    )}
                  </div>

                  {/* VMs */}
                  <div>
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm text-[var(--zf-muted)]">VMs</span>
                      <span className="text-sm font-medium text-[var(--zf-ink)]">
                        {quota.used_vms} / {quota.max_vms}
                      </span>
                    </div>
                    <div className="h-2 bg-[var(--zf-canvas)] rounded-full overflow-hidden">
                      <div
                        className={`h-full ${getUsageColor(quotaUsage?.vms_percent || 0)}`}
                        style={{
                          width: `${Math.min((quota.used_vms / quota.max_vms) * 100, 100)}%`,
                        }}
                      />
                    </div>
                    {quotaUsage && (
                      <p className="text-xs text-[var(--zf-muted)] mt-1">
                        {quotaUsage.vms_percent.toFixed(1)}% used
                      </p>
                    )}
                  </div>
                </div>

                {/* Exceeded Resources Warning */}
                {isExceeded && quotaUsage && quotaUsage.exceeded_resources.length > 0 && (
                  <div className="mt-4 p-3 bg-red-50 border border-red-200 rounded flex items-start gap-2">
                    <AlertTriangle className="w-5 h-5 text-[var(--zf-danger)] flex-shrink-0 mt-0.5" />
                    <div>
                      <p className="text-sm font-medium text-red-700">
                        Quota exceeded for:{' '}
                        {quotaUsage.exceeded_resources.join(', ')}
                      </p>
                      <p className="text-xs text-red-700 mt-1">
                        New VMs matching this quota cannot be created until resources are freed
                      </p>
                    </div>
                  </div>
                )}
              </div>
            )
          })}
        </div>
      )}

      {showCreateDialog && (
        <QuotaDialog
          mode="create"
          onClose={() => setShowCreateDialog(false)}
          onSuccess={loadData}
        />
      )}

      {editingQuota && (
        <QuotaDialog
          mode="edit"
          quota={editingQuota}
          onClose={() => setEditingQuota(null)}
          onSuccess={loadData}
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
