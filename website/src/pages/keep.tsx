import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import Head from '@docusaurus/Head';
import useBaseUrl from '@docusaurus/useBaseUrl';
import marketing from '../../../docs/keep/marketing.json';
import Reveal from '../components/Reveal';
import KeepStory from '../components/KeepStory';
import CockpitMock from '../components/compare/CockpitMock';
import ProfileLadder from '../components/compare/ProfileLadder';
import Quickstart from '../components/compare/Quickstart';
import {DESKTOP_PACKS} from '../data/desktopPacks';
import type {Quick} from '../components/compare/data';
import {CtaBand, HeroShell, HonestyBand, Page, Section, useHashScroll} from '../components/marketing/Shell';
import shell from '../components/marketing/Shell.module.css';
import styles from './keep.module.css';

const COMPARE_ROWS = ['Where it runs', 'Policy', 'The cell', 'Model'];

/** The install tabs in marketing.json, shaped for the homepage's terminal component. */
const INSTALL: Quick[] = marketing.install.map((tab, i) => ({
  id: tab.label,
  label: tab.label,
  blurb: tab.note,
  code: tab.commands.join('\n'),
  ...(i === 0 ? {href: '/docs/tutorials/keep-pdf-brief', cta: 'Then brief a PDF'} : {}),
}));

export default function KeepPage(): ReactNode {
  useHashScroll();
  const card = useBaseUrl('/img/social-card.png', {absolute: true});
  const compare = marketing.compare.filter((r) => COMPARE_ROWS.includes(r.label));
  // "Your agent gets a real computer. You keep the keys." → headline + gradient line.
  const [headline, ...accent] = marketing.tagline.split(/(?<=\.)\s+/);

  return (
    <Layout
      title="Keep — your agent gets a real computer"
      description="An open-source workstation for an untrusted AI agent: a sealed FluxVM cell, signed policy and network rules enforced on the host, on hardware you control.">
      <Head>
        <meta property="og:image" content={card} />
        <meta name="twitter:card" content="summary_large_image" />
      </Head>
      <Page>
        <HeroShell
          eyebrow="Zyvor Keep"
          title={headline}
          accent={accent.join(' ')}
          sub={marketing.lede}
          buttons={
            <>
              <a className="button button--primary button--lg" href="#try">
                Try it in 60 seconds
              </a>
              <Link className="button button--outline button--lg button--secondary" to="/docs/keep/">
                Read the docs
              </Link>
            </>
          }
          stats={[
            ['0', 'egress connects, on stage'],
            ['1', 'signed policy you can diff'],
            ['3', 'open layers, one stack'],
          ]}
          cueHref="#how"
          cueLabel="Scroll to how it works"
        />
        <main>
          <Section
            id="how"
            eyebrow="How it works"
            title="A real computer for an agent you can’t fully trust."
            lede="A sealed cell, keys you hold, approvals only you can give, and a counter that proves it."
            wide>
            <KeepStory />
          </Section>

          <Section
            id="see-it"
            eyebrow="See it"
            title="The agent proposes. A person decides."
            lede="A real run of the Keep runtime: the agent’s change waits behind an approval, and nothing reaches the sandbox until a person says yes."
            wide>
            <Reveal>
              <figure style={{margin: '0 auto', maxWidth: 960}}>
                <video
                  autoPlay
                  loop
                  muted
                  playsInline
                  controls
                  poster={useBaseUrl('/demos/keep-speculate.gif')}
                  style={{width: '100%', borderRadius: 12}}>
                  <source src={useBaseUrl('/demos/keep-speculate.webm')} type="video/webm" />
                  <source src={useBaseUrl('/demos/keep-speculate.mp4')} type="video/mp4" />
                </video>
                <figcaption style={{marginTop: 12, fontSize: '0.9rem', opacity: 0.8}}>
                  Real Keep runtime. FluxVM is the CI stub, so no VM boots in this recording; the changeset it
                  returns is a response captured from a real FluxVM. Reproduce it with{' '}
                  <code>./scripts/record-demos.sh keep-speculate</code>.
                </figcaption>
              </figure>
            </Reveal>
          </Section>

          <Section
            id="cockpit"
            eyebrow="The cockpit"
            title="What you watch while it works."
            lede="A goal, its plan, the decisions the policy made, and the egress counter — with the honesty badge always on."
            tint
            wide>
            <CockpitMock />
          </Section>

          <Section
            id="why"
            eyebrow="Why Keep"
            title="You run it. You read it. You take it with you."
            wide>
            <ul className={shell.cards}>
              {marketing.values.map((v, i) => (
                <li key={v.title}>
                  <Reveal delay={i * 100} className={shell.fill}>
                    <div className={shell.card}>
                      <h3>{v.title}</h3>
                      <p className={styles.cardBody}>{v.body}</p>
                    </div>
                  </Reveal>
                </li>
              ))}
            </ul>
            <ul className={styles.features}>
              {marketing.features.map((f, i) => (
                <li key={f.title}>
                  <Reveal delay={(i % 4) * 80} className={shell.tile}>
                    <b>{f.title}</b>
                    <span>{f.body}</span>
                  </Reveal>
                </li>
              ))}
            </ul>
          </Section>

          <Section
            id="profiles"
            eyebrow="Security profiles"
            title="Three rungs. One says what it can’t prove."
            lede="Keep labels every cell with the evidence it actually has. Measured is software-test; hardware attestation stays gated until a verified run."
            tint
            wide>
            <ProfileLadder />
          </Section>

          <Section id="try" eyebrow="Try it" title="Run it in 60 seconds.">
            <Quickstart items={INSTALL} />
          </Section>

          <Section
            id="phones"
            eyebrow="For phone makers"
            title="An agent computer per user. The keys stay on the phone."
            lede="How an Android maker, or anyone with a phone and an account system, can offer this: a sealed cell per job in your cloud, approvals only the user’s enrolled phone key can sign, and the model you choose."
            wide>
            <Reveal>
              <div className={styles.phonesFigure}>
                <img
                  src={useBaseUrl('/keep/vendor-architecture.svg')}
                  alt="Phone, vendor gateway and push relay on the vendor side; shards running Keep cells on the Keep side."
                  loading="lazy"
                />
              </div>
            </Reveal>
            <Link className={styles.more} to="/keep/phones">
              Keep for phone makers ›
            </Link>
          </Section>

          <Section
            id="desktop"
            eyebrow="Mac, Windows, developer and office files"
            title="Files from a Mac or a PC, read in a sealed cell."
            lede="Ready-made use cases for what people export from their machines and tools (GitHub, Xcode, VS Code, browsers, Excel) and for the paperwork around invoices, purchase orders, staff and claims. Keep reads the file you give it; it does not connect to the computer or drive its desktop, and it is not your books of account."
            tint
            wide>
            <ul className={styles.features}>
              {DESKTOP_PACKS.map((p, i) => (
                <li key={p.id}>
                  <Reveal delay={(i % 4) * 80} className={shell.tile}>
                    <b>
                      {p.os} · {p.title}
                    </b>
                    <span>
                      <strong>Drop in:</strong> {p.drop}
                    </span>
                    <span>{p.get}</span>
                  </Reveal>
                </li>
              ))}
            </ul>
            <p className={styles.cardBody} style={{marginTop: '1rem'}}>
              Extractive: no model reads the file, and the office packs list and count rather than add up. The Windows samples follow the commands’ documented layouts and are
              not yet checked against a real Windows export. The output can name hosts, accounts and software, and the
              evidence class is software-test.{' '}
              <a href="https://github.com/zyvorai/fabric/blob/main/docs/keep/SCENARIOS.md#mac-and-windows-packs">
                Details and commands
              </a>
              .
            </p>
          </Section>

          <Section
            id="muse"
            eyebrow="Meta Muse vs Keep"
            title="Same threat model. Different owner."
            tint>
            <div className={styles.rows}>
              {compare.map((r, i) => (
                <Reveal key={r.label} delay={i * 80} className={styles.row}>
                  <span className={styles.rowLabel}>{r.label}</span>
                  <span className={styles.rowMuse}>{r.muse}</span>
                  <span className={styles.rowKeep}>{r.keep}</span>
                </Reveal>
              ))}
            </div>
            <Link className={styles.more} to="/?t=stack#matrix">
              The full comparison ›
            </Link>
          </Section>

          <HonestyBand
            items={[
              marketing.honesty,
              <>
                Muse details here are as publicly described; public detail is thin.{' '}
                <a href="https://github.com/zyvorai/fabric/issues">Corrections welcome.</a>
              </>,
            ]}
          />

          <CtaBand title="Run the open version.">
            <Link className="button button--primary button--lg" to="/docs/getting-started/Quick-Start">
              Quick start
            </Link>
            <Link className="button button--outline button--lg button--secondary" to="/">
              Fabric vs the field
            </Link>
          </CtaBand>
        </main>
      </Page>
    </Layout>
  );
}
