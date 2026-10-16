// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useRef, useState } from 'react'
import { deployPackFile, MerionStatus } from '../../api/agents'
import { inspectPack, MerionPackSummary } from '../../lib/merionPack'

interface Props {
  status: MerionStatus | null
  onDeployed: (name: string) => void
  onError: (message: string) => void
}

/**
 * "Deploy a pack": upload a `.merionpack.json` made with `fabric-agent pack bundle`.
 * The author signs it on their own machine; the console only uploads the file,
 * so the signing seed never reaches the browser.
 */
export default function DeployPack({ status, onDeployed, onError }: Props) {
  const inputRef = useRef<HTMLInputElement>(null)
  const [text, setText] = useState<string | null>(null)
  const [fileName, setFileName] = useState('')
  const [summary, setSummary] = useState<MerionPackSummary | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const choose = async (file: File | undefined) => {
    setText(null)
    setSummary(null)
    setProblem(null)
    if (!file) return
    setFileName(file.name)
    const body = await file.text()
    const { summary: s, error } = inspectPack(body)
    if (error || !s) {
      setProblem(error ?? 'Could not read the pack.')
      return
    }
    if (status?.signature_required && !s.signed) {
      setProblem(
        'This runtime is in Keep mode and needs a signed pack. Run: MERION_POLICY_SEED=<seed> fabric-agent pack bundle <dir>',
      )
    }
    setText(body)
    setSummary(s)
  }

  const deploy = async () => {
    if (!text || !summary) return
    setBusy(true)
    try {
      const out = await deployPackFile(text)
      onDeployed(out.name)
      setText(null)
      setSummary(null)
      setFileName('')
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-3">
      <input
        ref={inputRef}
        type="file"
        accept=".json,application/json"
        className="hidden"
        onChange={(e) => void choose(e.target.files?.[0])}
      />
      <button
        type="button"
        className="zf-btn zf-btn-secondary zf-btn-sm"
        onClick={() => inputRef.current?.click()}
        disabled={busy}
      >
        {fileName || 'Choose a .merionpack.json'}
      </button>
      {summary && (
        <ul className="text-sm text-[var(--zf-muted)] list-disc pl-5">
          <li>
            Agent <code className="font-mono">{summary.name}</code>
          </li>
          <li>{summary.signed ? 'Signed by its author' : 'Not signed'}</li>
          <li>
            {summary.hasPolicy
              ? `Includes merion.policy.yaml${summary.policySigned ? ' (signed)' : ''}`
              : 'No separate policy'}
          </li>
        </ul>
      )}
      {problem && <p className="text-sm text-red-600 whitespace-pre-wrap">{problem}</p>}
      <button
        type="button"
        className="zf-btn zf-btn-primary"
        onClick={() => void deploy()}
        disabled={busy || !summary || Boolean(status?.signature_required && !summary.signed)}
      >
        {busy ? 'Deploying…' : 'Deploy pack'}
      </button>
    </div>
  )
}
