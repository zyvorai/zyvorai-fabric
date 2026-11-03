// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/**
 * A user-defined Merion use case, as agent-runtime stores it (`POST /v1/demos`).
 * It is data only: an extractor enum plus bounded rules. Mirrors
 * agent-runtime/src/demo_rules.rs; the server re-validates everything.
 */

export type Extractor = 'pdftotext' | 'text' | 'html' | 'eml' | 'docx' | 'xlsx' | 'pptx' | 'ocr'

/** Extractors that read one fixed kind of file, and the extensions they take. */
export const FIXED_ACCEPTS: Partial<Record<Extractor, string[]>> = {
  pdftotext: ['pdf'],
  html: ['html', 'htm'],
  eml: ['eml', 'mbox'],
  docx: ['docx'],
  xlsx: ['xlsx'],
  pptx: ['pptx'],
  ocr: ['png', 'jpg', 'jpeg', 'tif', 'tiff'],
}

export type Rule =
  | { kind: 'keyword_sections'; title: string; keywords: string[]; max_lines?: number }
  | { kind: 'top_repeated_lines'; title: string; top?: number }
  | { kind: 'stats'; title?: string }
  | { kind: 'csv_columns'; title: string; columns: string[]; top?: number }
  | { kind: 'regex_extract'; title: string; pattern: string; group?: number; max_matches?: number }
  | { kind: 'json_path'; title: string; paths: string[] }
  | { kind: 'table'; title: string; max_rows?: number }

/** Rule kinds the form builds. The others are written in the JSON editor. */
export type FormRuleKind = 'keyword_sections' | 'top_repeated_lines' | 'stats' | 'csv_columns'
export interface UseCaseSpec {
  id: string
  title: string
  description?: string
  accepts: string[]
  max_bytes?: number
  extract: Extractor
  summary: Rule[]
  artifact_title?: string
  sample?: { filename: string; text: string }
  /** Optional model step: the host sends the extracted text to one endpoint. JSON editor only. */
  model?: {
    credential: string
    base_url: string
    model: string
    instruction: string
    max_input_chars?: number
    max_output_tokens?: number
    fallback_chain?: string
  }
}

/** Editable form state; every field is a plain string so inputs stay simple. */
export interface RuleDraft {
  kind: FormRuleKind
  title: string
  /** Comma- or newline-separated: keywords for keyword_sections, columns for csv_columns. */
  items: string
  count: string
}

export interface UseCaseDraft {
  title: string
  description: string
  extract: Extractor
  /** Comma-separated extensions, e.g. "txt, log". */
  accepts: string
  rules: RuleDraft[]
  sampleText: string
}

export const MAX_RULES = 20

export function emptyDraft(): UseCaseDraft {
  return {
    title: '',
    description: '',
    extract: 'text',
    accepts: 'txt',
    rules: [{ kind: 'keyword_sections', title: 'Key lines', items: '', count: '5' }],
    sampleText: '',
  }
}

/** "Invoice check!" -> "invoice-check" (the id slug the server requires). */
export function slugify(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 40)
    .replace(/-+$/g, '')
}

const list = (s: string): string[] =>
  s
    .split(/[\n,]/)
    .map((x) => x.trim())
    .filter(Boolean)

const positive = (s: string): number | undefined => {
  const n = Number.parseInt(s, 10)
  return Number.isFinite(n) && n > 0 ? n : undefined
}

function toRule(d: RuleDraft): Rule {
  switch (d.kind) {
    case 'keyword_sections':
      return {
        kind: 'keyword_sections',
        title: d.title.trim(),
        keywords: list(d.items),
        ...(positive(d.count) ? { max_lines: positive(d.count) } : {}),
      }
    case 'top_repeated_lines':
      return {
        kind: 'top_repeated_lines',
        title: d.title.trim(),
        ...(positive(d.count) ? { top: positive(d.count) } : {}),
      }
    case 'stats':
      return { kind: 'stats', ...(d.title.trim() ? { title: d.title.trim() } : {}) }
    case 'csv_columns':
      return {
        kind: 'csv_columns',
        title: d.title.trim(),
        columns: list(d.items),
        ...(positive(d.count) ? { top: positive(d.count) } : {}),
      }
  }
}

/** Form -> spec. Empty sample text means no sample. */
export function draftToSpec(d: UseCaseDraft): UseCaseSpec {
  const accepts =
    FIXED_ACCEPTS[d.extract] ?? list(d.accepts).map((e) => e.replace(/^\./, '').toLowerCase())
  const spec: UseCaseSpec = {
    id: slugify(d.title),
    title: d.title.trim(),
    accepts,
    extract: d.extract,
    summary: d.rules.map(toRule),
  }
  if (d.description.trim()) spec.description = d.description.trim()
  if (d.extract === 'text' && d.sampleText.trim() && accepts.length > 0) {
    spec.sample = { filename: `sample.${accepts[0]}`, text: d.sampleText }
  }
  return spec
}

/** Cheap client-side checks so obvious mistakes never leave the form. */
export function validateSpec(spec: UseCaseSpec): string[] {
  const errs: string[] = []
  if (!spec.id) errs.push('Give the use case a name.')
  if (spec.accepts.length === 0) errs.push('List at least one file extension.')
  if (spec.accepts.some((e) => !/^[a-z0-9]{1,8}$/.test(e))) {
    errs.push('Extensions are lowercase letters and digits, e.g. txt, log, csv.')
  }
  if (spec.extract === 'text' && spec.accepts.includes('pdf')) {
    errs.push('PDF files need the PDF extractor.')
  }
  const fixed = FIXED_ACCEPTS[spec.extract]
  const wrong = fixed ? spec.accepts.find((e) => !fixed.includes(e)) : undefined
  if (fixed && wrong) errs.push(`The ${spec.extract} extractor reads only .${fixed.join(' / .')}, not .${wrong}.`)
  if (spec.summary.length === 0) errs.push('Add at least one summary rule.')
  if (spec.summary.length > MAX_RULES) errs.push(`At most ${MAX_RULES} rules.`)
  spec.summary.forEach((r, i) => {
    const n = i + 1
    if (r.kind === 'keyword_sections') {
      if (!r.title) errs.push(`Rule ${n}: add a title.`)
      if (r.keywords.length === 0) errs.push(`Rule ${n}: add at least one keyword.`)
    } else if (r.kind === 'top_repeated_lines' && !r.title) {
      errs.push(`Rule ${n}: add a title.`)
    } else if (r.kind === 'csv_columns') {
      if (!r.title) errs.push(`Rule ${n}: add a title.`)
      if (r.columns.length === 0) errs.push(`Rule ${n}: add at least one column name.`)
    }
  })
  return errs
}

/** Parse pasted JSON into a spec, or explain why not. */
export function parseSpecJson(text: string): { spec?: UseCaseSpec; error?: string } {
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch (e) {
    return { error: `Not valid JSON: ${e instanceof Error ? e.message : String(e)}` }
  }
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) {
    return { error: 'Expected a JSON object.' }
  }
  const o = raw as Partial<UseCaseSpec>
  if (typeof o.id !== 'string' || typeof o.title !== 'string') {
    return { error: 'The spec needs "id" and "title".' }
  }
  if (!Array.isArray(o.accepts) || !Array.isArray(o.summary) || !o.extract) {
    return { error: 'The spec needs "accepts", "extract" and "summary".' }
  }
  return { spec: o as UseCaseSpec }
}
