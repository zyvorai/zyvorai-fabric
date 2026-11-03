// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react'
import { Link } from 'react-router'
import { AuroraBackdrop, GlowCard } from '../ui'

/** The hero shared by the marketing sub-pages: the aurora behind a headline and a lede. Content sits above the glow. */
export function MarketingHero({ title, lede }: { title: ReactNode; lede: ReactNode }) {
  return (
    <section className="mkt-hero relative overflow-hidden !min-h-0 !pb-16 !pt-24">
      <AuroraBackdrop />
      <h1 className="mkt-reveal">{title}</h1>
      <p className="lede mkt-reveal-delay">{lede}</p>
    </section>
  )
}

/** The things a feature covers, as pills. Plain text in a list, so a screen reader reads them as a list. */
export function ChipList({ items, label }: { items: string[]; label: string }) {
  return (
    <ul aria-label={label} className="m-0 mt-4 flex list-none flex-wrap gap-2 p-0">
      {items.map((c) => (
        <li
          key={c}
          className="rounded-full border border-[var(--zf-hairline)] bg-[var(--zf-surface)] px-3 py-1 text-sm text-[var(--zf-secondary)]"
        >
          {c}
        </li>
      ))}
    </ul>
  )
}

interface FeatureRowProps {
  title: string
  body: ReactNode
  chips?: string[]
  chipsLabel?: string
  /** A picture that shows the feature. Decorative: the title, body and chips say everything it shows. */
  visual: ReactNode
  /** Put the picture on the left on wide screens. */
  flip?: boolean
}

/** One feature: words on one side, a picture on the other (stacked on a phone). The words come first in reading order. */
export function FeatureRow({ title, body, chips, chipsLabel, visual, flip = false }: FeatureRowProps) {
  return (
    <div className="grid items-center gap-8 md:grid-cols-2 md:gap-14" data-testid="feature-row" data-flip={flip}>
      <div className={`min-w-0 ${flip ? 'md:order-2' : ''}`}>
        <h2>{title}</h2>
        <p>{body}</p>
        {chips && chips.length > 0 && <ChipList items={chips} label={chipsLabel ?? `${title} covers`} />}
      </div>
      <GlowCard className={`flex min-h-[15rem] min-w-0 items-center justify-center p-4 sm:p-6 ${flip ? 'md:order-1' : ''}`}>
        <div className="w-full" aria-hidden="true">{visual}</div>
      </GlowCard>
    </div>
  )
}

/** The closing call to action, the same inverted band as the home page. */
export function CtaBand({
  title,
  body,
  primary,
  secondary,
}: {
  title: string
  body: string
  primary: { to: string; label: string }
  secondary?: { to: string; label: string }
}) {
  return (
    <section className="mkt-band">
      <h2>{title}</h2>
      <p>{body}</p>
      <div className="mt-6 flex flex-wrap justify-center gap-3">
        <Link to={primary.to} className="zf-btn mkt-band-cta">
          {primary.label}
        </Link>
        {secondary && (
          <Link to={secondary.to} className="zf-btn zf-btn-secondary">
            {secondary.label}
          </Link>
        )}
      </div>
    </section>
  )
}
