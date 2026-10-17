import {useEffect, useRef, useState} from 'react';
import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import {QUICKSTARTS} from './data';
import type {Quick} from './data';
import styles from './Quickstart.module.css';

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Clipboard API is unavailable on insecure origins; fall back to a hidden textarea.
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.setAttribute('readonly', '');
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    let ok = false;
    try {
      ok = document.execCommand('copy');
    } catch {
      ok = false;
    }
    document.body.removeChild(ta);
    return ok;
  }
}

/** Tabbed copy-to-clipboard terminal. Defaults to the homepage recipes; /merion passes its own. */
export default function Quickstart({items = QUICKSTARTS}: {items?: Quick[]}): ReactNode {
  const [id, setId] = useState(items[0].id);
  const [state, setState] = useState<'idle' | 'copied' | 'failed'>('idle');
  const timer = useRef<number | undefined>(undefined);
  const q = items.find((x) => x.id === id) ?? items[0];

  useEffect(() => () => window.clearTimeout(timer.current), []);

  const onCopy = async () => {
    const ok = await copyText(q.code);
    setState(ok ? 'copied' : 'failed');
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setState('idle'), 1800);
  };

  return (
    <div className={styles.wrap}>
      <div className={styles.tabs} role="group" aria-label="Quickstart">
        {items.map((x) => (
          <button
            key={x.id}
            type="button"
            className={clsx(styles.tab, x.id === id && styles.on)}
            aria-pressed={x.id === id}
            onClick={() => {
              setId(x.id);
              setState('idle');
            }}>
            {x.label}
          </button>
        ))}
      </div>

      <div className={styles.panel} key={q.id}>
        <p className={styles.blurb}>{q.blurb}</p>
        <div className={styles.term}>
          <div className={styles.termBar}>
            <span>bash</span>
            <button type="button" onClick={onCopy}>
              {state === 'copied' ? 'Copied' : state === 'failed' ? 'Press ⌘C' : 'Copy'}
            </button>
            <span className={styles.sr} role="status" aria-live="polite">
              {state === 'copied' ? 'Copied to clipboard' : state === 'failed' ? 'Copy failed' : ''}
            </span>
          </div>
          <pre>
            <code>{q.code}</code>
          </pre>
        </div>
        {q.expect && (
          <p className={styles.expect}>
            <b>Expect</b> <code>{q.expect}</code>
          </p>
        )}
        {q.needs && (
          <ul className={styles.needs} aria-label="Prerequisites">
            {q.needs.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        )}
        {q.href && q.cta && (
          <Link className={styles.link} to={q.href}>
            {q.cta} ›
          </Link>
        )}
      </div>
    </div>
  );
}
