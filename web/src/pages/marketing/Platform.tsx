// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { Blocks, Boxes, Globe, Terminal } from 'lucide-react'
import MarketingLayout from '../../components/MarketingLayout'
import { CtaBand, MarketingHero } from '../../components/marketing/blocks'
import { HubVisual } from '../../components/marketing/visuals'
import { GlowCard } from '../../components/ui'

const SURFACES = [
  { name: 'Web', detail: 'Apple-minimal console for day-2 operations.', Icon: Globe },
  { name: 'CLI', detail: 'fabricctl — scriptable table, JSON, and YAML output.', Icon: Terminal },
  { name: 'Operator', detail: 'Kubernetes VirtualMachine CRDs reconciled to Fabric.', Icon: Boxes },
  { name: 'Terraform', detail: 'Declare VMs with the zyvor-fabricd provider.', Icon: Blocks },
]

export default function Platform() {
  return (
    <MarketingLayout>
      <MarketingHero
        title={<>Four ways in.<br />One daemon.</>}
        lede="Every interface talks to zyvor-fabricd over the same REST and WebSocket APIs."
      />
      <section className="mkt-section !pt-0 space-y-12">
        <GlowCard className="p-6 sm:p-10">
          <HubVisual />
        </GlowCard>
        <ul className="m-0 grid list-none gap-4 p-0 sm:grid-cols-2">
          {SURFACES.map(({ name, detail, Icon }) => (
            <li key={name}>
              <GlowCard className="h-full p-8" interactive>
                <span className="mb-4 flex h-11 w-11 items-center justify-center rounded-full border border-[var(--zf-hairline)] text-[var(--zf-brand-text)]" aria-hidden="true">
                  <Icon className="h-5 w-5" />
                </span>
                <h2 className="!mb-2 !text-2xl">{name}</h2>
                <p className="!m-0 !text-base">{detail}</p>
              </GlowCard>
            </li>
          ))}
        </ul>
      </section>
      <CtaBand
        title="One API underneath."
        body="Pick the interface that fits the job. They all see the same fleet."
        primary={{ to: '/sign-in', label: 'Sign in' }}
        secondary={{ to: '/product', label: 'See the product →' }}
      />
    </MarketingLayout>
  )
}
