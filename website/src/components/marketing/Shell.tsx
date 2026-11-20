// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import {useEffect, useRef} from 'react';
import type {ReactNode, RefObject} from 'react';
import clsx from 'clsx';
import Heading from '@theme/Heading';
import Reveal from '@site/src/components/Reveal';
import styles from './Shell.module.css';

/**
 * Shared building blocks for the Fabric marketing pages (homepage and /keep):
 * one set of tokens, one hero, one section rhythm, one closing pair of black bands.
 */

/** Writes 0→1 scroll progress of the hero into a CSS variable. */
function useHeroProgress(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const node = ref.current;
    if (!node || window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      return;
    }
    let raf = 0;
    const update = () => {
      raf = 0;
      const p = Math.min(1, Math.max(0, window.scrollY / (node.offsetHeight * 0.75)));
      node.style.setProperty('--p', p.toFixed(3));
    };
    const onScroll = () => {
      if (!raf) {
        raf = requestAnimationFrame(update);
      }
    };
    update();
    window.addEventListener('scroll', onScroll, {passive: true});
    return () => {
      window.removeEventListener('scroll', onScroll);
      if (raf) {
        cancelAnimationFrame(raf);
      }
    };
  }, [ref]);
}

/** Scrolls to the URL hash once the page has mounted (anchors sit below the fold). */
export function useHashScroll() {
  useEffect(() => {
    const id = window.location.hash.slice(1);
    if (!id) {
      return;
    }
    const timer = window.setTimeout(() => {
      document.getElementById(id)?.scrollIntoView({block: 'start'});
    }, 300);
    return () => window.clearTimeout(timer);
  }, []);
}

/** Tokens wrapper: light/dark body, black hero and bands. */
export function Page({children}: {children: ReactNode}): ReactNode {
  return <div className={styles.page}>{children}</div>;
}

/** Invisible scroll target that clears the sticky navbar. */
export function Anchor({id}: {id: string}): ReactNode {
  return <div id={id} className={styles.anchor} />;
}

export function Section({
  eyebrow,
  title,
  lede,
  children,
  tint,
  wide,
  id,
}: {
  eyebrow: string;
  title: string;
  lede?: string;
  children: ReactNode;
  tint?: boolean;
  wide?: boolean;
  id?: string;
}): ReactNode {
  return (
    <>
      {id && <Anchor id={id} />}
      <section className={clsx(styles.section, tint && styles.sectionTint)}>
        <div className={clsx(styles.wrap, wide && styles.wrapWide)}>
          <Reveal className={clsx(!lede && styles.headGap)}>
            <p className={styles.eyebrow}>{eyebrow}</p>
            <Heading as="h2" className={styles.title}>
              {title}
            </Heading>
            {lede && <p className={styles.lede}>{lede}</p>}
          </Reveal>
          {children}
        </div>
      </section>
    </>
  );
}

export function HeroShell({
  eyebrow,
  title,
  accent,
  sub,
  buttons,
  stats,
  cueHref,
  cueLabel,
}: {
  eyebrow: string;
  title: ReactNode;
  /** Second line of the headline, painted with the gradient. */
  accent: ReactNode;
  sub: string;
  buttons: ReactNode;
  stats: readonly (readonly [string, string])[];
  cueHref: string;
  cueLabel: string;
}): ReactNode {
  const ref = useRef<HTMLElement>(null);
  useHeroProgress(ref);
  return (
    <header className={styles.hero} ref={ref}>
      <div className={styles.heroGlow} aria-hidden />
      <div className={styles.heroInner}>
        <p className={clsx(styles.heroEyebrow, styles.rise)}>{eyebrow}</p>
        <Heading as="h1" className={clsx(styles.heroTitle, styles.rise, styles.rise2)}>
          {title}
          <br />
          <span className={styles.heroAccent}>{accent}</span>
        </Heading>
        <p className={clsx(styles.heroSub, styles.rise, styles.rise3)}>{sub}</p>
        <div className={clsx(styles.heroBtns, styles.rise, styles.rise4)}>{buttons}</div>
        <ul className={clsx(styles.heroStats, styles.rise, styles.rise5)}>
          {stats.map(([n, label]) => (
            <li key={label}>
              <b>{n}</b>
              <span>{label}</span>
            </li>
          ))}
        </ul>
      </div>
      <a className={styles.scrollCue} href={cueHref} aria-label={cueLabel}>
        <span />
      </a>
    </header>
  );
}

/** Black band with the things we do not claim. */
export function HonestyBand({items}: {items: ReactNode[]}): ReactNode {
  return (
    <section className={clsx(styles.black, styles.honesty)}>
      <div className={styles.wrap}>
        <Reveal>
          <p className={styles.eyebrow}>Honesty</p>
          <Heading as="h2" className={clsx(styles.title, styles.onDark)}>
            What we don’t claim yet.
          </Heading>
          <ul className={styles.honestyList}>
            {items.map((item, i) => (
              <li key={i}>{item}</li>
            ))}
          </ul>
        </Reveal>
      </div>
    </section>
  );
}

/** Closing black band with a big line and a button pair. */
export function CtaBand({title, children}: {title: string; children: ReactNode}): ReactNode {
  return (
    <section className={clsx(styles.black, styles.cta)}>
      <Reveal>
        <Heading as="h2" className={styles.ctaTitle}>
          {title}
        </Heading>
        <div className={clsx(styles.btnrow, styles.center)}>{children}</div>
      </Reveal>
    </section>
  );
}
