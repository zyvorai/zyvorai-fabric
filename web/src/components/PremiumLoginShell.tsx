// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/**
 * Apple Store chapter login — full-bleed white hero (brand → title → lede → pill CTAs),
 * parchment second chapter for credentials. Mirrors hyper2kvm / apple.com store pattern.
 */
import type { ReactNode } from 'react'
import { AlertCircle } from 'lucide-react'
import '../zyvor-premium-login.css'

export type PremiumLoginTone = 'sky' | 'violet' | 'emerald' | 'amber' | 'orange'

export type PremiumLoginPill = {
  icon?: ReactNode
  label: string
  tone?: PremiumLoginTone
}

export type PremiumLoginShellProps = {
  logo?: ReactNode
  productName: string
  productWordmark?: string
  productSubtitle?: string
  heroTitle?: ReactNode
  heroSubheadline?: ReactNode
  heroCta?: ReactNode
  accent?: PremiumLoginTone
  chapterNote?: ReactNode
  pills?: PremiumLoginPill[]
  /** Extra identity strip under the wordmark (host, k8s, version). */
  instanceMeta?: ReactNode
  panelTitle?: string
  panelSubtitle?: ReactNode
  panelHint?: ReactNode
  footer?: ReactNode
  formClassName?: string
  className?: string
  showSignInChapter?: boolean
  children?: ReactNode
}

export function PremiumLoginShell({
  logo,
  productName,
  productWordmark,
  productSubtitle,
  heroTitle = 'Private cloud control plane.',
  heroSubheadline,
  heroCta,
  accent = 'sky',
  chapterNote = 'Zyvor Fabric · sign in to continue',
  pills,
  instanceMeta,
  panelTitle,
  panelSubtitle,
  panelHint,
  footer,
  formClassName = '',
  className = '',
  showSignInChapter = true,
  children,
}: PremiumLoginShellProps) {
  const tagline =
    heroSubheadline ??
    productSubtitle ??
    'VMs, networking, and storage — driven by FluxVM. Scroll to sign in.'
  const formHeading =
    panelSubtitle ?? (panelTitle && panelTitle !== 'Sign in' ? panelTitle : 'Sign in')
  const wordmark =
    productWordmark !== undefined ? productWordmark.trim() : (productName || 'Zyvor Fabric').trim()
  const pageClass = [
    'login-page',
    'login-store-page',
    'min-h-screen',
    'flex',
    'flex-col',
    accent === 'orange' ? 'login-accent-orange' : '',
    className,
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <div className={pageClass} data-tone={accent}>
      <main className="login-store-scroll" aria-label="Sign in">
        <section className="login-chapter login-chapter-hero" data-tone={accent} aria-label={productName}>
          <div className="login-chapter-inner">
            {logo ? <div className="login-logo inline-flex mb-5">{logo}</div> : null}
            {wordmark ? (
              <p className="login-wordmark" aria-label={productName}>
                {wordmark}
              </p>
            ) : null}
            {instanceMeta ? <div className="login-instance-meta">{instanceMeta}</div> : null}
            <h1 className="login-hero-title">{heroTitle}</h1>
            {tagline ? <p className="login-tagline">{tagline}</p> : null}
            {pills?.length ? (
              <div className="login-pill-row">
                {pills.map((pill) => (
                  <span key={pill.label} data-tone={pill.tone ?? accent} className="login-pill">
                    <span className="login-pill-dot" aria-hidden />
                    {pill.icon}
                    {pill.label}
                  </span>
                ))}
              </div>
            ) : null}
            {heroCta ? <div className="login-cta">{heroCta}</div> : null}
            {chapterNote ? <p className="login-chapter-note">{chapterNote}</p> : null}
          </div>
        </section>

        {showSignInChapter && children ? (
          <section
            id="login-sign-in"
            className="login-chapter login-chapter-sign-in"
            aria-label="Credentials"
          >
            <div className="login-chapter-inner login-sign-in-inner">
              <p className="login-form-heading">{formHeading}</p>
              <div className={`login-card ${formClassName}`.trim()}>{children}</div>
              {panelHint ? <p className="login-hint">{panelHint}</p> : null}
            </div>
          </section>
        ) : null}
      </main>

      {footer}
    </div>
  )
}

export function LoginError({ message }: { message: string }) {
  return (
    <div
      className="flex items-start gap-2.5 bg-[var(--zf-danger)]/10 border border-[var(--zf-danger)]/30 rounded-xl p-3 mb-6 login-shake"
      role="alert"
      aria-live="assertive"
    >
      <AlertCircle className="h-4 w-4 text-[var(--zf-danger)] shrink-0 mt-0.5" aria-hidden />
      <div>
        <p className="text-sm font-medium text-[var(--zf-ink)]">Unable to sign in</p>
        <p className="text-sm text-[var(--zf-secondary)] mt-0.5">{message}</p>
      </div>
    </div>
  )
}

export function LoginField({
  label,
  id,
  children,
}: {
  label: string
  id: string
  children: ReactNode
}) {
  return (
    <div className="mb-4">
      <label htmlFor={id} className="block text-xs font-medium text-[var(--zf-muted)] mb-1.5">
        {label}
      </label>
      <div className="relative group">{children}</div>
    </div>
  )
}

export function LoginSubmit({
  loading,
  disabled,
  children,
  className = '',
}: {
  loading?: boolean
  disabled?: boolean
  children: ReactNode
  className?: string
}) {
  return (
    <button type="submit" disabled={disabled || loading} className={`login-btn-primary ${className}`.trim()}>
      {children}
    </button>
  )
}

export function LoginRemember({
  checked,
  onChange,
  label = 'Remember me on this device',
  hint,
}: {
  checked: boolean
  onChange: (checked: boolean) => void
  label?: string
  hint?: string
}) {
  return (
    <div className="mt-5">
      <label className="flex items-center gap-2.5 cursor-pointer select-none">
        <input
          type="checkbox"
          checked={checked}
          onChange={(e) => onChange(e.target.checked)}
          className="w-4 h-4 rounded border-[var(--zf-hairline)] accent-[var(--zf-link-fill)]"
        />
        <span className="text-sm text-[var(--zf-muted)]">{label}</span>
      </label>
      {hint ? <p className="text-xs text-[var(--zf-muted)] mt-1.5 ml-[1.625rem]">{hint}</p> : null}
    </div>
  )
}

export function LoginDivider({ label = 'or' }: { label?: string }) {
  return (
    <div className="relative py-3 mt-2 text-center text-xs uppercase tracking-[0.18em] text-[var(--zf-muted)]">
      <span className="relative z-[1] px-3 bg-[var(--zf-surface)]">{label}</span>
      <div className="absolute inset-x-0 top-1/2 -translate-y-1/2 border-t border-[var(--zf-hairline)]" />
    </div>
  )
}

/** @deprecated typo guard — use PremiumLoginShell */
export const PremumLoginShell = PremiumLoginShell
