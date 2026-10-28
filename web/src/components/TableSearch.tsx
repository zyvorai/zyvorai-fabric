// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { Search, X } from 'lucide-react'

interface TableSearchProps {
  value: string
  onChange: (value: string) => void
  placeholder?: string
  resultCount?: number
  totalCount?: number
  className?: string
}

/** Search box for table toolbars, matching VMList's original styling/behavior so every list page looks and feels the same. */
export default function TableSearch({ value, onChange, placeholder = 'Search...', resultCount, totalCount, className = '' }: TableSearchProps) {
  const showCount = resultCount !== undefined && totalCount !== undefined
  return (
    <div className={`flex items-center gap-3 ${className}`}>
      <div className="relative max-w-md flex-1">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[var(--zf-muted)]" />
        <input
          type="text"
          placeholder={placeholder}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="w-full bg-[var(--zf-canvas)] border border-[var(--zf-hairline)] rounded-lg py-2 pl-9 pr-8 text-sm text-[var(--zf-ink)] placeholder-[var(--zf-muted)] focus:outline-none focus:border-blue-500/50 focus:ring-1 focus:ring-blue-500/20 transition-colors"
        />
        {value && (
          <button onClick={() => onChange('')} className="absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition-colors" aria-label="Clear search">
            <X className="w-3.5 h-3.5" />
          </button>
        )}
      </div>
      {showCount && (
        <span className="text-xs text-[var(--zf-muted)] shrink-0">
          {resultCount === totalCount ? `${totalCount}` : `${resultCount} of ${totalCount}`}
        </span>
      )}
    </div>
  )
}
