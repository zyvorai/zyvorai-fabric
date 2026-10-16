import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Reveal from '@site/src/components/Reveal';
import {GATE_CHECKS, RUNS, runHref} from './data';
import styles from './Receipts.module.css';

export default function Receipts(): ReactNode {
  return (
    <div className={styles.wrap}>
      <ul className={styles.runs}>
        {RUNS.map((r, i) => (
          <li key={r.id}>
            <Reveal delay={i * 90} className={styles.fill}>
              <article className={styles.card}>
                <header>
                  <span className={styles.when}>{r.when}</span>
                  <span className={styles.badge}>software-test</span>
                </header>
                <h3>{r.title}</h3>
                <p className={styles.tpl}>
                  template <code>{r.template}</code>
                </p>
                <ul className={styles.results}>
                  {r.results.map((x) => (
                    <li key={x}>{x}</li>
                  ))}
                </ul>
                {r.note && <p className={styles.note}>{r.note}</p>}
                <a className={styles.id} href={runHref(r.id)}>
                  {r.id} ›
                </a>
              </article>
            </Reveal>
          </li>
        ))}
      </ul>

      <Reveal delay={120}>
        <div className={styles.gate}>
          <h3>What the gate proves</h3>
          <dl>
            {GATE_CHECKS.map(([k, v]) => (
              <div key={k}>
                <dt>{k}</dt>
                <dd>{v}</dd>
              </div>
            ))}
          </dl>
          <p>
            Archived logs live in the repo; <code>./scripts/merion-pilot-gate.sh</code> reruns the
            gate. <Link to="/docs/merion/pilot-runs/">Pilot runs ›</Link>
          </p>
        </div>
      </Reveal>
    </div>
  );
}
