// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

export type IllustrationName = 'no-vms' | 'no-sessions' | 'all-clear' | 'sealed'

interface IllustrationProps {
  name: IllustrationName
  /** Pixel width; height follows the 160x120 viewBox. */
  size?: number
}

const INK = 'var(--zf-muted)'
const LINE = 'var(--zf-hairline)'

/** Spot illustrations for empty states and onboarding. Inline SVG on the --zf-* tokens, so they follow the theme.
    Decorative (aria-hidden): the title and text next to one say the same thing. */
export function Illustration({ name, size = 160 }: IllustrationProps) {
  return (
    <svg width={size} height={(size * 3) / 4} viewBox="0 0 160 120" fill="none" aria-hidden="true" className="mx-auto">
      <defs>
        <radialGradient id="zfIllGlow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--zf-brand)" stopOpacity="0.35" />
          <stop offset="100%" stopColor="var(--zf-brand)" stopOpacity="0" />
        </radialGradient>
      </defs>
      <ellipse cx="80" cy="60" rx="70" ry="48" fill="url(#zfIllGlow)" />
      {name === 'no-vms' && (
        <g>
          <rect x="38" y="30" width="84" height="22" rx="6" stroke={INK} strokeWidth="1.5" strokeDasharray="4 4" />
          <rect x="38" y="58" width="84" height="22" rx="6" stroke={INK} strokeWidth="1.5" strokeDasharray="4 4" />
          <circle cx="50" cy="41" r="3" fill={LINE} />
          <circle cx="50" cy="69" r="3" fill={LINE} />
          <path d="M80 92v12M74 98h12" stroke="var(--zf-brand)" strokeWidth="2.5" strokeLinecap="round" />
        </g>
      )}
      {name === 'no-sessions' && (
        <g>
          <rect x="40" y="26" width="80" height="60" rx="8" stroke={INK} strokeWidth="1.5" />
          <path d="M52 44h40M52 56h56M52 68h28" stroke={LINE} strokeWidth="3" strokeLinecap="round" />
          <circle cx="112" cy="88" r="14" fill="var(--zf-surface)" stroke="var(--zf-brand)" strokeWidth="2" />
          <path d="M107 88h10M112 83v10" stroke="var(--zf-brand)" strokeWidth="2" strokeLinecap="round" />
        </g>
      )}
      {name === 'all-clear' && (
        <g>
          <path d="M80 24l34 12v26c0 20-14 34-34 42-20-8-34-22-34-42V36z" fill="var(--zf-surface)" stroke="var(--zf-proof)" strokeWidth="2" strokeLinejoin="round" />
          <path d="M66 62l10 10 20-22" stroke="var(--zf-proof)" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round" className="zf-draw" style={{ ['--zf-len' as string]: 60 }} />
        </g>
      )}
      {name === 'sealed' && (
        <g>
          <circle cx="80" cy="62" r="34" stroke={LINE} strokeWidth="1.5" />
          <circle cx="80" cy="62" r="24" stroke="var(--zf-proof)" strokeWidth="2" strokeDasharray="3 5" className="zf-flow" />
          <rect x="68" y="56" width="24" height="18" rx="4" fill="var(--zf-surface)" stroke="var(--zf-proof)" strokeWidth="2" />
          <path d="M73 56v-5a7 7 0 0114 0v5" stroke="var(--zf-proof)" strokeWidth="2" strokeLinecap="round" />
        </g>
      )}
    </svg>
  )
}
