// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import React from 'react'
import { AlertTriangle } from 'lucide-react'

interface ErrorBoundaryProps {
  children: React.ReactNode
  fallback?: React.ReactNode
}

interface ErrorBoundaryState {
  hasError: boolean
  error: Error | null
}

export class ErrorBoundary extends React.Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props)
    this.state = { hasError: false, error: null }
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('ErrorBoundary caught an error:', error, errorInfo)
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback
      }

      return (
        <div className="flex flex-col items-center justify-center min-h-[400px] p-8">
          <div className="p-3 rounded-xl bg-red-500/10 mb-4">
            <AlertTriangle className="w-8 h-8 text-red-600" />
          </div>
          <h2 className="text-lg font-semibold text-[var(--zf-ink)] mb-1">Something went wrong</h2>
          <p className="text-sm text-[var(--zf-muted)] mb-6 text-center max-w-md">
            {this.state.error?.message || 'An unexpected error occurred.'}
          </p>
          <button
            onClick={() => window.location.reload()}
            className="px-4 py-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg transition-colors text-sm font-medium text-white"
          >
            Reload Page
          </button>
        </div>
      )
    }

    return this.props.children
  }
}

export function PageErrorBoundary({ children }: { children: React.ReactNode }) {
  const [key, setKey] = React.useState(0)

  return (
    <ErrorBoundary
      key={key}
      fallback={
        <div className="flex flex-col items-center justify-center p-12">
          <div className="p-2.5 rounded-xl bg-yellow-500/10 mb-3">
            <AlertTriangle className="w-6 h-6 text-amber-600" />
          </div>
          <h3 className="text-base font-semibold text-[var(--zf-ink)] mb-1">Section error</h3>
          <p className="text-sm text-[var(--zf-muted)] mb-5">This section encountered an error</p>
          <div className="flex gap-2">
            <button
              onClick={() => setKey((k) => k + 1)}
              className="px-4 py-2 bg-[var(--zf-link-fill)] hover:bg-[var(--zf-link-fill-hover)] rounded-lg transition-colors text-sm font-medium"
            >
              Try Again
            </button>
            <button
              onClick={() => window.location.reload()}
              className="px-4 py-2 bg-[var(--zf-surface)] border border-[var(--zf-hairline)] hover:border-[var(--zf-hairline)] rounded-lg transition-colors text-sm text-[var(--zf-ink)]"
            >
              Reload Page
            </button>
          </div>
        </div>
      }
    >
      {children}
    </ErrorBoundary>
  )
}
