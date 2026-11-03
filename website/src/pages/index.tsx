import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';
import {PageMetadata} from '@docusaurus/theme-common';
import Reveal from '@site/src/components/Reveal';
import Quickstart from '@site/src/components/compare/Quickstart';
import type {Quick} from '@site/src/components/compare/data';
import {CtaBand, HeroShell, HonestyBand, Page, Section, useHashScroll} from '@site/src/components/marketing/Shell';
import shell from '@site/src/components/marketing/Shell.module.css';
import styles from './home.module.css';

/** The four front doors: one API behind all of them. */
const DOORS: [string, string][] = [
  ['CLI', '`fabricctl` creates, inspects and scripts everything the API can do.'],
  ['Web console', '92 pages: fleet health, VMs, networks, storage, security and Merion.'],
  ['Kubernetes operator', '`VirtualMachine` resources next to your pods, turned into API calls.'],
  ['Terraform', 'A provider is included. Plan your private cloud like any other.'],
];

const PILLARS: [string, string, string, string][] = [
  ['Compute and network', 'VM lifecycle and software-defined networking, with a TC/eBPF dataplane that enforces per-VM policy and rate limits.', '/docs/network-fabric-architecture', 'Network Fabric'],
  ['Secure by default', 'PAM, LDAP and OIDC sign-in, JWT auth, three-tier RBAC, audit export and encryption at rest. Built in, not bolted on.', 'https://github.com/zyvorai/fabric/blob/main/SECURITY.md', 'Security'],
  ['Private inference (Beta)', 'OpenAI-compatible endpoints on the same daemon: models, Maglev-weighted backends and API keys. Single-cluster Beta, not GA.', '/docs/ai-workloads-at-a-glance', 'AI Workloads'],
];

const MERION: [string, string][] = [
  ['Approve from anywhere', 'Your iPhone or Android phone signs each decision with a key that never leaves it. Slack and Teams get the same request with the host-rendered preview; buttons appear only when no phone signature is required.'],
  ['See everything it did', 'Every action lands in a signed audit trail. Send it to any OpenTelemetry collector; an allowlist keeps file contents, previews and credentials out.'],
  ['Propose, then decide', 'The agent speculates in an isolated copy of the cell. Its changes wait behind an approval, and the agent can never approve.'],
  ['Sealed by design', 'A deny-all network policy set by the host, credentials that never enter the cell, and a counter that shows zero outbound connections.'],
];

const QUICK: Quick[] = [
  {
    id: 'source',
    label: 'From source',
    blurb: 'Build, install, start the daemon, and create your first VM. Needs a Linux server with KVM.',
    code: `git clone https://github.com/zyvorai/fabric.git && cd fabric
make build && sudo make install
sudo systemctl enable --now zyvor-fabricd
fabricctl create web-01 --image fedora-41 --cpus 2 --memory 4096 --tenant acme`,
    expect: 'Console at https://localhost:9095/app',
    needs: ['Linux with KVM', 'Ports 9095 (API and UI) and 7788 (FluxVM, localhost)'],
    href: '/docs/getting-started/Quick-Start',
    cta: 'Getting started',
  },
  {
    id: 'ship',
    label: 'Ship to a host',
    blurb: 'One command puts FluxVM and Fabric on a remote bare-metal host.',
    code: './scripts/ship USER@HOST',
    expect: 'Fabric and FluxVM running under systemd',
    needs: ['SSH access to the host', 'Linux with KVM'],
    href: '/docs/deploy',
    cta: 'Deploy guide',
  },
  {
    id: 'docker',
    label: 'Docker',
    blurb: 'Try it in containers on a machine that has KVM.',
    code: 'make docker-up',
    expect: 'http://localhost:9095',
    needs: ['Docker or Podman', '/dev/kvm available'],
    href: '/docs/DOCKER',
    cta: 'Docker guide',
  },
];

export default function Home(): ReactNode {
  useHashScroll();
  return (
    <Layout
      title="Zyvor Fabric — the private cloud that fits in one daemon"
      description="VMs, networking, storage, security and AI inference from one ~15 MB Rust daemon on any Linux server with KVM. Open source, Apache-2.0.">
      <PageMetadata image="/img/compare-social.png" />
      <Page>
        <HeroShell
          eyebrow="Zyvor Fabric"
          title="The private cloud"
          accent="that fits in one daemon."
          sub="VMs, networking, storage, security and AI, from one ~15 MB Rust daemon. About five minutes on any Linux server with KVM."
          buttons={
            <>
              <a className="button button--primary button--lg" href="#start">
                Get started
              </a>
              <Link className="button button--outline button--lg button--secondary" to="/merion">
                Meet Merion
              </Link>
            </>
          }
          stats={[
            ['780+', 'REST endpoints'],
            ['4', 'front doors, one API'],
            ['0', 'security issues outstanding'],
          ]}
          cueHref="#doors"
          cueLabel="Scroll to the front doors"
        />
        <main>
          <Section
            id="doors"
            eyebrow="One daemon"
            title="Four front doors. One API."
            lede="The CLI, the console, Kubernetes and Terraform all talk to the same API, so nothing drifts between them."
            wide>
            <ul className={styles.four}>
              {DOORS.map(([t, b], i) => (
                <li key={t}>
                  <Reveal delay={i * 80} className={shell.tile}>
                    <b>{t}</b>
                    <span>{b}</span>
                  </Reveal>
                </li>
              ))}
            </ul>
            <p className={styles.note}>
              Fabric decides what should exist. <a href="https://github.com/zyvorai/zyvor-fluxvm">FluxVM</a> makes it
              exist. <a href="https://github.com/zyvorai/zyvor-guestkit">GuestKit</a> prepares the disk.
            </p>
          </Section>

          <Section id="pillars" eyebrow="What is inside" title="Everything a private cloud needs." tint wide>
            <ul className={shell.cards}>
              {PILLARS.map(([t, b, to, cta], i) => (
                <li key={t}>
                  <Reveal delay={i * 100} className={shell.fill}>
                    <div className={shell.card}>
                      <Heading as="h3">{t}</Heading>
                      <p className={styles.body}>{b}</p>
                      <Link className={shell.cardLink} to={to}>
                        {cta} ›
                      </Link>
                    </div>
                  </Reveal>
                </li>
              ))}
            </ul>
          </Section>

          <Section
            id="merion"
            eyebrow="Merion"
            title="Your agent gets a real computer. You keep the keys."
            lede="An open workstation for an AI agent you can’t fully trust: a sealed microVM, network rules the host enforces, and approvals only you can give."
            wide>
            <ul className={styles.four}>
              {MERION.map(([t, b], i) => (
                <li key={t}>
                  <Reveal delay={i * 80} className={shell.tile}>
                    <b>{t}</b>
                    <span>{b}</span>
                  </Reveal>
                </li>
              ))}
            </ul>
            <Link className={styles.more} to="/merion">
              Explore Merion ›
            </Link>
          </Section>

          <Section
            id="clusters"
            eyebrow="Fleets"
            title="Every cluster. One view."
            lede="Register your other Fabric daemons and see their hosts, VMs and health together. A host that is down never breaks the list, and maintenance runs on the target host’s own API with your own permissions."
            tint>
            <Link className={styles.more} to="/docs/federation">
              Federation guide ›
            </Link>
          </Section>

          <Section id="start" eyebrow="Get started" title="Five minutes. One server.">
            <Quickstart items={QUICK} />
          </Section>

          <Section
            eyebrow="Compare"
            title="How it stacks up."
            lede="Fabric against Proxmox, OpenStack and libvirt, and the Merion stack against Meta’s Muse, side by side, with the cases where something else is the better choice."
            tint>
            <Link className={styles.more} to="/compare">
              See the matrix ›
            </Link>
          </Section>

          <HonestyBand
            items={[
              <>
                Live migration: the disk-copy path is GA; the native FluxVM transport is preview and full HA cutover is
                pre-GA. For large multi-host clusters with mature shared-storage migration today, Proxmox VE is the safer
                choice.
              </>,
              <>AI Workloads is single-cluster Beta, not GA.</>,
              <>
                Merion evidence is <code>software-test</code>. The Slack, Teams, OpenTelemetry, federation and Android
                features are tested against local fakes, not yet a live workspace, collector, second daemon or device.
              </>,
            ]}
          />

          <CtaBand title="Run the open version.">
            <Link className="button button--primary button--lg" to="/docs/getting-started/Quick-Start">
              Quick start
            </Link>
            <Link className="button button--outline button--lg button--secondary" to="/compare">
              Compare
            </Link>
          </CtaBand>
        </main>
      </Page>
    </Layout>
  );
}
