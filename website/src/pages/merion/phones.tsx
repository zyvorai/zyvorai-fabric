import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import Head from '@docusaurus/Head';
import useBaseUrl from '@docusaurus/useBaseUrl';
import clsx from 'clsx';
import Reveal from '../../components/Reveal';
import Figure from '../../components/PhoneVendor/Figure';
import DayStepper from '../../components/PhoneVendor/DayStepper';
import {CtaBand, HeroShell, HonestyBand, Page, Section, useHashScroll} from '../../components/marketing/Shell';
import {CAN_SAY, CANNOT_SAY, MODELS, OWNS, STATUS, USE_CASES} from '../../data/phoneVendor';
import styles from '../../components/PhoneVendor/styles.module.css';

const BADGE = {built: 'Built and tested', reference: 'Reference code', gap: 'Not built'} as const;

export default function PhoneVendorPage(): ReactNode {
  useHashScroll();
  const card = useBaseUrl('/img/social-card.png', {absolute: true});

  return (
    <Layout
      title="Keep for phone makers — an agent computer per user, the keys on the phone"
      description="How an Android maker or any company with a phone and an account system can offer users a personal agent: a sealed Keep cell in the vendor's cloud, approvals signed by the phone's own key.">
      <Head>
        <meta property="og:image" content={card} />
        <meta name="twitter:card" content="summary_large_image" />
      </Head>
      <Page>
        <HeroShell
          eyebrow="Zyvor Keep · for phone makers"
          title="Every user gets an agent computer."
          accent="The phone holds the keys."
          sub="A blueprint for an Android maker, or anyone with a phone and an account system: a sealed Keep cell per job in your cloud, approvals that only the user’s enrolled phone key can sign, and the model you choose."
          buttons={
            <>
              <a className="button button--primary button--lg" href="#shape">
                See the architecture
              </a>
              <Link className="button button--outline button--lg button--secondary" to="/docs/merion/VENDORS">
                Read the guide
              </Link>
            </>
          }
          stats={[
            ['1 host', 'is one shard; add more and route users'],
            ['P-256', 'phone key signs each approval'],
            ['0', 'outbound connections from a cell, per run'],
          ]}
          cueHref="#shape"
          cueLabel="Scroll to the architecture"
        />
        <main>
          <Section
            id="shape"
            eyebrow="The shape"
            title="You run the accounts and the phones. Keep runs the cells."
            lede="Keep stays a single-host building block. You run many hosts, put a gateway in front, and route each user to one."
            wide>
            <Reveal>
              <Figure
                file="vendor-architecture.svg"
                alt="Architecture: phone, vendor gateway, push relay and model on the vendor side; shards running Keep and FluxVM cells on the Keep side."
                caption="Purple is yours, blue is Keep. Dashed boxes are interfaces Keep defines and you implement."
                minWidth={900}
              />
            </Reveal>
          </Section>

          <Section
            id="owns"
            eyebrow="Who runs what"
            title="A clean line between your product and ours."
            tint
            wide>
            <ul className={styles.legend}>
              <li>
                <i style={{background: '#7c5cd6'}} />
                The vendor owns
              </li>
              <li>
                <i style={{background: 'var(--c-accent)'}} />
                Keep provides
              </li>
            </ul>
            <ul className={styles.split}>
              {OWNS.map((o, i) => (
                <li key={o.title}>
                  <Reveal delay={(i % 4) * 70} className={styles.fillReveal}>
                    <div className={clsx(styles.own, o.who === 'vendor' ? styles.ownVendor : styles.ownKeep)}>
                      <b>{o.title}</b>
                      <span>{o.body}</span>
                    </div>
                  </Reveal>
                </li>
              ))}
            </ul>
          </Section>

          <Section
            id="day"
            eyebrow="A user’s day"
            title="Five steps from sign-up to an approved action."
            lede="Pick a step to see what the phone shows and which call is made."
            wide>
            <DayStepper />
            <Reveal>
              <Figure
                file="vendor-user-day.svg"
                alt="Five steps: sign up, enrol the phone, ask, approve, see what happened."
                caption="The same five steps, with who does each."
                minWidth={820}
              />
            </Reveal>
          </Section>

          <Section
            id="approval"
            eyebrow="The approval handshake"
            title="The phone signs the exact decision. The shard checks."
            lede="A person’s ‘yes’ is a signature over a readable text: the approval, the decision, a digest of the planned action, and a challenge that expires."
            tint
            wide>
            <Reveal>
              <Figure
                file="approval-handshake.svg"
                alt="Sequence: the agent asks, the shard opens an approval and pushes through the relay, the phone signs and the shard verifies before the action runs."
                caption="Keep never embeds a vendor push SDK: it posts a signed message to a relay you run. This proves the enrolled key made this decision. It does not make the vault user-held."
                minWidth={900}
              />
            </Reveal>
          </Section>

          <Section
            id="isolation"
            eyebrow="Many users, one shard"
            title="A user token reaches one person’s data. Nothing else."
            lede="Another user’s session, approval or artifact answers 404, the same as an id that does not exist. Operator routes are closed to users, and losing a phone means revoking its tokens."
            wide>
            <Reveal>
              <Figure
                file="tenant-isolation.svg"
                alt="Two users on one shard, each with their own sessions, cells, approvals, artifacts and audit rows; cross-user requests answer 404."
                caption={
                  <>
                    Details in <Link to="/docs/merion/TENANCY">Many users on one Keep</Link>.
                  </>
                }
                minWidth={860}
              />
            </Reveal>
          </Section>

          <Section
            id="usecases"
            eyebrow="What users can do"
            title="The files on a phone, turned into answers."
            lede="Ready-made use cases for what people export: chats, bank alerts, statements, calendars, contacts, booking and billing mail, receipts."
            tint
            wide>
            <p className={styles.ucFlow}>
              <b>Share a file in the app</b>
              <i aria-hidden>→</i>
              <span>gateway posts it to <code>POST /v1/demos/&#123;use_case&#125;</code> with the user’s token</span>
              <i aria-hidden>→</i>
              <b>A sealed cell reads it</b>
              <i aria-hidden>→</i>
              <span>the summary comes back, with 0 outbound connections reported</span>
            </p>
            <ul className={styles.ucGrid}>
              {USE_CASES.map((u, i) => (
                <li key={u.id}>
                  <Reveal delay={(i % 4) * 60} className={styles.fillReveal}>
                    <div className={clsx(styles.uc, u.built && styles.ucBuiltIn)}>
                      <span className={styles.ucLabel}>{u.built ? 'Built in' : 'Pack'}</span>
                      <b>{u.title}</b>
                      <span className={styles.ucFiles}>
                        {u.files.map((f) => (
                          <span className={styles.ucFile} key={f}>
                            {f}
                          </span>
                        ))}
                      </span>
                      <span>
                        <strong>You drop in:</strong> {u.drop}
                      </span>
                      <span>
                        <strong>You get:</strong> {u.get}
                      </span>
                    </div>
                  </Reveal>
                </li>
              ))}
            </ul>
            <p style={{marginTop: '1rem'}}>
              These are extractive: no model reads the file, and none reads photos or screenshots (Keep does no OCR). They
              handle sensitive files, and the evidence class is software-test, so the host’s operator could still read a
              cell’s memory. Details and how to copy one for your language:{' '}
              <Link to="/docs/merion/SCENARIOS#phone-user-packs">Phone-user packs</Link>.
            </p>
          </Section>

          <Section
            id="models"
            eyebrow="Your model"
            title="Pick the model. The agent never holds the key."
            lede="Use any OpenAI-compatible endpoint. The host adds the credential after the vault says yes, so the cell never sees a real secret."
            wide>
            <Reveal>
              <div className={styles.models}>
                <div className={styles.node}>
                  <b>Agent in a cell</b>
                  <span>Asks for a model call</span>
                </div>
                <span className={styles.arrowLine} aria-hidden>
                  →
                </span>
                <div className={styles.node}>
                  <b>Vault-gated model socket, on the host</b>
                  <span>Checks the vault, asks a person the first time, adds the key, records the call</span>
                  <div className={styles.chips}>
                    {MODELS.map((m) => (
                      <span className={styles.chip} key={m}>
                        {m}
                      </span>
                    ))}
                  </div>
                </div>
              </div>
            </Reveal>
            <p style={{marginTop: '1rem'}}>
              <Link to="/docs/merion/MODELS">Endpoints and credential descriptors</Link>. Provider URLs there are marked
              to verify against the provider’s current docs.
            </p>
          </Section>

          <Section
            id="numbers"
            eyebrow="Numbers"
            title="Measured once, on one host. Measure yours."
            lede="Cold runs took 13 to 21 seconds and got slower as more ran at once. That suits jobs, not a chat that must answer at once."
            tint
            wide>
            <Reveal>
              <Figure
                file="vendor-benchmark.svg"
                alt="Bar charts: median run time 12.7, 15.9 and 21.1 seconds; 4.7, 7.8 and 11.0 runs per minute; 1.0, 2.8 and 4.5 GiB of memory, at 1, 2 and 4 runs at once."
                caption={
                  <>
                    The method, the caveats and a failed first attempt after a reboot, left unexplained, are in{' '}
                    <Link to="/docs/merion/VENDORS#sizing-measure-do-not-guess">Sizing</Link>.
                  </>
                }
                minWidth={760}
              />
            </Reveal>
          </Section>

          <Section
            id="status"
            eyebrow="What exists"
            title="Built, reference, and not built."
            lede="Nothing here is drawn as working if it is not.">
            <ul className={styles.status}>
              {STATUS.map((s, i) => (
                <li key={s.area}>
                  <Reveal delay={(i % 4) * 50}>
                    <div className={clsx(styles.statusRow, s.state === 'gap' && styles.statusRowGap)}>
                      <span
                        className={clsx(
                          styles.badge,
                          s.state === 'built' && styles.badgeBuilt,
                          s.state === 'reference' && styles.badgeReference,
                          s.state === 'gap' && styles.badgeGap,
                        )}>
                        {BADGE[s.state]}
                      </span>
                      <span className={styles.statusArea}>
                        {s.href ? <Link to={s.href}>{s.area}</Link> : s.area}
                      </span>
                      <span className={styles.statusNote}>{s.note}</span>
                    </div>
                  </Reveal>
                </li>
              ))}
            </ul>
          </Section>

          <Section
            id="claims"
            eyebrow="What you can promise users"
            title="Say what is tested. Do not say what is not."
            tint
            wide>
            <div className={styles.claims}>
              <Reveal>
                <div className={clsx(styles.claimBox, styles.claimYes)}>
                  <h3>You can say</h3>
                  <ul>
                    {CAN_SAY.map((c) => (
                      <li key={c}>{c}</li>
                    ))}
                  </ul>
                </div>
              </Reveal>
              <Reveal delay={100}>
                <div className={clsx(styles.claimBox, styles.claimNo)}>
                  <h3>Do not say</h3>
                  <ul>
                    {CANNOT_SAY.map((c) => (
                      <li key={c}>{c}</li>
                    ))}
                  </ul>
                </div>
              </Reveal>
            </div>
          </Section>

          <Section id="start" eyebrow="Start here" title="Read in this order." wide>
            <ul className={styles.links}>
              <li>
                <Link to="/docs/merion/VENDORS">
                  <b>1. Keep for a phone vendor</b>
                  <span>The blueprint, sizing, regions, claims, and questions for your counsel.</span>
                </Link>
              </li>
              <li>
                <Link to="/docs/merion/TENANCY">
                  <b>2. Many users on one Keep</b>
                  <span>User tokens, isolation, quotas, usage and revocation.</span>
                </Link>
              </li>
              <li>
                <Link to="/docs/merion/mobile/">
                  <b>3. The phone side</b>
                  <span>Enrol a key, receive the push, sign, decide. Test vectors included.</span>
                </Link>
              </li>
              <li>
                <a href="https://github.com/zyvorai/fabric/tree/main/reference/vendor-gateway">
                  <b>4. Reference gateway</b>
                  <span>Login, placement by region, token minting and a push relay, as tested reference code.</span>
                </a>
              </li>
              <li>
                <Link to="/docs/merion/MODELS">
                  <b>5. Choosing a model</b>
                  <span>Qwen, DeepSeek, GLM, a local server or your own.</span>
                </Link>
              </li>
              <li>
                <Link to="/docs/merion/">
                  <b>6. Keep itself</b>
                  <span>The cell, policy, vault and audit that everything above stands on.</span>
                </Link>
              </li>
            </ul>
          </Section>

          <HonestyBand
            items={[
              'Evidence class is software-test: the vendor’s operators can still read a cell’s memory and the secrets in the host environment. Confidential hardware with user-held keys is the goal of Keep 0.2 and is not available.',
              'Keep makes no compliance claim for any country or industry. Questions about data location, model licensing and content rules belong with your own counsel and security team.',
              'Meta’s Muse is described only as publicly reported. Corrections welcome.',
            ]}
          />

          <CtaBand title="Start with one shard.">
            <Link className="button button--primary button--lg" to="/docs/merion/VENDORS">
              Read the guide
            </Link>
            <Link className="button button--outline button--lg button--secondary" to="/merion">
              About Keep
            </Link>
          </CtaBand>
        </main>
      </Page>
    </Layout>
  );
}
