// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { useEffect, type MouseEvent, type ReactNode } from 'react'

interface ModalProps {
  open: boolean
  onClose: () => void
  children: ReactNode
  /** Sizing/positioning classes for the card, e.g. 'max-w-md'. */
  className?: string
  /** Use the fully frosted Tier B treatment (single-instance chrome like
   *  CommandPalette/HelpDialog) instead of the default Tier A modal-card. */
  glass?: boolean
}

/** Shared modal shell -- backdrop + card + Esc-to-close + click-outside. */
export function Modal({ open, onClose, children, className = 'max-w-md', glass = false }: ModalProps) {
  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [open, onClose])

  if (!open) return null

  const handleBackdropClick = (e: MouseEvent<HTMLDivElement>) => {
    if (e.target === e.currentTarget) onClose()
  }

  return (
    <div className={glass ? 'glass-modal-backdrop' : 'modal-backdrop'} onClick={handleBackdropClick}>
      <div className={`${glass ? 'glass-modal-card' : 'modal-card'} w-full ${className}`} role="dialog" aria-modal="true">
        {children}
      </div>
    </div>
  )
}
