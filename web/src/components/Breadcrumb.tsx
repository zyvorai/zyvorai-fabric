// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { Link, useLocation } from 'react-router'
import { ChevronRight, Home } from 'lucide-react'
import { routeLabels } from '../utils/routes'

export default function Breadcrumb() {
  const { pathname } = useLocation()

  if (pathname === '/app' || pathname === '/app/') return null

  const segments = pathname.split('/').filter(Boolean)
  // Drop leading "app" from crumbs display — home is console dashboard
  const crumbs: { path: string; label: string }[] = []
  let cumulative = ''
  for (const seg of segments) {
    cumulative += `/${seg}`
    if (seg === 'app') continue
    const label = routeLabels[cumulative] || routeLabels[`/${seg}`] || decodeURIComponent(seg)
    crumbs.push({ path: cumulative, label })
  }

  if (crumbs.length === 0) return null

  return (
    <nav className="mb-6 flex items-center gap-1.5 text-sm flex-wrap">
      <Link
        to="/app"
        aria-label="Dashboard"
        className="text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition flex items-center gap-1"
      >
        <Home className="w-3.5 h-3.5" />
        <span className="hidden sm:inline">Dashboard</span>
      </Link>
      {crumbs.map((crumb, i) => {
        const isLast = i === crumbs.length - 1
        return (
          <span key={crumb.path} className="flex items-center gap-1.5">
            <ChevronRight className="w-3.5 h-3.5 text-[var(--zf-hairline)]" />
            {isLast ? (
              <span className="text-[var(--zf-ink)] font-medium">{crumb.label}</span>
            ) : (
              <Link to={crumb.path} className="text-[var(--zf-muted)] hover:text-[var(--zf-ink)] transition">
                {crumb.label}
              </Link>
            )}
          </span>
        )
      })}
    </nav>
  )
}
