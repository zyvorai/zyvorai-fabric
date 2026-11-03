// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

interface SkeletonProps {
  className?: string
  style?: React.CSSProperties
}

function SkeletonBase({ className = '', style }: SkeletonProps) {
  return <div className={`zf-skeleton ${className}`} style={style} />
}

export function SkeletonText({ className = '' }: SkeletonProps) {
  return <SkeletonBase className={`h-4 ${className}`} />
}

export function SkeletonCard() {
  return (
    <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
      <div className="p-5 space-y-3">
        <div className="flex items-start justify-between">
          <div className="space-y-2 flex-1">
            <SkeletonBase className="h-5 w-36" />
            <SkeletonBase className="h-3 w-24" />
          </div>
          <SkeletonBase className="h-5 w-16 rounded-full" />
        </div>
        <div className="flex gap-4">
          <SkeletonBase className="h-4 w-20" />
          <SkeletonBase className="h-4 w-20" />
        </div>
      </div>
      <div className="px-5 py-3 border-t border-[var(--zf-hairline)] flex gap-2">
        <SkeletonBase className="h-8 w-16 rounded-md" />
        <SkeletonBase className="h-8 w-16 rounded-md" />
      </div>
    </div>
  )
}

export function SkeletonTable({ rows = 5, cols = 4 }: { rows?: number; cols?: number }) {
  return (
    <div className="bg-[var(--zf-canvas)] rounded-xl border border-[var(--zf-hairline)] overflow-hidden">
      <div className="grid gap-4 p-4 border-b border-[var(--zf-hairline)]" style={{ gridTemplateColumns: `repeat(${cols}, 1fr)` }}>
        {Array.from({ length: cols }).map((_, i) => (
          <SkeletonBase key={i} className="h-3" />
        ))}
      </div>
      {Array.from({ length: rows }).map((_, row) => (
        <div
          key={row}
          className="grid gap-4 p-4 border-b border-[var(--zf-hairline)]/50 last:border-b-0"
          style={{ gridTemplateColumns: `repeat(${cols}, 1fr)` }}
        >
          {Array.from({ length: cols }).map((_, col) => (
            <SkeletonBase key={col} className="h-4" />
          ))}
        </div>
      ))}
    </div>
  )
}

const CHART_BARS = [45, 62, 38, 70, 55, 80, 48, 66, 35, 74]

export function SkeletonChart() {
  return (
    <div className="bg-[var(--zf-canvas)] rounded-xl p-5 border border-[var(--zf-hairline)]">
      <div className="flex items-center justify-between mb-4">
        <SkeletonBase className="h-4 w-28" />
        <SkeletonBase className="h-5 w-12" />
      </div>
      <div className="flex items-end gap-1.5 h-[180px]">
        {Array.from({ length: 20 }).map((_, i) => (
          <SkeletonBase
            key={i}
            className="flex-1 rounded-sm"
            style={{ height: `${CHART_BARS[i % CHART_BARS.length]}%` } as React.CSSProperties}
          />
        ))}
      </div>
    </div>
  )
}

export function SkeletonDashboard() {
  return (
    <div className="space-y-6">
      <div className="space-y-1">
        <SkeletonBase className="h-7 w-32" />
        <SkeletonBase className="h-4 w-56" />
      </div>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="bg-[var(--zf-canvas)] rounded-xl p-5 border border-[var(--zf-hairline)]">
            <SkeletonBase className="h-8 w-8 rounded-lg mb-3" />
            <SkeletonBase className="h-7 w-16 mb-1" />
            <SkeletonBase className="h-3 w-20" />
          </div>
        ))}
      </div>
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <SkeletonChart />
        <SkeletonChart />
      </div>
    </div>
  )
}

/** Announces a loading region to assistive tech while its children are decorative skeleton blocks. */
function LoadingRegion({ label, children, className = '' }: { label: string; children: React.ReactNode; className?: string }) {
  return (
    <div role="status" aria-busy="true" className={className}>
      <span className="sr-only">{label}</span>
      <div aria-hidden>{children}</div>
    </div>
  )
}

/** Stands in for a whole page while its data loads: the title block, a toolbar row, then the content shape
    (a table for list pages, cards for grids). Matches PageHeader's height so the page does not jump when it arrives. */
export function PageLoading({ variant = 'table' }: { variant?: 'table' | 'cards' }) {
  return (
    <LoadingRegion label="Loading" className="animate-fade-in">
      <div className="mb-6 sm:mb-8 space-y-3">
        <SkeletonBase className="h-8 sm:h-9 w-56" />
        <SkeletonBase className="h-4 w-80 max-w-full" />
      </div>
      <div className="flex gap-2 mb-5">
        <SkeletonBase className="h-9 w-28 rounded-full" />
        <SkeletonBase className="h-9 w-20 rounded-full" />
      </div>
      {variant === 'cards' ? (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {Array.from({ length: 6 }).map((_, i) => (
            <SkeletonCard key={i} />
          ))}
        </div>
      ) : (
        <SkeletonTable rows={6} cols={4} />
      )}
    </LoadingRegion>
  )
}

/** Stands in for a panel or tab body inside an already-rendered page. */
export function PanelLoading({ label = 'Loading' }: { label?: string }) {
  return (
    <LoadingRegion label={label}>
      <SkeletonTable rows={4} cols={3} />
    </LoadingRegion>
  )
}
