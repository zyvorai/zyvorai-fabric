// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { ReactNode, useMemo, useState } from 'react'
import { Link, NavLink, useNavigate } from 'react-router'
import { LogOut, Menu, PanelLeftClose, PanelLeftOpen, Search, X } from 'lucide-react'
import { useAuth } from '../contexts/AuthContext'
import { NAV_GROUPS, flattenNavGroup } from '../navigation/navConfig'
import ConnectionStatus from './ConnectionStatus'
import CommandPalette from './CommandPalette'
import Breadcrumb from './Breadcrumb'
import HelpDialog, { type HelpTab } from './HelpDialog'
import { useSequenceShortcuts } from '../hooks/useSequenceShortcut'
import { ZyvorLockup } from './ZyvorMark'
import ThemeToggle from './ThemeToggle'
import { useKeyboardShortcut, isInputFocused } from '../hooks/useKeyboardShortcut'
import { useRecordRecentPage } from '../hooks/useRecordRecentPage'

function ConsoleShortcuts({
  helpOpen,
  helpTab,
  onOpenHelp,
  onCloseHelp,
  onHelpTabChange,
}: {
  helpOpen: boolean
  helpTab: HelpTab
  onOpenHelp: (tab?: HelpTab) => void
  onCloseHelp: () => void
  onHelpTabChange: (tab: HelpTab) => void
}) {
  const navigate = useNavigate()
  const shortcuts = useMemo(
    () => [
      { sequence: ['g', 'd'] as [string, string], handler: () => navigate('/app') },
      { sequence: ['g', 'v'] as [string, string], handler: () => navigate('/app/vms') },
      { sequence: ['g', 'n'] as [string, string], handler: () => navigate('/app/network') },
      { sequence: ['g', 's'] as [string, string], handler: () => navigate('/app/storage') },
      { sequence: ['g', 'c'] as [string, string], handler: () => navigate('/app/create') },
      { sequence: ['g', 'l'] as [string, string], handler: () => navigate('/app/logs') },
      { sequence: ['g', 'b'] as [string, string], handler: () => navigate('/app/backups') },
      { sequence: ['g', 'i'] as [string, string], handler: () => navigate('/app/disk-images') },
      { sequence: ['g', 'e'] as [string, string], handler: () => navigate('/app/live-metrics') },
    ],
    [navigate],
  )
  useSequenceShortcuts(shortcuts)

  useKeyboardShortcut({
    key: '?',
    handler: (e) => {
      if (isInputFocused()) return
      e.preventDefault()
      if (helpOpen) onCloseHelp()
      else onOpenHelp('shortcuts')
    },
  })

  return (
    <HelpDialog open={helpOpen} tab={helpTab} onClose={onCloseHelp} onTabChange={onHelpTabChange} />
  )
}

export default function ConsoleLayout({ children }: { children: ReactNode }) {
  const { user, logout } = useAuth()
  const navigate = useNavigate()
  const [mobileNav, setMobileNav] = useState(false)
  const [helpOpen, setHelpOpen] = useState(false)
  const [helpTab, setHelpTab] = useState<HelpTab>('shortcuts')
  // Per-viewer preference, not app state -- localStorage rather than a
  // backend setting. Defaults expanded; a bad/blocked accessor (private
  // window, cleared storage) just falls back to that default.
  const [collapsed, setCollapsed] = useState(() => {
    try {
      return localStorage.getItem('zf-sidebar-collapsed') === '1'
    } catch {
      return false
    }
  })
  const toggleCollapsed = () => {
    setCollapsed((prev) => {
      const next = !prev
      try {
        localStorage.setItem('zf-sidebar-collapsed', next ? '1' : '0')
      } catch {
        // ignore -- private window / storage blocked
      }
      return next
    })
  }
  useRecordRecentPage()

  const onLogout = () => {
    logout()
    navigate('/')
  }

  return (
    <div className="console-shell">
      <a href="#main-content" className="zf-skip-link">
        Skip to content
      </a>
      <ConsoleShortcuts
        helpOpen={helpOpen}
        helpTab={helpTab}
        onOpenHelp={(tab = 'shortcuts') => {
          setHelpTab(tab)
          setHelpOpen(true)
        }}
        onCloseHelp={() => setHelpOpen(false)}
        onHelpTabChange={setHelpTab}
      />
      <CommandPalette onOpenHelp={(tab) => { setHelpTab(tab ?? 'shortcuts'); setHelpOpen(true) }} />

      <header className="console-topbar">
        <button
          type="button"
          className="lg:hidden zf-btn zf-btn-ghost zf-btn-sm !px-2"
          onClick={() => setMobileNav((v) => !v)}
          aria-label="Toggle navigation"
        >
          {mobileNav ? <X className="w-4 h-4" /> : <Menu className="w-4 h-4" />}
        </button>
        <Link to="/app" className="console-brand" aria-label="Zyvor Fabric">
          <ZyvorLockup markClassName="w-6 h-6" />
          <span className="text-[var(--zf-muted)] font-medium">Fabric</span>
        </Link>
        <div className="flex-1" />
        <button
          type="button"
          className="zf-btn zf-btn-ghost zf-btn-sm console-topbar-search"
          onClick={() =>
            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', metaKey: true }))
          }
        >
          <Search className="w-3.5 h-3.5" />
          Search
          <kbd className="text-[10px] text-[var(--zf-muted)] ml-1">⌘K</kbd>
        </button>
        <ConnectionStatus />
        <ThemeToggle />
        <Link to="/" className="text-xs text-[var(--zf-muted)] hidden md:inline hover:text-[var(--zf-ink)]">
          Site
        </Link>
        <span className="text-xs text-[var(--zf-muted)] hidden sm:inline">{user?.username}</span>
        <button type="button" className="zf-btn zf-btn-ghost zf-btn-sm !px-2" onClick={onLogout} title="Sign out">
          <LogOut className="w-3.5 h-3.5" />
        </button>
      </header>

      <div className="console-body">
        <aside
          className={`console-sidebar ${collapsed ? 'collapsed' : ''} ${mobileNav ? '!block fixed inset-x-0 top-[52px] z-20 h-[calc(100vh-52px)]' : ''}`}
        >
          <button
            type="button"
            onClick={toggleCollapsed}
            title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            className="console-sidebar-collapse-btn hidden lg:flex"
          >
            {collapsed ? <PanelLeftOpen className="w-3.5 h-3.5" /> : <PanelLeftClose className="w-3.5 h-3.5" />}
            {!collapsed && <span>Collapse</span>}
          </button>
          {NAV_GROUPS.map((group) => {
            const items = flattenNavGroup(group)
            return (
              <div key={group.name} className="console-sidebar-group">
                {!collapsed && <div className="console-sidebar-label">{group.compact}</div>}
                {items.map((item) => {
                  const Icon = item.icon
                  return (
                    <NavLink
                      key={item.path}
                      to={item.path}
                      end={item.path === '/app'}
                      title={collapsed ? item.label : undefined}
                      className={({ isActive }) =>
                        `console-nav-link${isActive ? ' active' : ''}`
                      }
                      onClick={() => setMobileNav(false)}
                    >
                      <Icon className="w-3.5 h-3.5 shrink-0" />
                      {!collapsed && <span className="truncate">{item.label}</span>}
                    </NavLink>
                  )
                })}
              </div>
            )
          })}
        </aside>

        <main id="main-content" className="console-main" role="main">
          <Breadcrumb />
          {children}
        </main>
      </div>
    </div>
  )
}
