// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react'
import { Plus, Edit, Trash2, Power, PowerOff, Play, Clock, Calendar } from 'lucide-react'
import {
  listSchedules,
  Schedule,
  deleteSchedule,
  enableSchedule,
  disableSchedule,
  runScheduleNow,
  ScheduleHistory,
  getScheduleHistory
} from '../api/schedule'
import { useToastContext } from '../contexts/ToastContext'
import { useConfirm } from '../hooks/useConfirm'
import ConfirmDialog from '../components/ConfirmDialog'
import ScheduleDialog from '../components/ScheduleDialog'
import { PageHeader } from '../components/ui'
import ErrorBanner from '../components/ErrorBanner'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'
import { hintsForError } from '../utils/daemonHints'
import RelativeTime from '../components/RelativeTime'
import TableSearch from '../components/TableSearch'
import { useTableFilter } from '../hooks/useTableFilter'

export default function Schedules() {
  const toast = useToastContext()
  const { confirmState, confirm, cancel } = useConfirm()
  const [schedules, setSchedules] = useState<Schedule[]>([])
  const [history, setHistory] = useState<ScheduleHistory[]>([])
  const [loading, setLoading] = useState(true)
  const [showCreateDialog, setShowCreateDialog] = useState(false)
  const [editingSchedule, setEditingSchedule] = useState<Schedule | null>(null)
  const [showHistory, setShowHistory] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)
  const { query: scheduleQuery, setQuery: setScheduleQuery, filtered: filteredSchedules } = useTableFilter(schedules, (s) => [s.name, s.vm_name, s.action])

  useEffect(() => {
    loadData()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const loadData = async () => {
    setLoadError(null)
    const [schedRes, histRes] = await Promise.allSettled([
      listSchedules(),
      getScheduleHistory(),
    ])
    if (schedRes.status === 'fulfilled') {
      setSchedules(schedRes.value)
    } else {
      const msg = formatUserError(schedRes.reason)
      setLoadError(msg)
      toastFailure(toast, 'Failed to load schedules', schedRes.reason)
      setSchedules([])
    }
    if (histRes.status === 'fulfilled') {
      setHistory(histRes.value.slice(0, 20))
    } else {
      setHistory([])
    }
    setLoading(false)
  }

  const handleDelete = async (id: string) => {
    const ok = await confirm('Delete Schedule', 'Delete this schedule? This action cannot be undone.', { variant: 'danger', confirmLabel: 'Delete' })
    if (!ok) return

    try {
      await deleteSchedule(id)
      toast.success('Schedule deleted successfully')
      loadData()
    } catch (error) {
      toastFailure(toast, 'Failed to delete schedule', error)
    }
  }

  const handleToggleEnabled = async (schedule: Schedule) => {
    try {
      if (schedule.enabled) {
        await disableSchedule(schedule.id)
        toast.success('Schedule disabled')
      } else {
        await enableSchedule(schedule.id)
        toast.success('Schedule enabled')
      }
      loadData()
    } catch (error) {
      toastFailure(toast, `Failed to ${schedule.enabled ? 'disable' : 'enable'} schedule`, error)
    }
  }

  const handleRunNow = async (schedule: Schedule) => {
    const ok = await confirm('Run Schedule Now', `Run "${schedule.name}" now?`, { variant: 'danger', confirmLabel: 'Run Now' })
    if (!ok) return

    try {
      await runScheduleNow(schedule.id)
      toast.success('Schedule executed successfully')
      setTimeout(loadData, 1000) // Refresh after execution
    } catch (error) {
      toastFailure(toast, 'Failed to run schedule', error)
    }
  }

  const getActionColor = (action: string): string => {
    switch (action) {
      case 'start': return 'text-emerald-700 bg-emerald-50 border border-emerald-200'
      case 'stop': return 'text-red-700 bg-red-50 border border-red-200'
      case 'restart': return 'text-amber-800 bg-amber-50 border border-amber-200'
      case 'snapshot': return 'text-[var(--zf-link)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)]'
      default: return 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border border-[var(--zf-hairline)]'
    }
  }

  const getScheduleTypeText = (schedule: Schedule): string => {
    if (schedule.schedule_type === 'once') {
      return `Once at ${schedule.time}`
    } else if (schedule.schedule_type === 'daily') {
      return `Daily at ${schedule.time}`
    } else if (schedule.schedule_type === 'weekly') {
      const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']
      const selectedDays = schedule.days_of_week?.map(d => days[d]).join(', ') || ''
      return `Weekly on ${selectedDays} at ${schedule.time}`
    }
    return 'Unknown'
  }

  if (loading) {
    return <div className="text-center py-8">Loading...</div>
  }

  return (
    <div>
      {loadError && (
        <ErrorBanner
          title="Could not load schedules"
          headline={loadError}
          hints={hintsForError(loadError)}
          onRetry={loadData}
        />
      )}
      <PageHeader
        title="VM Schedules"
        description="Automate VM lifecycle operations"
        actions={
          <>
            <button
              onClick={() => setShowHistory(!showHistory)}
              className={showHistory ? 'zf-btn zf-btn-primary' : 'zf-btn zf-btn-ghost'}
            >
              <Clock className="w-4 h-4" />
              History
            </button>
            <button
              onClick={() => setShowCreateDialog(true)}
              className="zf-btn zf-btn-primary"
            >
              <Plus className="w-4 h-4" />
              Create Schedule
            </button>
          </>
        }
      />

      {/* Schedules List */}
      {!showHistory && (
        <>
          {schedules.length === 0 ? (
            <div className="text-center py-12 bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
              <Calendar className="w-16 h-16 text-[var(--zf-muted)] mx-auto mb-4" />
              <p className="text-xl text-[var(--zf-muted)] mb-4">No schedules configured</p>
              <p className="text-[var(--zf-muted)] mb-6">Create schedules to automate VM operations</p>
              <button
                onClick={() => setShowCreateDialog(true)}
                className="zf-btn zf-btn-primary"
              >
                <Plus className="w-4 h-4" />
                Create First Schedule
              </button>
            </div>
          ) : (
            <div className="space-y-4">
              <TableSearch
                value={scheduleQuery}
                onChange={setScheduleQuery}
                placeholder="Search by name, VM, action..."
                resultCount={filteredSchedules.length}
                totalCount={schedules.length}
                className="max-w-sm"
              />
              {filteredSchedules.length === 0 && (
                <p className="text-sm text-[var(--zf-muted)] py-8 text-center">No schedules match "{scheduleQuery}"</p>
              )}
              {filteredSchedules.map((schedule) => (
                <div
                  key={schedule.id}
                  className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)] p-6"
                >
                  {/* Header */}
                  <div className="flex items-start justify-between mb-4">
                    <div className="flex-1">
                      <div className="flex items-center gap-3 mb-2">
                        <h3 className="text-lg font-bold">{schedule.name}</h3>
                        <span
                          className={`px-2 py-1 rounded text-xs font-medium border ${
                            schedule.enabled
                              ? 'text-emerald-700 bg-emerald-50 border-emerald-200'
                              : 'text-[var(--zf-muted)] bg-[var(--zf-canvas)] border-[var(--zf-hairline)]'
                          }`}
                        >
                          {schedule.enabled ? 'Enabled' : 'Disabled'}
                        </span>
                        <span className={`px-2 py-1 rounded text-xs font-medium ${getActionColor(schedule.action)}`}>
                          {schedule.action.toUpperCase()}
                        </span>
                      </div>
                      <p className="text-sm text-[var(--zf-muted)] mb-1">
                        VM: <span className="text-[var(--zf-ink)] font-medium">{schedule.vm_name}</span>
                      </p>
                      <p className="text-sm text-[var(--zf-muted)] mb-1">
                        Schedule: {getScheduleTypeText(schedule)}
                      </p>
                      {schedule.next_run && (
                        <p className="text-sm text-[var(--zf-link)]">
                          Next run: <RelativeTime date={schedule.next_run} />
                        </p>
                      )}
                      {schedule.last_run && (
                        <p className="text-xs text-[var(--zf-muted)]">
                          Last run: <RelativeTime date={schedule.last_run} />
                        </p>
                      )}
                    </div>

                    <div className="flex gap-2">
                      <button
                        onClick={() => handleRunNow(schedule)}
                        disabled={!schedule.enabled}
                        className="p-2 rounded-lg border border-[var(--zf-hairline)] text-[var(--zf-ink)] hover:bg-black/[0.04] transition disabled:opacity-50 disabled:cursor-not-allowed"
                        title="Run Now"
                      >
                        <Play className="w-4 h-4" />
                      </button>
                      <button
                        onClick={() => handleToggleEnabled(schedule)}
                        className={`p-2 rounded-lg border transition ${
                          schedule.enabled
                            ? 'text-amber-800 bg-amber-50 border-amber-200 hover:bg-amber-100'
                            : 'text-emerald-700 bg-emerald-50 border-emerald-200 hover:bg-emerald-100'
                        }`}
                        title={schedule.enabled ? 'Disable' : 'Enable'}
                      >
                        {schedule.enabled ? (
                          <PowerOff className="w-4 h-4" />
                        ) : (
                          <Power className="w-4 h-4" />
                        )}
                      </button>
                      <button
                        onClick={() => setEditingSchedule(schedule)}
                        className="p-2 rounded-lg border border-[var(--zf-hairline)] text-[var(--zf-link)] hover:bg-black/[0.04] transition"
                        title="Edit"
                      >
                        <Edit className="w-4 h-4" />
                      </button>
                      <button
                        onClick={() => handleDelete(schedule.id)}
                        className="p-2 rounded-lg border border-red-200 text-red-700 bg-red-50 hover:bg-red-100 transition"
                        title="Delete"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </>
      )}

      {/* History */}
      {showHistory && (
        <div className="bg-[var(--zf-canvas)] rounded-lg border border-[var(--zf-hairline)]">
          <div className="p-4 border-b border-[var(--zf-hairline)]">
            <h2 className="text-lg font-bold">Execution History</h2>
            <p className="text-sm text-[var(--zf-muted)]">Latest 20 executions</p>
          </div>
          <div className="overflow-x-auto">
            {history.length === 0 ? (
              <div className="text-center py-12">
                <p className="text-[var(--zf-muted)]">No execution history yet</p>
              </div>
            ) : (
              <table className="w-full">
                <thead className="bg-[var(--zf-surface)]">
                  <tr>
                    <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                      Schedule
                    </th>
                    <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                      VM
                    </th>
                    <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                      Action
                    </th>
                    <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                      Executed At
                    </th>
                    <th className="px-4 py-3 text-left text-xs font-medium text-[var(--zf-muted)] uppercase">
                      Status
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-[var(--zf-hairline)]">
                  {history.map((item, idx) => (
                    <tr key={idx} className="hover:bg-[var(--zf-surface)]">
                      <td className="px-4 py-3 text-sm">{item.schedule_name}</td>
                      <td className="px-4 py-3 text-sm font-medium">{item.vm_name}</td>
                      <td className="px-4 py-3">
                        <span className={`px-2 py-1 rounded text-xs font-medium ${getActionColor(item.action)}`}>
                          {item.action.toUpperCase()}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-sm text-[var(--zf-muted)]">
                        <RelativeTime date={item.executed_at} />
                      </td>
                      <td className="px-4 py-3">
                        <span
                          className={`px-2 py-1 rounded text-xs font-medium border ${
                            item.status === 'success'
                              ? 'text-emerald-700 bg-emerald-50 border-emerald-200'
                              : 'text-red-700 bg-red-50 border-red-200'
                          }`}
                        >
                          {item.status.toUpperCase()}
                        </span>
                        {item.error && (
                          <p className="text-xs text-red-600 mt-1">{item.error}</p>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </div>
      )}

      {showCreateDialog && (
        <ScheduleDialog
          mode="create"
          onClose={() => setShowCreateDialog(false)}
          onSuccess={loadData}
        />
      )}

      {editingSchedule && (
        <ScheduleDialog
          mode="edit"
          schedule={editingSchedule}
          onClose={() => setEditingSchedule(null)}
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
