// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useCallback, useState } from 'react'
import { runPreflight } from '../api/preflight'
import type { PreflightResult } from '../lib/preflight'
import { formatUserError } from '../utils/apiError'
import { PreflightView } from './PreflightView'

/** Runs the read-only pre-flight check on demand for one VM. */
export default function PreflightPanel({ vmName }: { vmName: string }) {
  const [result, setResult] = useState<PreflightResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const run = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setResult(await runPreflight(vmName))
    } catch (err) {
      setError(formatUserError(err))
    } finally {
      setLoading(false)
    }
  }, [vmName])

  return <PreflightView result={result} loading={loading} error={error} onRun={() => void run()} />
}
