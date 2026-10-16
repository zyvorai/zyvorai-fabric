// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState } from 'react'
import { Plus, Trash2 } from 'lucide-react'
import { saveDemo } from '../../api/agents'
import {
  draftToSpec,
  emptyDraft,
  MAX_RULES,
  parseSpecJson,
  RuleDraft,
  UseCaseDraft,
  UseCaseSpec,
  validateSpec,
} from '../../lib/useCaseSpec'

const RULE_LABELS: Record<RuleDraft['kind'], string> = {
  keyword_sections: 'Lines that mention…',
  top_repeated_lines: 'Most repeated lines',
  stats: 'Counts (lines, words)',
  csv_columns: 'CSV column summary',
}

const inputClass =
  'w-full rounded-md border border-[var(--zf-hairline)] bg-transparent px-2 py-1.5 text-sm'

interface Props {
  /** Called with the saved use case id so the picker can select it. */
  onDeployed: (id: string) => void
  onError: (message: string) => void
}

/**
 * "Deploy your own": a form (or pasted JSON) that becomes a declarative use case.
 * No code runs on the host: the extractor is an enum and the summary is a short
 * list of bounded rules. Anything that needs real code is a TypeScript agent pack.
 */
export default function DeployUseCase({ onDeployed, onError }: Props) {
  const [tab, setTab] = useState<'form' | 'json'>('form')
  const [draft, setDraft] = useState<UseCaseDraft>(emptyDraft)
  const [json, setJson] = useState('')
  const [busy, setBusy] = useState(false)

  const formSpec = useMemo(() => draftToSpec(draft), [draft])
  const jsonParsed = useMemo(() => (json.trim() ? parseSpecJson(json) : null), [json])
  const spec: UseCaseSpec | undefined = tab === 'form' ? formSpec : jsonParsed?.spec
  const problems =
    tab === 'form'
      ? validateSpec(formSpec)
      : jsonParsed?.error
        ? [jsonParsed.error]
        : spec
          ? validateSpec(spec)
          : ['Paste a use-case spec.']

  const setRule = (i: number, patch: Partial<RuleDraft>) =>
    setDraft((d) => ({ ...d, rules: d.rules.map((r, j) => (j === i ? { ...r, ...patch } : r)) }))

  const deploy = async () => {
    if (!spec || problems.length > 0) return
    setBusy(true)
    try {
      const out = await saveDemo(spec)
      onDeployed(out.id)
      if (tab === 'form') setDraft(emptyDraft())
      else setJson('')
    } catch (e) {
      onError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-4">
      <div className="flex gap-2" role="tablist" aria-label="How to define it">
        {(['form', 'json'] as const).map((t) => (
          <button
            key={t}
            type="button"
            role="tab"
            aria-selected={tab === t}
            className={tab === t ? 'zf-btn zf-btn-primary zf-btn-sm' : 'zf-btn zf-btn-secondary zf-btn-sm'}
            onClick={() => setTab(t)}
          >
            {t === 'form' ? 'Form' : 'Paste JSON'}
          </button>
        ))}
      </div>

      {tab === 'json' ? (
        <textarea
          className={`${inputClass} font-mono text-xs h-48`}
          aria-label="Use-case spec JSON"
          placeholder='{"id":"invoice-check","title":"Invoice check","accepts":["txt"],"extract":"text","summary":[{"kind":"keyword_sections","title":"Totals","keywords":["total"]}]}'
          value={json}
          onChange={(e) => setJson(e.target.value)}
        />
      ) : (
        <div className="space-y-3">
          <label className="block text-sm">
            <span className="text-[var(--zf-muted)]">Name</span>
            <input
              className={inputClass}
              value={draft.title}
              onChange={(e) => setDraft({ ...draft, title: e.target.value })}
              placeholder="Invoice check"
              maxLength={80}
            />
          </label>
          <label className="block text-sm">
            <span className="text-[var(--zf-muted)]">What it does (optional)</span>
            <input
              className={inputClass}
              value={draft.description}
              onChange={(e) => setDraft({ ...draft, description: e.target.value })}
              maxLength={240}
            />
          </label>
          <div className="grid grid-cols-2 gap-3">
            <label className="block text-sm">
              <span className="text-[var(--zf-muted)]">Reads</span>
              <select
                className={inputClass}
                value={draft.extract}
                onChange={(e) =>
                  setDraft({ ...draft, extract: e.target.value as UseCaseDraft['extract'] })
                }
              >
                <option value="text">Text files</option>
                <option value="pdftotext">PDF (text layer)</option>
                <option value="docx">Word (.docx)</option>
                <option value="xlsx">Excel (.xlsx, first sheet)</option>
                <option value="pptx">PowerPoint (.pptx, slides and notes)</option>
                <option value="ocr">Photo or screenshot (OCR, English)</option>
                <option value="html">HTML page</option>
                <option value="eml">Email (.eml / .mbox)</option>
              </select>
            </label>
            {draft.extract === 'text' && (
              <label className="block text-sm">
                <span className="text-[var(--zf-muted)]">File extensions</span>
                <input
                  className={inputClass}
                  value={draft.accepts}
                  onChange={(e) => setDraft({ ...draft, accepts: e.target.value })}
                  placeholder="txt, log, csv"
                />
              </label>
            )}
          </div>

          <div className="space-y-2">
            <p className="text-sm text-[var(--zf-muted)]">Summary rules</p>
            {draft.rules.map((r, i) => (
              <div key={i} className="rounded-md border border-[var(--zf-hairline)] p-3 space-y-2">
                <div className="flex items-center gap-2">
                  <select
                    className={inputClass}
                    aria-label={`Rule ${i + 1} type`}
                    value={r.kind}
                    onChange={(e) => setRule(i, { kind: e.target.value as RuleDraft['kind'] })}
                  >
                    {Object.entries(RULE_LABELS).map(([k, label]) => (
                      <option key={k} value={k}>
                        {label}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    className="zf-btn zf-btn-ghost zf-btn-sm"
                    aria-label={`Remove rule ${i + 1}`}
                    onClick={() =>
                      setDraft((d) => ({ ...d, rules: d.rules.filter((_, j) => j !== i) }))
                    }
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
                <input
                  className={inputClass}
                  aria-label={`Rule ${i + 1} title`}
                  placeholder={r.kind === 'stats' ? 'Title (optional)' : 'Section title'}
                  value={r.title}
                  onChange={(e) => setRule(i, { title: e.target.value })}
                />
                {(r.kind === 'keyword_sections' || r.kind === 'csv_columns') && (
                  <input
                    className={inputClass}
                    aria-label={`Rule ${i + 1} ${r.kind === 'csv_columns' ? 'columns' : 'keywords'}`}
                    placeholder={
                      r.kind === 'csv_columns' ? 'Column names, comma separated' : 'Keywords, comma separated'
                    }
                    value={r.items}
                    onChange={(e) => setRule(i, { items: e.target.value })}
                  />
                )}
                {r.kind !== 'stats' && (
                  <input
                    className={inputClass}
                    aria-label={`Rule ${i + 1} limit`}
                    placeholder="How many to show (optional)"
                    inputMode="numeric"
                    value={r.count}
                    onChange={(e) => setRule(i, { count: e.target.value })}
                  />
                )}
              </div>
            ))}
            <button
              type="button"
              className="zf-btn zf-btn-secondary zf-btn-sm"
              disabled={draft.rules.length >= MAX_RULES}
              onClick={() =>
                setDraft((d) => ({
                  ...d,
                  rules: [...d.rules, { kind: 'keyword_sections', title: '', items: '', count: '5' }],
                }))
              }
            >
              <Plus className="w-4 h-4 mr-1 inline" /> Add rule
            </button>
          </div>

          {draft.extract === 'text' && (
            <label className="block text-sm">
              <span className="text-[var(--zf-muted)]">Sample text (optional, lets you run it in one click)</span>
              <textarea
                className={`${inputClass} font-mono text-xs h-24`}
                value={draft.sampleText}
                onChange={(e) => setDraft({ ...draft, sampleText: e.target.value })}
              />
            </label>
          )}
        </div>
      )}

      {problems.length > 0 && (json.trim() || tab === 'form') && (
        <ul className="text-xs text-[var(--zf-muted)] list-disc pl-5">
          {problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}

      <button
        type="button"
        className="zf-btn zf-btn-primary"
        onClick={() => void deploy()}
        disabled={busy || problems.length > 0}
      >
        {busy ? 'Deploying…' : 'Deploy use case'}
      </button>
    </div>
  )
}
