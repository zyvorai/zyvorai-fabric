// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useEffect, useState } from 'react'
import { PageHeader } from '../components/ui'
import { PageLoading } from '../components/Skeleton'
import { SnapshotHealthView } from '../components/SnapshotHealthView'
import { getSnapshotHealth, type SnapshotHealthReport } from '../api/snapshotHealth'
import { formatUserError } from '../utils/apiError'

export default function SnapshotHealth() {
  const [report, setReport] = useState<SnapshotHealthReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  const refresh = useCallback(async () => {
    try {
      setReport(await getSnapshotHealth())
      setError(null)
    } catch (e) {
      setError(formatUserError(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  if (loading) return <PageLoading />
  return (
    <div>
      <PageHeader
        title="Snapshot health"
        description="Per-VM recovery point status against each snapshot policy, broken chains and snapshot space forecast."
        onRefresh={() => void refresh()}
      />
      {error && <p role="alert" className="mb-4 text-red-700">{error}</p>}
      {report && <SnapshotHealthView report={report} />}
    </div>
  )
}
