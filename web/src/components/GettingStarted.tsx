// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router'
import { ArrowRight, LayoutTemplate, Server, Terminal, Users } from 'lucide-react'
import { GlowCard, Illustration } from './ui'

const STEPS = [
  {
    title: 'Create your first VM',
    description: 'Pick an image, size it, and boot — usually under a minute.',
    to: '/app/create',
    cta: 'Create VM',
    Icon: Server,
  },
  {
    title: 'Start from a template',
    description: 'Skip blank-VM setup with a preconfigured starting point.',
    to: '/app/templates',
    cta: 'View templates',
    Icon: LayoutTemplate,
  },
  {
    title: 'Explore the API',
    description: 'Fire live requests at every endpoint without writing a client.',
    to: '/app/playground',
    cta: 'Open playground',
    Icon: Terminal,
  },
  {
    title: 'Invite your team',
    description: 'Set up access control before handing out the console.',
    to: '/app/access-control',
    cta: 'Configure access',
    Icon: Users,
  },
]

/** Shown when the fleet is empty: calm first-run guidance. The first step is the one to take, so it carries the brand halo. */
export function GettingStarted() {
  return (
    <div className="zf-panel p-6 sm:p-10 animate-fade-in">
      <div className="mb-8 flex flex-col items-center gap-4 text-center sm:flex-row sm:text-left">
        <Illustration name="no-vms" size={168} />
        <div>
          <h2 className="text-[28px] font-semibold tracking-[-0.03em] text-[var(--zf-ink)] mb-2">Get started</h2>
          <p className="text-[15px] text-[var(--zf-muted)] max-w-xl">
            Your private cloud is ready. Create a VM or explore the surfaces below.
          </p>
        </div>
      </div>
      <ol className="grid gap-4 sm:grid-cols-2">
        {STEPS.map((step, i) => (
          <li key={step.to}>
            <GlowCard tone={i === 0 ? 'brand' : 'neutral'} interactive className="h-full p-5">
              <div className="flex items-start gap-3">
                <span
                  className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-[var(--zf-hairline)] text-[var(--zf-brand-text)]"
                  aria-hidden="true"
                >
                  <step.Icon className="h-4 w-4" />
                </span>
                <div className="min-w-0">
                  <h3 className="text-[17px] font-semibold tracking-[-0.02em] mb-1">
                    <span className="text-[var(--zf-muted)] tabular-nums">{i + 1}. </span>
                    {step.title}
                  </h3>
                  <p className="text-[14px] text-[var(--zf-muted)] mb-3">{step.description}</p>
                  <Link to={step.to} className="inline-flex items-center gap-1 text-[14px] font-medium text-[var(--zf-link)]">
                    {step.cta}
                    <ArrowRight className="w-3.5 h-3.5" />
                  </Link>
                </div>
              </div>
            </GlowCard>
          </li>
        ))}
      </ol>
    </div>
  )
}
