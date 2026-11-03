// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { PageHeader, GlowCard, AuroraBackdrop, StatusPulse, AnimatedNumber, Donut, FlowLine, Illustration } from '../components/ui'
import type { IllustrationName, Tone } from '../components/ui'

const TONES: Array<{ tone: Tone; title: string; text: string }> = [
  { tone: 'neutral', title: 'Neutral', text: 'Plain depth: sheen and shadow, no halo.' },
  { tone: 'brand', title: 'Brand', text: 'The Zyvor orange, for the one thing to look at.' },
  { tone: 'proof', title: 'Proof', text: '0 CONNECT, sealed: a state that is good and verified.' },
  { tone: 'warn', title: 'Warning', text: 'Close to a limit, not over it.' },
  { tone: 'danger', title: 'Danger', text: 'Refused, failed or over a limit.' },
]

const ILLUSTRATIONS: Array<{ name: IllustrationName; label: string }> = [
  { name: 'no-vms', label: 'No VMs' },
  { name: 'no-sessions', label: 'No sessions' },
  { name: 'all-clear', label: 'All clear' },
  { name: 'sealed', label: 'Sealed cell' },
]

/** Gallery of the bold-layer primitives (ux foundation). Not linked from the navigation: it exists so each primitive can be
    looked at, screenshotted and axe-checked in both themes in one place. */
export default function UiKit() {
  return (
    <div className="space-y-8">
      <PageHeader title="UI kit" description="Depth, glow and motion primitives. Every one reads the theme tokens." />

      <section aria-labelledby="kit-hero" className="relative overflow-hidden rounded-[var(--zf-radius-lg)] border border-[var(--zf-hairline)] bg-[var(--zf-surface)] p-8">
        <AuroraBackdrop />
        <div className="relative">
          <h2 id="kit-hero" className="text-3xl font-semibold tracking-[-0.022em] text-[var(--zf-ink)]">Aurora backdrop</h2>
          <p className="mt-2 max-w-xl text-[var(--zf-secondary)]">A slow glow behind a hero. Decorative, stops for reduced motion.</p>
        </div>
      </section>

      <section aria-labelledby="kit-cards">
        <h2 id="kit-cards" className="mb-3 text-lg font-semibold text-[var(--zf-ink)]">Glow cards</h2>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {TONES.map((t) => (
            <GlowCard key={t.tone} tone={t.tone} className="p-5">
              <div className="flex items-center gap-2">
                <StatusPulse tone={t.tone} live={t.tone !== 'neutral'} label={`${t.title} state`} />
                <h3 className="font-semibold text-[var(--zf-ink)]">{t.title}</h3>
              </div>
              <p className="mt-2 text-sm text-[var(--zf-secondary)]">{t.text}</p>
            </GlowCard>
          ))}
        </div>
      </section>

      <section aria-labelledby="kit-data" className="grid gap-4 lg:grid-cols-3">
        <h2 id="kit-data" className="sr-only">Data graphics</h2>
        <GlowCard className="flex items-center gap-5 p-5">
          <Donut
            center="6"
            centerLabel="VMs"
            segments={[
              { label: 'Running', value: 3, color: 'var(--zf-success)' },
              { label: 'Paused', value: 1, color: 'var(--zf-warning)' },
              { label: 'Stopped', value: 1, color: 'var(--zf-muted)' },
              { label: 'Failed', value: 1, color: 'var(--zf-danger)' },
            ]}
          />
          <ul className="space-y-1 text-sm text-[var(--zf-secondary)]">
            <li>Running 3</li>
            <li>Paused 1</li>
            <li>Stopped 1</li>
            <li>Failed 1</li>
          </ul>
        </GlowCard>
        <GlowCard className="p-5">
          <p className="text-sm text-[var(--zf-muted)]">Calls today</p>
          <p className="mt-1 text-5xl font-semibold tracking-[-0.03em] text-[var(--zf-ink)]">
            <AnimatedNumber value={1284} />
          </p>
          <p className="mt-2 text-sm text-[var(--zf-secondary)]">Counts to a new value; static with reduced motion.</p>
        </GlowCard>
        <GlowCard className="p-5">
          <svg viewBox="0 0 240 90" className="h-24 w-full" aria-label="Agent cell talks to the model through the gateway" role="img">
            <circle cx="30" cy="45" r="14" fill="var(--zf-surface)" stroke="var(--zf-brand)" strokeWidth="2" />
            <circle cx="120" cy="45" r="14" fill="var(--zf-surface)" stroke="var(--zf-proof)" strokeWidth="2" />
            <circle cx="210" cy="45" r="14" fill="var(--zf-surface)" stroke="var(--zf-cool)" strokeWidth="2" />
            <FlowLine d="M44 45H106" tone="brand" />
            <FlowLine d="M134 45H196" tone="proof" />
          </svg>
          <p className="mt-2 text-sm text-[var(--zf-secondary)]">Cell, gateway, model. Moving dashes mean traffic.</p>
        </GlowCard>
      </section>

      <section aria-labelledby="kit-ill">
        <h2 id="kit-ill" className="mb-3 text-lg font-semibold text-[var(--zf-ink)]">Illustrations</h2>
        <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
          {ILLUSTRATIONS.map((i) => (
            <GlowCard key={i.name} className="p-4 text-center">
              <Illustration name={i.name} />
              <p className="mt-2 text-sm text-[var(--zf-secondary)]">{i.label}</p>
            </GlowCard>
          ))}
        </div>
      </section>
    </div>
  )
}
