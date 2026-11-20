// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { Link } from 'react-router'
import { Home, ArrowLeft } from 'lucide-react'

export default function NotFound() {
  return (
    <div className="flex flex-col items-center justify-center min-h-[60vh] text-center animate-fade-in px-4">
      <div className="text-[96px] font-semibold leading-none tracking-[-0.06em] text-[var(--zf-hairline)] select-none mb-4">
        404
      </div>
      <h2 className="text-[22px] font-semibold tracking-[-0.03em] text-[var(--zf-ink)] mb-2">
        Page not found
      </h2>
      <p className="text-[15px] text-[var(--zf-muted)] mb-8 max-w-sm">
        The page you&apos;re looking for doesn&apos;t exist or has moved.
      </p>
      <div className="flex gap-3">
        <button type="button" onClick={() => window.history.back()} className="zf-btn zf-btn-ghost">
          <ArrowLeft className="w-4 h-4" />
          Go back
        </button>
        <Link to="/app" className="zf-btn zf-btn-primary">
          <Home className="w-4 h-4" />
          Console
        </Link>
      </div>
    </div>
  )
}
