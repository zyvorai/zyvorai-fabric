// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useEffect, useState } from 'react'
import {
  createConfigSnapshot,
  getConfigDiff,
  listConfigSnapshots,
  type ConfigDiff,
  type ConfigSnapshotMeta,
} from '../api/configSnapshots'
import { formatUserError } from '../utils/apiError'
import { SnapshotDiffView } from './SnapshotDiffView'

/** Snapshot history and diff, backed by /api/config/snapshots and /api/config/diff. */
export default function ConfigSnapshotsTab() {
  const [snapshots, setSnapshots] = useState<ConfigSnapshotMeta[]>([])
  const [max, setMax] = useState(50)
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('current')
  const [diff, setDiff] = useState<ConfigDiff | null>(null)
  const [diffLoading, setDiffLoading] = useState(false)
  const [creating, setCreating] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    try {
      const res = await listConfigSnapshots()
      setSnapshots(res.snapshots)
      setMax(res.max)
      // Default comparison: newest snapshot against the current configuration.
      setFrom((prev) => (prev && res.snapshots.some((s) => s.id === prev) ? prev : (res.snapshots[0]?.id ?? '')))
      setError(null)
    } catch (err) {
      setError(formatUserError(err))
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  useEffect(() => {
    if (!from || !to) {
      setDiff(null)
      return
    }
    let cancelled = false
    setDiffLoading(true)
    getConfigDiff(from, to)
      .then((d) => {
        if (!cancelled) {
          setDiff(d)
          setError(null)
        }
      })
      .catch((err) => {
        if (!cancelled) setError(formatUserError(err))
      })
      .finally(() => {
        if (!cancelled) setDiffLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [from, to])

  const onCreate = async () => {
    setCreating(true)
    try {
      const meta = await createConfigSnapshot()
      await load()
      setFrom(meta.id)
      setTo('current')
    } catch (err) {
      setError(formatUserError(err))
    } finally {
      setCreating(false)
    }
  }

  return (
    <SnapshotDiffView
      snapshots={snapshots}
      max={max}
      from={from}
      to={to}
      onFromChange={setFrom}
      onToChange={setTo}
      onCreate={() => void onCreate()}
      creating={creating}
      diff={diff}
      diffLoading={diffLoading}
      error={error}
    />
  )
}
