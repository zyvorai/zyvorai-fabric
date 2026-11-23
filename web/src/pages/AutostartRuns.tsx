// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useEffect, useState } from 'react'
import { PageHeader } from '../components/ui'
import { PageLoading } from '../components/Skeleton'
import { AutostartRunsView } from '../components/AutostartRunsView'
import {
  getAutostartRun,
  listAutostartRuns,
  startAutostartRun,
  type AutostartRun,
  type AutostartRunSummary,
} from '../api/autostart'
import { useSilentPoll } from '../hooks/useSilentPoll'
import { useToastContext } from '../contexts/ToastContext'
import { formatUserError } from '../utils/apiError'
import { toastFailure } from '../utils/toastError'

export default function AutostartRuns() {
  const toast = useToastContext()
  const [runs, setRuns] = useState<AutostartRunSummary[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [selected, setSelected] = useState<AutostartRun | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      const list = await listAutostartRuns()
      setRuns(list.runs)
      setError(null)
      if (selectedId) setSelected(await getAutostartRun(selectedId))
    } catch (e) {
      setError(formatUserError(e))
    } finally {
      setLoading(false)
    }
  }, [selectedId])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const active = runs.some((r) => r.state === 'running')
  useSilentPoll(refresh, 2000, active)

  const runNow = async () => {
    try {
      const job = await startAutostartRun()
      toast.success('Autostart run started')
      setSelectedId(job.id)
    } catch (e) {
      toastFailure(toast, 'Failed to start autostart run', e)
    }
  }

  if (loading) return <PageLoading />
  return (
    <div>
      <PageHeader
        title="Autostart runs"
        description="Recent reconciliation passes that start VMs flagged to run."
        onRefresh={() => void refresh()}
        primaryAction={
          <button type="button" className="btn-primary" onClick={() => void runNow()}>
            Run now
          </button>
        }
      />
      {error && <p role="alert" className="mb-4 text-red-700">{error}</p>}
      <AutostartRunsView runs={runs} selected={selected} onSelect={setSelectedId} />
    </div>
  )
}
