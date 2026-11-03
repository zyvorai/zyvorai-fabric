// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { ReactNode } from 'react'
import { Link } from 'react-router'
import { useAuth } from '../contexts/AuthContext'
import { ZyvorLockup } from './ZyvorMark'
import ThemeToggle from './ThemeToggle'

const LINKS = [
  { label: 'Product', to: '/product' },
  { label: 'Platform', to: '/platform' },
  { label: 'Merion', to: '/merion' },
  { label: 'Security', to: '/security' },
]

export default function MarketingLayout({ children }: { children: ReactNode }) {
  const { isAuthenticated } = useAuth()

  return (
    <div className="min-h-screen bg-[var(--zf-canvas)] text-[var(--zf-ink)]">
      <header className="mkt-nav">
        <div className="mkt-nav-inner">
          <nav className="flex items-center gap-7">
            <Link to="/" className="mkt-brand" aria-label="Zyvor Fabric">
              <ZyvorLockup markClassName="w-6 h-6" />
              <span className="text-[var(--zf-muted)] font-medium">Fabric</span>
            </Link>
            {LINKS.map((l) => (
              <Link key={l.to} to={l.to} className="hidden sm:inline">
                {l.label}
              </Link>
            ))}
          </nav>
          <div className="flex items-center gap-5">
            <ThemeToggle />
            {isAuthenticated ? (
              <Link to="/app" className="font-medium">
                Open console
              </Link>
            ) : (
              <Link to="/sign-in" className="font-medium">
                Sign in
              </Link>
            )}
          </div>
        </div>
      </header>
      {children}
      <footer className="mkt-footer">
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
          <span>Copyright © {new Date().getFullYear()} ZyvorAI Labs. All rights reserved.</span>
          <div className="flex gap-5">
            <Link to="/product">Product</Link>
            <Link to="/platform">Platform</Link>
            <Link to="/merion">Merion</Link>
            <Link to="/security">Security</Link>
            <a href="https://zyvor.dev" target="_blank" rel="noreferrer">
              zyvor.dev
            </a>
          </div>
        </div>
      </footer>
    </div>
  )
}
