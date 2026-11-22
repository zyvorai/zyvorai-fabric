// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

/**
 * zyvor.dev top-left brand: open orange "Z" stroke + lowercase "zyvor" wordmark.
 * Accent matches --hs-accent-fill / #ff5a15.
 */

export const ZYVOR_ACCENT = '#ff5a15'

export default function ZyvorMark({
  className = 'w-6 h-6',
  title,
}: {
  className?: string
  title?: string
}) {
  return (
    <svg
      viewBox="0 0 18 18"
      className={className}
      aria-hidden={title ? undefined : true}
      role={title ? 'img' : undefined}
    >
      {title ? <title>{title}</title> : null}
      <path
        d="M2 2h14L6.6 16H16"
        fill="none"
        stroke={ZYVOR_ACCENT}
        strokeWidth="2.6"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  )
}

/** Full navbar lockup as on https://zyvor.dev — orange Z + "zyvor". */
export function ZyvorLockup({
  className = '',
  markClassName = 'w-6 h-6',
  wordmarkClassName = '',
  showWordmark = true,
}: {
  className?: string
  markClassName?: string
  wordmarkClassName?: string
  showWordmark?: boolean
}) {
  return (
    <span className={`inline-flex items-center gap-2 ${className}`.trim()}>
      <ZyvorMark className={markClassName} />
      {showWordmark ? (
        <span
          className={`font-semibold tracking-[-0.02em] text-[1.05em] leading-none text-[var(--zf-ink,#f5f5f7)] ${wordmarkClassName}`.trim()}
        >
          zyvor
        </span>
      ) : null}
    </span>
  )
}
