// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useEffect, useState } from 'react'
import { PageHeader } from '../components/ui'
import { PageLoading } from '../components/Skeleton'
import { SnapshotHygieneView } from '../components/HousekeepingViews'
import { getSnapshotHygiene, type HygieneReport } from '../api/housekeeping'
import { formatUserError } from '../utils/apiError'

export default function SnapshotHygiene() {
  const [report, setReport] = useState<HygieneReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [hours, setHours] = useState('72')

  const refresh = useCallback(async () => {
    try {
      const h = Number(hours)
      setReport(await getSnapshotHygiene(Number.isInteger(h) && h > 0 ? h : undefined))
      setError(null)
    } catch (e) {
      setError(formatUserError(e))
    } finally {
      setLoading(false)
    }
  }, [hours])

  useEffect(() => {
    void refresh()
    // Only the initial load; changing the threshold waits for Apply.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  if (loading) return <PageLoading />
  return (
    <div>
      <PageHeader
        title="Snapshot hygiene"
        description="Stale snapshots, deep chains and the most space that could be reclaimed. Snapshots grow with every write and are not backups; nothing is deleted from this page."
        onRefresh={() => void refresh()}
      />
      {error && <p role="alert" className="mb-4 text-red-700">{error}</p>}
      <div className="mb-4 flex items-center gap-3">
        <label htmlFor="hyg-hours">Stale after (hours)</label>
        <input
          id="hyg-hours"
          type="number"
          min={1}
          value={hours}
          onChange={(e) => setHours(e.target.value)}
        />
        <button type="button" className="btn-primary" onClick={() => void refresh()}>
          Apply
        </button>
      </div>
      {report && <SnapshotHygieneView report={report} />}
    </div>
  )
}
