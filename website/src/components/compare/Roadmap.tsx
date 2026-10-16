import {useEffect, useRef} from 'react';
import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import {ROADMAP} from './data';
import type {RoadState} from './data';
import styles from './Roadmap.module.css';

const LABEL: Record<RoadState, string> = {
  shipped: 'Shipped',
  gated: 'Gated on hardware',
  next: 'Next',
};

export default function Roadmap(): ReactNode {
  const track = useRef<HTMLOListElement>(null);
  const root = useRef<HTMLDivElement>(null);

  // Fill the rail as the track scrolls sideways.
  useEffect(() => {
    const node = track.current;
    const host = root.current;
    if (!node || !host) {
      return;
    }
    let raf = 0;
    const update = () => {
      raf = 0;
      const max = node.scrollWidth - node.clientWidth;
      host.style.setProperty('--prog', max > 0 ? String(node.scrollLeft / max) : '1');
    };
    const onScroll = () => {
      if (!raf) {
        raf = requestAnimationFrame(update);
      }
    };
    update();
    node.addEventListener('scroll', onScroll, {passive: true});
    return () => {
      node.removeEventListener('scroll', onScroll);
      if (raf) {
        cancelAnimationFrame(raf);
      }
    };
  }, []);

  const nudge = (dir: 1 | -1) => {
    const node = track.current;
    if (node) {
      node.scrollBy({left: dir * Math.min(node.clientWidth * 0.8, 340), behavior: 'smooth'});
    }
  };

  return (
    <div className={styles.root} ref={root}>
      <div className={styles.bar}>
        <ul className={styles.legend} aria-hidden>
          <li className={styles.lgShipped}>Shipped</li>
          <li className={styles.lgGated}>Gated on hardware</li>
          <li className={styles.lgNext}>Next</li>
        </ul>
        <div className={styles.arrows}>
          <button type="button" aria-label="Scroll roadmap left" onClick={() => nudge(-1)}>
            ‹
          </button>
          <button type="button" aria-label="Scroll roadmap right" onClick={() => nudge(1)}>
            ›
          </button>
        </div>
      </div>
      <div className={styles.rail} aria-hidden>
        <span className={styles.fill} />
      </div>
      <ol className={styles.track} ref={track} tabIndex={0} aria-label="Merion roadmap">
        {ROADMAP.map((r) => (
          <li key={r.id} className={clsx(styles.item, styles[r.state])}>
            <span className={styles.node} aria-hidden />
            <span className={styles.state}>{LABEL[r.state]}</span>
            <b>{r.title}</b>
            <ul>
              {r.points.map((pt) => (
                <li key={pt}>{pt}</li>
              ))}
            </ul>
            <Link to={r.href}>{r.cta} ›</Link>
          </li>
        ))}
      </ol>
    </div>
  );
}
