// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import ErrorBanner from './ErrorBanner'
import { hintsForError } from '../utils/daemonHints'

type PageLoadBannerProps = {
  title: string
  headline: string | null
  onRetry?: () => void
  domain?: 'vm' | 'storage' | 'network' | 'auth'
}

export default function PageLoadBanner({ title, headline, onRetry, domain }: PageLoadBannerProps) {
  if (!headline) return null
  return (
    <ErrorBanner
      title={title}
      headline={headline}
      hints={hintsForError(headline, domain)}
      onRetry={onRetry}
    />
  )
}
