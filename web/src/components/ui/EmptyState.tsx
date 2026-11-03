// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { ReactNode } from 'react'

interface EmptyStateProps {
  icon?: ReactNode
  /** A larger picture shown in place of the icon tile (see Illustration). Decorative: the title and text say the same thing. */
  illustration?: ReactNode
  title: string
  description?: string
  action?: ReactNode
}

export function EmptyState({ icon, illustration, title, description, action }: EmptyStateProps) {
  return (
    <div className="text-center py-20 px-4">
      {illustration ? (
        <div className="mb-4">{illustration}</div>
      ) : (
        <div className="icon-tile icon-tile-lg mx-auto mb-5 text-[var(--zf-muted)]">{icon}</div>
      )}
      <h3 className="text-[21px] font-semibold tracking-[-0.016em] text-[var(--zf-ink)] mb-1">
        {title}
      </h3>
      {description && (
        <p className="text-[17px] text-[var(--zf-secondary)] mb-6 max-w-md mx-auto tracking-[-0.022em]">
          {description}
        </p>
      )}
      {action}
    </div>
  )
}
