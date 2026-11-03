// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import MarketingLayout from '../../components/MarketingLayout'
import { CtaBand, FeatureRow, MarketingHero } from '../../components/marketing/blocks'
import { MerionVisual, NetworkVisual, StorageVisual, VmVisual } from '../../components/marketing/visuals'

export default function Product() {
  return (
    <MarketingLayout>
      <MarketingHero
        title={<>The control plane<br />for your private cloud.</>}
        lede="Zyvor Fabric wraps a pluggable VM driver — FluxVM — in production ops: RBAC, HA, network security, storage, and automation."
      />
      <section className="mkt-section !pt-0 space-y-24">
        <FeatureRow
          title="Virtual machines"
          body="Lifecycle, templates, warm pools, console and VNC, cloud-init, snapshots, and bulk operations — one model across interfaces."
          chips={['Lifecycle', 'Templates', 'Warm pools', 'Console and VNC', 'Cloud-init', 'Snapshots', 'Bulk operations']}
          visual={<VmVisual />}
        />
        <FeatureRow
          title="Network fabric"
          body="Bridges, VLANs, bonds, floating IPs, and Cilium-style network security: policies, firewall, QoS, DNS, VPN, NAT, and monitoring."
          chips={['Bridges', 'VLANs', 'Bonds', 'Floating IPs', 'Policies', 'Firewall', 'QoS', 'DNS', 'VPN', 'NAT', 'Monitoring']}
          visual={<NetworkVisual />}
          flip
        />
        <FeatureRow
          title="Storage & resilience"
          body="Pools and volumes, Ceph, backups, replication, fault tolerance, and site recovery — without assembling an OpenStack."
          chips={['Pools and volumes', 'Ceph', 'Backups', 'Replication', 'Fault tolerance', 'Site recovery']}
          visual={<StorageVisual />}
        />
        <FeatureRow
          title="Merion — agent workstation"
          body="Give an untrusted agent a FluxVM cell with signed policy, vaulted credentials, and host eBPF that proves zero CONNECT on a PDF brief — without copying consumer browser chrome."
          chips={['Signed policy', 'Vaulted credentials', 'Host eBPF', 'Zero CONNECT']}
          visual={<MerionVisual />}
          flip
        />
      </section>
      <CtaBand
        title="See it on your own fleet."
        body="Sign in to the console, or meet Merion, the agent workstation."
        primary={{ to: '/sign-in', label: 'Sign in to console' }}
        secondary={{ to: '/merion', label: 'Meet Merion →' }}
      />
    </MarketingLayout>
  )
}
