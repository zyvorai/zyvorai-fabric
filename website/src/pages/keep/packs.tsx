import {useMemo, useState} from 'react';
import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import clsx from 'clsx';
import {HeroShell, HonestyBand, Page, Section, useHashScroll} from '../../components/marketing/Shell';
import {GALLERY_PACKS} from '../../data/packGallery.generated';
import styles from '../../components/PackGallery/styles.module.css';

const GROUPS = ['All', ...Array.from(new Set(GALLERY_PACKS.map((p) => p.group))).sort()];
const README = 'https://github.com/zyvorai/fabric/tree/main/examples/merion-agents/';

export default function PackGalleryPage(): ReactNode {
  useHashScroll();
  const [group, setGroup] = useState('All');
  const [query, setQuery] = useState('');
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return GALLERY_PACKS.filter(
      (p) =>
        (group === 'All' || p.group === group) &&
        (!q || `${p.id} ${p.title} ${p.description} ${p.accepts.join(' ')} ${p.reads}`.toLowerCase().includes(q)),
    );
  }, [group, query]);

  return (
    <Layout
      title="Keep use cases: drop a file, get the answer"
      description="Every Keep use case, generated from the packs in the repository: statements, chats, logs, decks, receipts and bills from photos, bank operations files and more.">
      <Page>
        <HeroShell
          eyebrow="Zyvor Keep · use cases"
          title="Drop a file."
          accent="Get the answer."
          sub={`${GALLERY_PACKS.length} ready-made use cases. Each one is a small declarative pack you can read, and each runs in a sealed cell with no network.`}
          buttons={
            <>
              <a className="button button--primary button--lg" href="#gallery">
                Browse the use cases
              </a>
              <Link className="button button--outline button--lg button--secondary" to="/docs/merion/PACKS">
                Write your own
              </Link>
            </>
          }
          stats={[
            [String(GALLERY_PACKS.length), 'use cases in the repository'],
            ['0', 'outbound connections from a cell, per run'],
            ['JSON', 'a pack is one small file'],
          ]}
          cueHref="#gallery"
          cueLabel="Scroll to the use cases"
        />
        <main>
          <Section id="gallery" eyebrow="Gallery" title="Find the one that reads your file." wide>
            <div className={styles.toolbar}>
              <input
                className={styles.search}
                type="search"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search: statement, .eml, log, receipt…"
                aria-label="Search use cases"
              />
              <div className={styles.chips} role="group" aria-label="Filter by area">
                {GROUPS.map((g) => (
                  <button
                    key={g}
                    type="button"
                    className={clsx(styles.chip, g === group && styles.chipOn)}
                    aria-pressed={g === group}
                    onClick={() => setGroup(g)}>
                    {g}
                  </button>
                ))}
              </div>
            </div>
            <p className={styles.count} aria-live="polite">
              {shown.length} of {GALLERY_PACKS.length} use cases
            </p>
            {shown.length === 0 ? (
              <p className={styles.empty}>No use case matches. Clear the search or pick another area.</p>
            ) : (
              <div className={styles.grid}>
                {shown.map((p) => (
                  <a key={p.id} className={styles.card} href={`${README}${p.id}`}>
                    <p className={styles.group}>{p.group}</p>
                    <h3 className={styles.cardTitle}>{p.title}</h3>
                    <p className={styles.desc}>{p.description}</p>
                    <div className={styles.meta}>
                      {p.accepts.map((a) => (
                        <span key={a} className={styles.tag}>
                          .{a}
                        </span>
                      ))}
                      <span className={styles.tag}>{p.reads}</span>
                      {p.sample && <span className={styles.tag}>sample included</span>}
                      {p.model && <span className={styles.tag}>model step</span>}
                    </div>
                  </a>
                ))}
              </div>
            )}
          </Section>
          <HonestyBand
            items={[
              'The cell has no network, and the result reports 0 outbound connections. The evidence class is software-test: whoever operates the host could still read a cell’s memory.',
              'Summaries are extractive (keywords, patterns, counts, tables). No model reads your file unless a use case declares one and your host allows it.',
              'These packs read personal files. Check amounts read from photos against the original; the bank packs have not been run on real exports yet.',
            ]}
          />
        </main>
      </Page>
    </Layout>
  );
}
