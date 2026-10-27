// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState, useCallback, useRef } from 'react'
import { useLocation, useNavigate } from 'react-router'
import { Search, ArrowRight, Server, Plus, Home, Terminal, RotateCw, Play, Square, Star, Clock, Pin, Keyboard, Info } from 'lucide-react'
import { listVMs, startVM, stopVM, VM } from '../api/vm'
import { useToastContext } from '../contexts/ToastContext'
import { useKeyboardShortcut } from '../hooks/useKeyboardShortcut'
import { navGroups, flattenNavGroup } from '../utils/routes'
import { toastFailure } from '../utils/toastError'
import { getPageLabel } from '../utils/pageLabels'
import { getPinnedPages, isPagePinned, togglePinnedPage } from '../utils/pinnedPages'
import { getRecentPages, recordRecentPage } from '../utils/recentPages'
import type { HelpTab } from './HelpDialog'

interface Command {
  id: string
  label: string
  description?: string
  action: () => void
  category: string
  keywords?: string[]
  icon?: React.ReactNode
}

interface CommandPaletteProps {
  onOpenHelp?: (tab?: HelpTab) => void
}

export default function CommandPalette({ onOpenHelp }: CommandPaletteProps) {
  const navigate = useNavigate()
  const location = useLocation()
  const toast = useToastContext()
  const [isOpen, setIsOpen] = useState(false)
  const [pinnedPages, setPinnedPages] = useState<string[]>(() => getPinnedPages())
  const [query, setQuery] = useState('')
  const [selectedIndex, setSelectedIndex] = useState(0)
  const [vms, setVMs] = useState<VM[]>([])
  const inputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)

  const loadVMs = useCallback(async () => {
    try {
      const data = await listVMs()
      setVMs(data)
    } catch (err) { toastFailure(toast, 'Failed to load VMs for command palette', err) }
  }, [toast])

  useEffect(() => {
    if (isOpen) {
      loadVMs()
      // Focus input after animation
      setTimeout(() => inputRef.current?.focus(), 50)
    }
  }, [isOpen, loadVMs])

  const close = useCallback(() => {
    setIsOpen(false)
    setQuery('')
    setSelectedIndex(0)
  }, [])

  const go = useCallback(
    (path: string) => {
      recordRecentPage(path)
      navigate(path)
      close()
    },
    [navigate, close],
  )

  const execute = (cmd: Command) => {
    cmd.action()
    close()
  }

  const vmAction = useCallback(
    async (name: string, fn: (n: string) => Promise<void>, label: string) => {
      close()
      try {
        await fn(name)
        toast.success(`${label} '${name}' OK`)
      } catch (e: unknown) {
        toastFailure(toast, `${label} '${name}' failed`, e)
      }
    },
    [close, toast],
  )

  useKeyboardShortcut({
    key: 'k',
    ctrl: true,
    handler: () => {
      setIsOpen((prev) => !prev)
      setQuery('')
      setSelectedIndex(0)
      setPinnedPages(getPinnedPages())
    },
  })

  const staticCommands: Command[] = []

  for (const path of pinnedPages) {
    staticCommands.push({
      id: `pin-page-${path}`,
      label: getPageLabel(path),
      icon: <Star className="w-4 h-4 text-amber-400" />,
      action: () => go(path),
      category: 'Pinned pages',
    })
  }

  for (const path of getRecentPages()) {
    if (pinnedPages.includes(path)) continue
    staticCommands.push({
      id: `recent-page-${path}`,
      label: getPageLabel(path),
      icon: <Clock className="w-4 h-4" />,
      action: () => go(path),
      category: 'Recent pages',
    })
  }

  staticCommands.push({
    id: 'pin-current-page',
    label: isPagePinned(location.pathname) ? 'Unpin current page' : 'Pin current page',
    icon: <Pin className="w-4 h-4" />,
    action: () => {
      setPinnedPages(togglePinnedPage(location.pathname))
      close()
    },
    category: 'Quick Actions',
  })

  if (onOpenHelp) {
    staticCommands.push(
      {
        id: 'help-shortcuts',
        label: 'Help: keyboard shortcuts',
        icon: <Keyboard className="w-4 h-4" />,
        action: () => { close(); onOpenHelp('shortcuts') },
        category: 'Help',
      },
      {
        id: 'help-about',
        label: 'Help: about Zyvor Fabric',
        icon: <Info className="w-4 h-4" />,
        action: () => { close(); onOpenHelp('about') },
        category: 'Help',
      },
    )
  }

  for (const group of navGroups) {
    for (const item of flattenNavGroup(group)) {
      const Icon = item.icon
      staticCommands.push({
        id: `nav-${item.path}`,
        label: item.label,
        description: group.name,
        icon: <Icon className="w-4 h-4" />,
        action: () => go(item.path),
        category: 'Pages',
        keywords: [group.compact.toLowerCase(), group.name.toLowerCase()],
      })
    }
  }

  staticCommands.push(
    { id: 'nav-dashboard', label: 'Dashboard', icon: <Home className="w-4 h-4" />, action: () => go('/app'), category: 'Quick Actions', keywords: ['home'] },
    { id: 'nav-create', label: 'Create VM', icon: <Plus className="w-4 h-4" />, action: () => go('/app/create'), category: 'Quick Actions', keywords: ['new'] },
    { id: 'action-refresh', label: 'Refresh Page', icon: <RotateCw className="w-4 h-4" />, action: () => window.location.reload(), category: 'Quick Actions', keywords: ['reload'] },
  )

  // Dynamic VM commands
  const vmCommands: Command[] = vms.flatMap((vm) => {
    const cmds: Command[] = [
      {
        id: `vm-view-${vm.name}`,
        label: `View ${vm.name}`,
        description: `${vm.state} - ${vm.cpus} vCPUs, ${vm.memory} MB`,
        icon: <Server className="w-4 h-4" />,
        action: () => go(`/app/vms/${vm.name}`),
        category: 'Virtual Machines',
        keywords: [vm.name, 'details'],
      },
      {
        id: `vm-console-${vm.name}`,
        label: `Console: ${vm.name}`,
        icon: <Terminal className="w-4 h-4" />,
        action: () => go(`/app/vms/${vm.name}/console`),
        category: 'Virtual Machines',
        keywords: [vm.name, 'terminal', 'shell'],
      },
    ]

    if (vm.state === 'stopped') {
      cmds.push({
        id: `vm-start-${vm.name}`,
        label: `Start ${vm.name}`,
        icon: <Play className="w-4 h-4" />,
        action: () => { void vmAction(vm.name, startVM, 'Start') },
        category: 'VM Actions',
        keywords: [vm.name, 'boot', 'power on'],
      })
    }

    if (vm.state === 'running') {
      cmds.push({
        id: `vm-stop-${vm.name}`,
        label: `Stop ${vm.name}`,
        icon: <Square className="w-4 h-4" />,
        action: () => { void vmAction(vm.name, stopVM, 'Stop') },
        category: 'VM Actions',
        keywords: [vm.name, 'shutdown', 'power off'],
      })
    }

    return cmds
  })

  const commands = [...staticCommands, ...vmCommands]

  const fuzzyMatch = (text: string, pattern: string): number => {
    const t = text.toLowerCase()
    const p = pattern.toLowerCase()
    if (t.includes(p)) return 100
    let score = 0
    let pi = 0
    let consecutive = 0
    for (let ti = 0; ti < t.length && pi < p.length; ti++) {
      if (t[ti] === p[pi]) {
        score += 10
        consecutive++
        score += consecutive * 5
        if (ti === 0 || t[ti - 1] === ' ' || t[ti - 1] === '-' || t[ti - 1] === '_') {
          score += 15
        }
        pi++
      } else {
        consecutive = 0
      }
    }
    return pi === p.length ? score : 0
  }

  const filteredCommands = query
    ? commands
        .map((cmd) => {
          const q = query.toLowerCase()
          const labelScore = fuzzyMatch(cmd.label, q)
          const keywordScore = Math.max(...(cmd.keywords?.map((k) => fuzzyMatch(k, q)) || [0]))
          const descScore = cmd.description ? fuzzyMatch(cmd.description, q) : 0
          const score = Math.max(labelScore, keywordScore, descScore)
          return { cmd, score }
        })
        .filter(({ score }) => score > 0)
        .sort((a, b) => b.score - a.score)
        .map(({ cmd }) => cmd)
    : commands

  const groupedCommands = filteredCommands.reduce((acc, cmd) => {
    if (!acc[cmd.category]) acc[cmd.category] = []
    acc[cmd.category].push(cmd)
    return acc
  }, {} as Record<string, Command[]>)

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        e.preventDefault()
        close()
        return
      }
      if (isOpen) {
        if (e.key === 'ArrowDown') {
          e.preventDefault()
          setSelectedIndex((prev) => Math.min(prev + 1, filteredCommands.length - 1))
        } else if (e.key === 'ArrowUp') {
          e.preventDefault()
          setSelectedIndex((prev) => Math.max(prev - 1, 0))
        } else if (e.key === 'Enter') {
          e.preventDefault()
          const cmd = filteredCommands[selectedIndex]
          if (cmd) execute(cmd)
        }
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen, filteredCommands, selectedIndex])

  useEffect(() => {
    setSelectedIndex(0)
  }, [query])

  // Scroll selected item into view
  useEffect(() => {
    if (!listRef.current) return
    const selected = listRef.current.querySelector('[data-selected="true"]')
    selected?.scrollIntoView({ block: 'nearest' })
  }, [selectedIndex])

  if (!isOpen) return null

  let cmdIndex = 0

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[15vh] bg-black/60 backdrop-blur-sm animate-fade-in"
      onClick={close}
    >
      <div
        className="bg-[var(--zf-canvas)] rounded-xl shadow-2xl border border-[var(--zf-hairline)] w-full max-w-xl max-h-[420px] overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Search */}
        <div className="flex items-center gap-3 px-4 py-3 border-b border-[var(--zf-hairline)]">
          <Search className="w-4 h-4 text-[var(--zf-muted)] shrink-0" />
          <input
            ref={inputRef}
            type="text"
            placeholder="Search commands, VMs, pages..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="flex-1 bg-transparent border-none outline-none text-sm text-[var(--zf-ink)] placeholder-[var(--zf-muted)]"
            autoFocus
          />
          <kbd className="px-1.5 py-0.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded text-[10px] text-[var(--zf-muted)] font-mono shrink-0">
            ESC
          </kbd>
        </div>

        {/* Results */}
        <div ref={listRef} className="overflow-y-auto max-h-[320px] sidebar-scroll">
          {filteredCommands.length === 0 ? (
            <div className="px-4 py-8 text-center">
              <p className="text-sm text-[var(--zf-muted)]">No results for "{query}"</p>
            </div>
          ) : (
            Object.entries(groupedCommands).map(([category, cmds]) => (
              <div key={category}>
                <div className="px-4 py-1.5 text-xs font-semibold text-[var(--zf-muted)] uppercase tracking-wider sticky top-0 bg-[var(--zf-surface)]">
                  {category}
                </div>
                {cmds.map((cmd) => {
                  const isSelected = cmdIndex === selectedIndex
                  const currentIndex = cmdIndex
                  cmdIndex++
                  return (
                    <button
                      key={cmd.id}
                      data-selected={isSelected}
                      onClick={() => execute(cmd)}
                      onMouseEnter={() => setSelectedIndex(currentIndex)}
                      className={`w-full flex items-center gap-3 px-4 py-2 text-left transition-colors ${
                        isSelected ? 'bg-[var(--zf-link)]/10 text-[var(--zf-ink)]' : 'text-[var(--zf-muted)] hover:text-[var(--zf-ink)]'
                      }`}
                    >
                      {cmd.icon && (
                        <span className={`shrink-0 ${isSelected ? 'text-[var(--zf-link)]' : 'text-[var(--zf-muted)]'}`}>
                          {cmd.icon}
                        </span>
                      )}
                      <div className="flex-1 min-w-0">
                        <div className="text-sm font-medium truncate">{cmd.label}</div>
                        {cmd.description && (
                          <div className="text-xs text-[var(--zf-muted)] truncate">{cmd.description}</div>
                        )}
                      </div>
                      {isSelected && <ArrowRight className="w-3.5 h-3.5 text-[var(--zf-link)] shrink-0" />}
                    </button>
                  )
                })}
              </div>
            ))
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center gap-4 px-4 py-2 border-t border-[var(--zf-hairline)] text-xs text-[var(--zf-muted)]">
          <span className="flex items-center gap-1">
            <kbd className="px-1 py-0.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded font-mono">↑↓</kbd>
            navigate
          </span>
          <span className="flex items-center gap-1">
            <kbd className="px-1 py-0.5 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] rounded font-mono">↵</kbd>
            select
          </span>
          <span className="ml-auto">{filteredCommands.length} results</span>
        </div>
      </div>
    </div>
  )
}
