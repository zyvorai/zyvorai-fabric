// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useEffect, useState } from 'react'
import { PageHeader } from '../components/ui'
import { PageLoading } from '../components/Skeleton'
import { ReclaimView } from '../components/HousekeepingViews'
import { getReclaimReport, type ReclaimReport } from '../api/housekeeping'
import { formatUserError } from '../utils/apiError'

export default function Reclaim() {
  const [report, setReport] = useState<ReclaimReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [days, setDays] = useState('30')

  const refresh = useCallback(async () => {
    try {
      const d = Number(days)
      setReport(await getReclaimReport(Number.isInteger(d) && d > 0 ? d : undefined))
      setError(null)
    } catch (e) {
      setError(formatUserError(e))
    } finally {
      setLoading(false)
    }
  }, [days])

  useEffect(() => {
    void refresh()
    // Only the initial load; changing the threshold waits for Apply.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  if (loading) return <PageLoading />
  return (
    <div>
      <PageHeader
        title="Reclaim"
        description="VMs that look unused (stopped or failed for a long time) or whose lease ran out, and stale snapshots. Idle time is measured from the VM record's last change. Nothing is deleted from this page."
        onRefresh={() => void refresh()}
      />
      {error && <p role="alert" className="mb-4 text-red-700">{error}</p>}
      <div className="mb-4 flex items-center gap-3">
        <label htmlFor="reclaim-days">Idle after (days)</label>
        <input
          id="reclaim-days"
          type="number"
          min={1}
          value={days}
          onChange={(e) => setDays(e.target.value)}
        />
        <button type="button" className="btn-primary" onClick={() => void refresh()}>
          Apply
        </button>
      </div>
      {report && <ReclaimView report={report} />}
    </div>
  )
}
