// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState, useCallback } from 'react'
import { useNavigate } from 'react-router'
import { X, Keyboard } from 'lucide-react'

interface Shortcut {
  keys: string[]
  description: string
  category: string
}

const shortcuts: Shortcut[] = [
  { keys: ['g', 'd'], description: 'Go to Dashboard', category: 'Navigation' },
  { keys: ['g', 'v'], description: 'Go to VMs', category: 'Navigation' },
  { keys: ['g', 'l'], description: 'Go to Logs', category: 'Navigation' },
  { keys: ['g', 'n'], description: 'Go to Network', category: 'Navigation' },
  { keys: ['g', 's'], description: 'Go to Storage', category: 'Navigation' },
  { keys: ['g', 'c'], description: 'Create new VM', category: 'Navigation' },
  { keys: ['/'], description: 'Focus search input', category: 'Search' },
  { keys: ['Esc'], description: 'Clear search / Close dialogs', category: 'Search' },
  { keys: ['r'], description: 'Refresh current page', category: 'Actions' },
  { keys: ['?'], description: 'Show/hide this help', category: 'Actions' },
  { keys: ['Ctrl+K'], description: 'Open command palette', category: 'Actions' },
]

export default function KeyboardShortcutsPanel() {
  const navigate = useNavigate()
  const [isOpen, setIsOpen] = useState(false)
  const [, setPressedKeys] = useState<string[]>([])

  const handleNavigation = useCallback(
    (path: string) => navigate(path),
    [navigate]
  )

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return

      if (e.key === '?' && !e.ctrlKey && !e.metaKey) {
        e.preventDefault()
        setIsOpen((prev) => !prev)
        return
      }

      if (e.key === 'Escape' && isOpen) {
        e.preventDefault()
        setIsOpen(false)
        return
      }

      setPressedKeys((prev) => {
        const newKeys = [...prev, e.key].slice(-2)
        if (newKeys.length === 2 && newKeys[0] === 'g') {
          const routes: Record<string, string> = { d: '/', v: '/vms', l: '/logs', n: '/network', s: '/storage', c: '/create' }
          if (routes[newKeys[1]]) { handleNavigation(routes[newKeys[1]]); return [] }
        }
        if (e.key === '/' && !e.ctrlKey && !e.metaKey) {
          e.preventDefault()
          const input = document.querySelector('input[type="text"]') as HTMLInputElement
          input?.focus()
          return []
        }
        setTimeout(() => setPressedKeys([]), 1000)
        return newKeys
      })
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [isOpen, handleNavigation])

  if (!isOpen) return null

  const categories = Array.from(new Set(shortcuts.map((s) => s.category)))

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in"
      onClick={() => setIsOpen(false)}
    >
      <div
        className="bg-[var(--zf-canvas)] rounded-xl shadow-2xl border border-[var(--zf-hairline)] w-full max-w-lg overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-[var(--zf-hairline)]">
          <div className="flex items-center gap-2.5">
            <div className="p-1.5 rounded-lg bg-blue-500/10">
              <Keyboard className="w-4 h-4 text-[var(--zf-link)]" />
            </div>
            <h2 className="text-base font-semibold text-[var(--zf-ink)]">Keyboard Shortcuts</h2>
          </div>
          <button onClick={() => setIsOpen(false)} className="p-1.5 rounded-md text-[var(--zf-muted)] hover:text-[var(--zf-ink)] hover:bg-white/5 transition-colors">
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="px-5 py-4 overflow-y-auto max-h-[60vh] sidebar-scroll">
          {categories.map((category) => (
            <div key={category} className="mb-5 last:mb-0">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-[var(--zf-muted)] mb-2">{category}</h3>
              <div className="space-y-1">
                {shortcuts
                  .filter((s) => s.category === category)
                  .map((shortcut, index) => (
                    <div key={index} className="flex items-center justify-between py-2 px-3 rounded-lg hover:bg-white/[0.02] transition-colors">
                      <span className="text-sm text-[var(--zf-muted)]">{shortcut.description}</span>
                      <div className="flex items-center gap-1.5">
                        {shortcut.keys.map((key, ki) => (
                          <span key={ki} className="flex items-center gap-1">
                            <kbd className="px-2 py-0.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded text-xs font-mono text-[var(--zf-ink)] min-w-[24px] text-center">
                              {key}
                            </kbd>
                            {ki < shortcut.keys.length - 1 && (
                              <span className="text-xs text-[var(--zf-muted)]">then</span>
                            )}
                          </span>
                        ))}
                      </div>
                    </div>
                  ))}
              </div>
            </div>
          ))}
        </div>

        <div className="px-5 py-3 border-t border-[var(--zf-hairline)] text-center text-xs text-[var(--zf-muted)]">
          Press <kbd className="px-1.5 py-0.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded font-mono mx-0.5">?</kbd> to toggle
        </div>
      </div>
    </div>
  )
}
