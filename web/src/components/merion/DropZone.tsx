// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react'
import { Upload } from 'lucide-react'

interface DropZoneProps {
  /** What is chosen now: shown instead of the prompt. */
  chosen: string | null
  /** The prompt when nothing is chosen. */
  prompt: string
  hint?: string
  disabled?: boolean
  onFiles: (files: File[]) => void
  /** Open the file picker (the page's own hidden input). */
  onBrowse: () => void
}

/** A file drop target that is also a button: a click or Enter opens the picker, a dropped file is handed to the page. The
    picker stays the keyboard route, so nothing needs a mouse. */
export function DropZone({ chosen, prompt, hint, disabled = false, onFiles, onBrowse }: DropZoneProps) {
  const [over, setOver] = useState(false)
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onBrowse}
      onDragOver={(e) => {
        if (disabled) return
        e.preventDefault()
        setOver(true)
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault()
        setOver(false)
        if (disabled) return
        const files = Array.from(e.dataTransfer.files)
        if (files.length > 0) onFiles(files)
      }}
      data-over={over}
      className="flex w-full flex-col items-center gap-1 rounded-[var(--zf-radius-lg)] border-2 border-dashed border-[var(--zf-hairline)] px-4 py-6 text-center transition-colors hover:border-[var(--zf-brand)] data-[over=true]:border-[var(--zf-brand)] data-[over=true]:bg-[var(--zf-hover-tint)] disabled:opacity-60"
    >
      <Upload className="h-6 w-6 text-[var(--zf-brand-text)]" aria-hidden="true" />
      <span className="text-sm font-medium text-[var(--zf-ink)]">{chosen ?? prompt}</span>
      {hint && <span className="text-xs text-[var(--zf-muted)]">{hint}</span>}
    </button>
  )
}
