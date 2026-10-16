import {useCallback, useEffect, useRef, useState} from 'react';
import type {ReactNode} from 'react';
import clsx from 'clsx';
import styles from './CockpitMock.module.css';

type Step = 'pending' | 'running' | 'blocked' | 'done';

/* Plan-step states are the ones in docs/merion/goals/README.md. The session
 * itself is illustrative — no real run is being streamed. */
const STEPS = ['Read the vendor SOW', 'Summarise the risks', 'Delete the draft notes'];

const FRAMES: {steps: Step[]; log: string[]; paused?: string; artifact?: boolean}[] = [
  {steps: ['running', 'pending', 'pending'], log: ['policy · allow read sow.pdf']},
  {
    steps: ['done', 'running', 'pending'],
    log: ['policy · allow read sow.pdf', 'model · summarise via BYO socket'],
  },
  {
    steps: ['done', 'done', 'blocked'],
    log: [
      'policy · allow read sow.pdf',
      'model · summarise via BYO socket',
      'ask · delete notes.md → approval requested',
    ],
    paused: 'Waiting for your approval — phone, key or tray',
  },
  {
    steps: ['done', 'done', 'running'],
    log: [
      'model · summarise via BYO socket',
      'ask · delete notes.md → approval requested',
      'approval · granted out-of-band',
    ],
  },
  {
    steps: ['done', 'done', 'done'],
    log: [
      'ask · delete notes.md → approval requested',
      'approval · granted out-of-band',
      'artifact · brief.md written',
    ],
    artifact: true,
  },
];

const LAST = FRAMES.length - 1;
const STEP_MS = 1700;

const GLYPH: Record<Step, string> = {pending: '○', running: '◔', blocked: '⏸', done: '✓'};

export default function CockpitMock(): ReactNode {
  const ref = useRef<HTMLDivElement>(null);
  const [frame, setFrame] = useState(0);
  const [playing, setPlaying] = useState(false);
  const started = useRef(false);

  const play = useCallback(() => {
    setFrame(0);
    setPlaying(true);
  }, []);

  // Start once, when the mock scrolls into view.
  useEffect(() => {
    const node = ref.current;
    if (!node) {
      return;
    }
    if (
      typeof IntersectionObserver === 'undefined' ||
      window.matchMedia('(prefers-reduced-motion: reduce)').matches
    ) {
      setFrame(LAST);
      return;
    }
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting && !started.current) {
          started.current = true;
          setPlaying(true);
          io.disconnect();
        }
      },
      {threshold: 0.35},
    );
    io.observe(node);
    return () => io.disconnect();
  }, []);

  useEffect(() => {
    if (!playing) {
      return;
    }
    if (frame >= LAST) {
      setPlaying(false);
      return;
    }
    const t = window.setTimeout(() => setFrame((f) => f + 1), STEP_MS);
    return () => window.clearTimeout(t);
  }, [playing, frame]);

  const f = FRAMES[frame];
  return (
    <div className={styles.mock} ref={ref}>
      <div
        role="img"
        aria-label="Illustrative Keep cockpit: a goal advances, pauses for approval, then finishes with zero egress connects.">
      <div className={styles.chrome} aria-hidden>
        <i /><i /><i />
        <span>/app/merion · session</span>
        <em>Illustrative</em>
      </div>

      <div className={styles.badge} aria-hidden>
        <code>evidence=software-test · browser=a11y-only · proxy=strict</code>
      </div>

      <div className={styles.grid} aria-hidden>
        <section className={styles.goal}>
          <h4>Goal · brief the SOW</h4>
          <ol>
            {STEPS.map((s, i) => (
              <li key={s} className={styles[f.steps[i]]}>
                <span className={styles.g}>{GLYPH[f.steps[i]]}</span>
                {s}
                <small>{f.steps[i]}</small>
              </li>
            ))}
          </ol>
          {f.paused && <p className={styles.pause}>{f.paused}</p>}
          {f.artifact && <p className={styles.art}>Artifact · brief.md</p>}
        </section>

        <section className={styles.count}>
          <h4>Egress</h4>
          <div className={styles.zero}>0</div>
          <code>egress_connects</code>
        </section>

        <section className={styles.log}>
          <h4>Last decisions</h4>
          <ul>
            {f.log.map((l) => (
              <li key={l} className={styles.row}>
                {l}
              </li>
            ))}
          </ul>
        </section>

        <section className={styles.split}>
          <h4>Split sight</h4>
          <div className={styles.planes}>
            <div className={styles.agent}>
              <span>Agent</span>
              <pre>{`heading "Vendor SOW"
  @e1 button "Download"
  — a11y refs only`}</pre>
            </div>
            <div className={styles.ops}>
              <span>Operator</span>
              <div className={styles.px}>
                <i className={clsx(styles.scan)} />
                pixels · tabs
              </div>
            </div>
          </div>
        </section>
      </div>
      </div>

      <div className={styles.foot}>
        <span>Scripted for illustration — real cockpit: <code>/app/merion</code></span>
        {frame >= LAST && (
          <button type="button" onClick={play}>
            Replay
          </button>
        )}
      </div>
    </div>
  );
}
