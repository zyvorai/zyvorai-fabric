import {useState} from 'react';
import type {ReactNode} from 'react';
import clsx from 'clsx';
import styles from './styles.module.css';

type Beat = {
  id: string;
  title: string;
  body: string;
  visual: ReactNode;
};

function Rule({ok, children}: {ok: boolean; children: ReactNode}) {
  return (
    <li className={clsx(styles.rule, ok ? styles.ruleOk : styles.ruleNo)}>
      <span aria-hidden="true">{ok ? '✓' : '✕'}</span>
      {children}
    </li>
  );
}

const BEATS: Beat[] = [
  {
    id: 'cell',
    title: 'A sealed computer',
    body: 'The agent works in its own microVM with its own kernel. It cannot reach your machine, and the network rules live on the host, outside its reach.',
    visual: (
      <div className={styles.panel}>
        <p className={styles.tag}>Enforced by the host</p>
        <ul className={styles.rules}>
          <Rule ok>Gateway reachable</Rule>
          <Rule ok={false}>QUIC and WebRTC</Rule>
          <Rule ok={false}>Cloud metadata</Rule>
          <Rule ok={false}>Public DNS</Rule>
        </ul>
        <p className={styles.foot}>If a connection slips through anyway, the session freezes.</p>
      </div>
    ),
  },
  {
    id: 'keys',
    title: 'You hold the keys',
    body: 'Policy is a signed file you can diff in git. Passwords stay in a vault and are injected on the host, so the agent never sees a real secret.',
    visual: (
      <div className={styles.panel}>
        <p className={styles.tag}>Yours</p>
        <div className={styles.rowCard}>
          <strong>Policy</strong>
          <span>Signed. Diff it in git.</span>
        </div>
        <div className={styles.rowCard}>
          <strong>Vault</strong>
          <span>Injected on the host. Never in the cell.</span>
        </div>
        <div className={styles.rowCard}>
          <strong>Approvals</strong>
          <span>Yours alone to give.</span>
        </div>
      </div>
    ),
  },
  {
    id: 'approve',
    title: 'Approve what matters',
    body: 'Buying, sending and deleting are approved out of band, in your cockpit, never in the chat.',
    visual: (
      <div className={styles.panel}>
        <p className={styles.tag}>Waiting for you</p>
        <div className={styles.approval}>
          <strong>Send the brief by email?</strong>
          <span>The agent asked. Nothing leaves until you approve.</span>
          <div className={styles.approvalBtns}>
            <span className={styles.approve}>Approve</span>
            <span className={styles.deny}>Deny</span>
          </div>
        </div>
      </div>
    ),
  },
  {
    id: 'sight',
    title: 'See everything',
    body: 'The agent reads a structured outline of the page. You watch the real pixels, follow every decision, and can pause to step in.',
    visual: (
      <div className={styles.split}>
        <div className={styles.panel}>
          <p className={styles.tag}>Agent sees</p>
          <pre className={styles.code}>{`heading "Vendor SOW"
  @e1 button "Download PDF"
  @e2 textbox "Notes"`}</pre>
        </div>
        <div className={clsx(styles.panel, styles.pixels)}>
          <p className={styles.tag}>You see</p>
          <span className={styles.bar} />
          <span className={clsx(styles.bar, styles.barShort)} />
          <span className={styles.pill}>Download PDF</span>
        </div>
      </div>
    ),
  },
  {
    id: 'proof',
    title: 'Proof, not promises',
    body: 'Brief a vendor PDF and the cockpit counts outbound connections from Merion’s own audit journal, enforced on the host.',
    visual: (
      <div className={styles.panel}>
        <p className={styles.tag}>PDF brief demo</p>
        <p className={styles.big}>
          0<span> outbound connections</span>
        </p>
        <p className={styles.foot}>Counted from the audit journal. Enforced on the host.</p>
      </div>
    ),
  },
];

/** Muse's pattern: a short accordion on the left, one big visual on the right. */
export default function MerionStory(): ReactNode {
  const [active, setActive] = useState(0);
  return (
    <div className={styles.story}>
      <div className={styles.list} role="tablist" aria-orientation="vertical">
        {BEATS.map((b, i) => (
          <div key={b.id} className={clsx(styles.item, i === active && styles.itemOn)}>
            <button
              type="button"
              role="tab"
              id={`merion-tab-${b.id}`}
              aria-selected={i === active}
              aria-controls="merion-visual"
              className={styles.itemHead}
              onClick={() => setActive(i)}>
              <span>{b.title}</span>
              <span aria-hidden="true" className={styles.plus}>
                {i === active ? '–' : '+'}
              </span>
            </button>
            {i === active ? <p className={styles.itemBody}>{b.body}</p> : null}
          </div>
        ))}
      </div>
      <div
        id="merion-visual"
        role="tabpanel"
        aria-labelledby={`merion-tab-${BEATS[active].id}`}
        className={styles.stage}
        key={BEATS[active].id}>
        {BEATS[active].visual}
      </div>
    </div>
  );
}
