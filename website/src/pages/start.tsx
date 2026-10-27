import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import {PageMetadata} from '@docusaurus/theme-common';
import {CtaBand, HeroShell, Page, Section} from '@site/src/components/marketing/Shell';
import styles from './start.module.css';

/** [title, one line, route]. Every route is a built doc page; the sections follow the order a new reader needs them. */
type Doc = [string, string, string];

const START: Doc[] = [
  ['Install', 'System requirements, packages and building from source.', '/docs/getting-started/Installation'],
  ['Quick start', 'Create your first VM in five minutes.', '/docs/getting-started/Quick-Start'],
  ['Configuration', 'The config file, environment variables and every section.', '/docs/getting-started/Configuration'],
  ['Web console', 'Sign in, the dashboard, and managing VMs from the browser.', '/docs/getting-started/Web-UI'],
];

const LEARN: Doc[] = [
  ['Your first VM', 'Create, configure, start and connect to a virtual machine.', '/docs/tutorials/first-vm'],
  ['Networking', 'Bridges, VLANs, bonds, port forwarding and network policies.', '/docs/tutorials/networking'],
  ['Snapshots and backups', 'Snapshots, backup policies and restore.', '/docs/tutorials/snapshots-backups'],
  ['Security hardening', 'Lock down sign-in, RBAC, audit and encryption.', '/docs/tutorials/security-hardening'],
  ['Clustering', 'Run Fabric across more than one host.', '/docs/tutorials/clustering'],
  ['All tutorials', 'The full numbered list.', '/docs/tutorials/'],
];

const REFERENCE: Doc[] = [
  ['API reference', 'Every endpoint the console, CLI and operator call.', '/docs/reference/api'],
  ['CLI guide', 'What `fabricctl` can create, inspect and script.', '/docs/guides/cli'],
  ['Production deployment', 'Sizing, hardening and the systemd service.', '/docs/deployment/production'],
  ['Architecture', 'How the daemon, dataplane and storage fit together.', '/docs/architecture'],
  ['FAQ', 'Short answers to the questions that come up first.', '/docs/quick-reference/faq'],
  ['Glossary', 'The terms Fabric and Merion use.', '/docs/quick-reference/glossary'],
];

function Cards({docs}: {docs: Doc[]}): ReactNode {
  return (
    <div className={styles.grid}>
      {docs.map(([title, body, to]) => (
        <Link key={to} to={to} className={styles.card}>
          <h3 className={styles.cardTitle}>{title}</h3>
          <p className={styles.cardBody}>{body}</p>
          <span className={styles.cardGo}>Read →</span>
        </Link>
      ))}
    </div>
  );
}

/** Opens the navbar search. The plugin also listens for `/` anywhere on the site. */
function focusSearch() {
  document.querySelector<HTMLInputElement>('.navbar__search-input')?.focus();
}

export default function Start(): ReactNode {
  return (
    <Layout title="Documentation" description="Install, learn and look things up: the Zyvor Fabric documentation.">
      <PageMetadata title="Documentation" />
      <Page>
        <HeroShell
          eyebrow="Documentation"
          title="Everything you need."
          accent="Nothing you don’t."
          sub="Install Fabric, learn it by doing, then look things up. Press / anywhere to search."
          buttons={
            <>
              <Link className="button button--primary button--lg" to="/docs/getting-started/Quick-Start">
                Quick start
              </Link>
              <button type="button" className={`button button--outline button--lg button--secondary ${styles.searchBtn}`} onClick={focusSearch}>
                Search the docs
                <kbd className={styles.kbd}>/</kbd>
              </button>
            </>
          }
          stats={[
            ['5 min', 'to your first VM'],
            ['4', 'front doors, one API'],
            ['92', 'console pages'],
          ]}
          cueHref="#start"
          cueLabel="Jump to getting started"
        />
        <Section id="start" eyebrow="Start here" title="From nothing to a running VM." lede="Four short pages, in this order.">
          <Cards docs={START} />
        </Section>
        <Section eyebrow="Learn" title="Learn it by doing." lede="Step-by-step walkthroughs for the jobs you will actually do." tint>
          <Cards docs={LEARN} />
        </Section>
        <Section eyebrow="Look it up" title="When you know what you want.">
          <Cards docs={REFERENCE} />
        </Section>
        <CtaBand title="Can’t find it?">
          <Link className="button button--primary button--lg" to="/support">
            Ask for help
          </Link>
          <Link className="button button--outline button--lg button--secondary" to="/docs/documentation-map">
            Browse the full map
          </Link>
        </CtaBand>
      </Page>
    </Layout>
  );
}
