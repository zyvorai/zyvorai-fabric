// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { KeyRound, Network, ScrollText } from 'lucide-react'
import MarketingLayout from '../../components/MarketingLayout'
import { ChipList, CtaBand, MarketingHero } from '../../components/marketing/blocks'
import { GlowCard, Illustration } from '../../components/ui'

const PILLARS = [
  {
    title: 'Access',
    body: 'Admin, User, and Viewer roles. API keys for automation. Optional LDAP and OIDC.',
    chips: ['Admin', 'User', 'Viewer', 'API keys', 'LDAP (optional)', 'OIDC (optional)'],
    Icon: KeyRound,
  },
  {
    title: 'Network security',
    body: 'Policies, firewall, services, QoS, DNS, VPN mesh, packet mirror, NAT, and live monitoring — from the console or CLI.',
    chips: ['Policies', 'Firewall', 'Services', 'QoS', 'DNS', 'VPN mesh', 'Packet mirror', 'NAT', 'Live monitoring'],
    Icon: Network,
  },
  {
    title: 'Compliance',
    body: 'Audit logs, encryption at rest for sensitive state, and certificate management for TLS operations.',
    chips: ['Audit logs', 'Encryption at rest', 'Certificate management'],
    Icon: ScrollText,
  },
]

export default function SecurityPage() {
  return (
    <MarketingLayout>
      <MarketingHero
        title={<>Security that<br />stays out of the way.</>}
        lede="JWT roles, audit export, encryption, certificates, and a full network-security stack — built into the control plane."
      />
      <section className="mkt-section !pt-0 space-y-10">
        <div className="-mt-4">
          <Illustration name="all-clear" size={200} />
        </div>
        <ul className="m-0 grid list-none gap-4 p-0 md:grid-cols-3">
          {PILLARS.map(({ title, body, chips, Icon }) => (
            <li key={title}>
              <GlowCard className="h-full p-7">
                <span className="mb-4 flex h-11 w-11 items-center justify-center rounded-full border border-[var(--zf-hairline)] text-[var(--zf-proof-text)]" aria-hidden="true">
                  <Icon className="h-5 w-5" />
                </span>
                <h2 className="!mb-2 !text-2xl">{title}</h2>
                <p className="!m-0 !text-base">{body}</p>
                <ChipList items={chips} label={`${title} includes`} />
              </GlowCard>
            </li>
          ))}
        </ul>
      </section>
      <CtaBand
        title="Built into the control plane."
        body="Roles, network policy and audit logs come with the console."
        primary={{ to: '/sign-in', label: 'Sign in' }}
        secondary={{ to: '/platform', label: 'See the platform →' }}
      />
    </MarketingLayout>
  )
}
