// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect } from 'react'
import { X, Calendar } from 'lucide-react'
import { createSchedule, updateSchedule, CreateScheduleRequest, Schedule } from '../api/schedule'
import { listVMs, VM } from '../api/vm'
import { useToastContext } from '../contexts/ToastContext'

type ScheduleDialogProps =
  | { mode: 'create'; schedule?: never; onClose: () => void; onSuccess: () => void }
  | { mode: 'edit'; schedule: Schedule; onClose: () => void; onSuccess: () => void }

const DAYS_OF_WEEK = [
  { value: 0, label: 'Sunday' },
  { value: 1, label: 'Monday' },
  { value: 2, label: 'Tuesday' },
  { value: 3, label: 'Wednesday' },
  { value: 4, label: 'Thursday' },
  { value: 5, label: 'Friday' },
  { value: 6, label: 'Saturday' },
]

export default function ScheduleDialog({ mode, schedule, onClose, onSuccess }: ScheduleDialogProps) {
  const toast = useToastContext()
  const [submitting, setSubmitting] = useState(false)
  const [vms, setVMs] = useState<VM[]>([])
  const [formData, setFormData] = useState<CreateScheduleRequest>({
    name: schedule?.name ?? '',
    vm_name: schedule?.vm_name ?? '',
    action: schedule?.action ?? 'start',
    schedule_type: schedule?.schedule_type ?? 'daily',
    time: schedule?.time ?? '09:00',
    days_of_week: schedule?.days_of_week ?? [],
    enabled: schedule?.enabled ?? true,
  })

  useEffect(() => {
    if (mode === 'create') {
      loadVMs()
    }
  }, [mode])

  const loadVMs = async () => {
    try {
      const data = await listVMs()
      setVMs(data)
      if (data.length > 0) {
        setFormData(prev => prev.vm_name ? prev : { ...prev, vm_name: data[0].name })
      }
    } catch (error) {
      console.error('Failed to load VMs:', error)
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()

    if (!formData.name.trim()) {
      toast.error('Schedule name is required')
      return
    }

    if (mode === 'create' && !formData.vm_name) {
      toast.error('Please select a VM')
      return
    }

    if (formData.schedule_type === 'weekly' && (!formData.days_of_week || formData.days_of_week.length === 0)) {
      toast.error('Please select at least one day for weekly schedule')
      return
    }

    setSubmitting(true)
    try {
      if (mode === 'create') {
        await createSchedule(formData)
        toast.success('Schedule created successfully')
      } else {
        await updateSchedule(schedule.id, {
          name: formData.name,
          action: formData.action,
          schedule_type: formData.schedule_type,
          time: formData.time,
          days_of_week: formData.days_of_week,
        })
        toast.success('Schedule updated successfully')
      }
      onSuccess()
      onClose()
    } catch (_error) {
      toast.error(mode === 'create' ? 'Failed to create schedule' : 'Failed to update schedule')
    } finally {
      setSubmitting(false)
    }
  }

  const toggleDay = (day: number) => {
    const days = formData.days_of_week || []
    if (days.includes(day)) {
      setFormData({
        ...formData,
        days_of_week: days.filter(d => d !== day)
      })
    } else {
      setFormData({
        ...formData,
        days_of_week: [...days, day].sort()
      })
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="bg-[var(--zf-canvas)] rounded-lg shadow-2xl border border-[var(--zf-hairline)] w-full max-w-2xl max-h-[90vh] overflow-y-auto">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-[var(--zf-hairline)] sticky top-0 bg-[var(--zf-surface)] z-10">
          <div className="flex items-center gap-3">
            <Calendar className="w-6 h-6 text-blue-500" />
            <div>
              <h2 className="text-xl font-bold">
                {mode === 'create' ? 'Create Schedule' : 'Edit Schedule'}
              </h2>
              {mode === 'edit' && (
                <p className="text-sm text-[var(--zf-muted)]">VM: {schedule.vm_name}</p>
              )}
            </div>
          </div>
          <button
            onClick={onClose}
            className="text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition"
          >
            <X className="w-6 h-6" />
          </button>
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} className="p-6 space-y-6">
          {/* Name */}
          <div>
            <label className="block text-sm font-medium mb-2">
              Schedule Name <span className="text-red-500">*</span>
            </label>
            <input
              type="text"
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              placeholder="e.g., Stop dev VMs at night"
              className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-2 text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:border-blue-500"
              required
            />
          </div>

          {/* VM Selection (create mode only) */}
          {mode === 'create' && (
            <div>
              <label className="block text-sm font-medium mb-2">
                Virtual Machine <span className="text-red-500">*</span>
              </label>
              <select
                value={formData.vm_name}
                onChange={(e) => setFormData({ ...formData, vm_name: e.target.value })}
                className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500"
                required
              >
                {vms.length === 0 && (
                  <option value="">Loading VMs...</option>
                )}
                {vms.map((vm) => (
                  <option key={vm.name} value={vm.name}>
                    {vm.name} ({vm.state})
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* Action */}
          <div>
            <label className="block text-sm font-medium mb-2">
              Action <span className="text-red-500">*</span>
            </label>
            <select
              value={formData.action}
              onChange={(e) => setFormData({ ...formData, action: e.target.value as CreateScheduleRequest['action'] })}
              className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500"
              required
            >
              <option value="start">Start VM</option>
              <option value="stop">Stop VM</option>
              <option value="restart">Restart VM</option>
              <option value="snapshot">Create Snapshot</option>
            </select>
          </div>

          {/* Schedule Type */}
          <div>
            <label className="block text-sm font-medium mb-2">
              Schedule Type <span className="text-red-500">*</span>
            </label>
            <select
              value={formData.schedule_type}
              onChange={(e) => {
                const newType = e.target.value as CreateScheduleRequest['schedule_type']
                setFormData({
                  ...formData,
                  schedule_type: newType,
                  days_of_week: newType === 'weekly' ? formData.days_of_week : [],
                })
              }}
              className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500"
              required
            >
              <option value="once">Once (run one time)</option>
              <option value="daily">Daily (every day)</option>
              <option value="weekly">Weekly (specific days)</option>
            </select>
          </div>

          {/* Days of Week (for weekly) */}
          {formData.schedule_type === 'weekly' && (
            <div>
              <label className="block text-sm font-medium mb-3">
                Days of Week <span className="text-red-500">*</span>
              </label>
              <div className="flex flex-wrap gap-2">
                {DAYS_OF_WEEK.map((day) => {
                  const isSelected = formData.days_of_week?.includes(day.value)
                  return (
                    <button
                      key={day.value}
                      type="button"
                      onClick={() => toggleDay(day.value)}
                      className={`px-4 py-2 rounded-lg text-sm font-medium transition ${
                        isSelected
                          ? 'bg-[var(--zf-link-fill)] text-white'
                          : 'bg-[var(--zf-surface)] text-[var(--zf-ink)] hover:bg-[var(--zf-hairline)]'
                      }`}
                    >
                      {day.label}
                    </button>
                  )
                })}
              </div>
            </div>
          )}

          {/* Time */}
          <div>
            <label className="block text-sm font-medium mb-2">
              Time (UTC) <span className="text-red-500">*</span>
            </label>
            <input
              type="time"
              value={formData.time}
              onChange={(e) => setFormData({ ...formData, time: e.target.value })}
              className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg px-4 py-2 text-[var(--zf-ink)] focus:outline-none focus:border-blue-500"
              required
            />
            <p className="text-xs text-[var(--zf-muted)] mt-1">
              24-hour format (HH:MM), UTC -- not your browser's local time
            </p>
          </div>

          {/* Current Status (edit mode only) */}
          {mode === 'edit' && (
            <div className="p-4 bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg">
              <h4 className="text-sm font-medium mb-2">Current Status</h4>
              <div className="grid grid-cols-2 gap-4 text-sm">
                <div>
                  <p className="text-[var(--zf-muted)]">Status</p>
                  <p className={schedule.enabled ? 'text-emerald-600' : 'text-[var(--zf-muted)]'}>
                    {schedule.enabled ? 'Enabled' : 'Disabled'}
                  </p>
                </div>
                {schedule.next_run && (
                  <div>
                    <p className="text-[var(--zf-muted)]">Next Run</p>
                    <p className="text-[var(--zf-link)]">{new Date(schedule.next_run).toLocaleString()}</p>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Enabled (create mode only) */}
          {mode === 'create' && (
            <div className="flex items-center gap-3">
              <input
                type="checkbox"
                id="enabled"
                checked={formData.enabled}
                onChange={(e) => setFormData({ ...formData, enabled: e.target.checked })}
                className="w-4 h-4 bg-[var(--zf-canvas)] border-[var(--zf-hairline)] rounded focus:ring-blue-500"
              />
              <label htmlFor="enabled" className="text-sm font-medium">
                Enable schedule immediately
              </label>
            </div>
          )}

          {/* Footer */}
          <div className="flex items-center justify-end gap-3 pt-4 border-t border-[var(--zf-hairline)]">
            <button
              type="button"
              onClick={onClose}
              disabled={submitting}
              className="px-4 py-2 bg-[var(--zf-surface)] hover:bg-[var(--zf-hairline)] rounded-lg transition disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="px-4 py-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg transition disabled:opacity-50"
            >
              {submitting
                ? (mode === 'create' ? 'Creating...' : 'Saving...')
                : (mode === 'create' ? 'Create Schedule' : 'Save Changes')
              }
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
