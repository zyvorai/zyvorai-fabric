import {useState} from 'react';
import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import {PROFILES} from './data';
import styles from './ProfileLadder.module.css';

export default function ProfileLadder(): ReactNode {
  const [id, setId] = useState<string>('measured');
  const p = PROFILES.find((x) => x.id === id) ?? PROFILES[1];
  const rows: [string, string][] = [
    ['Evidence', p.evidence],
    ['Hardware attestation', p.attestation],
    ['Can the host read it?', p.hostRead],
    ['What you may claim', p.claim],
  ];
  return (
    <div className={styles.wrap}>
      <div className={styles.steps} role="group" aria-label="Security profiles, weakest to strongest">
        {PROFILES.map((x, i) => (
          <button
            key={x.id}
            type="button"
            className={clsx(styles.step, styles[`h${i}`], styles[x.status], x.id === id && styles.on)}
            aria-pressed={x.id === id}
            onClick={() => setId(x.id)}>
            <span className={styles.tag}>{x.statusLabel}</span>
            <b>{x.name}</b>
            <span className={styles.sub}>{x.sub}</span>
          </button>
        ))}
      </div>

      <div className={styles.detail} key={p.id}>
        <dl className={styles.facts}>
          {rows.map(([k, v]) => (
            <div key={k}>
              <dt>{k}</dt>
              <dd>{v}</dd>
            </div>
          ))}
        </dl>
        <figure className={clsx(styles.receipt, p.status === 'gated' && styles.locked)}>
          <figcaption>
            attestation <span>illustrative · field names from MERION-0.2</span>
          </figcaption>
          <pre tabIndex={0}>
            <code>
              {p.receipt.map(([k, v]) => (
                <span key={k} className={styles.line}>
                  <i>{k}</i>: {v}
                  {'\n'}
                </span>
              ))}
            </code>
          </pre>
          {p.status === 'gated' && <span className={styles.ribbon}>Gated · no verified run yet</span>}
        </figure>
      </div>

      <p className={styles.note}>
        Muse’s Secure VM is described as having the same limit today (public detail is thin). Merion
        states its limit here, in the product. <Link to="/docs/merion/SECURITY-PROFILES">Security profiles ›</Link>
      </p>
    </div>
  );
}
