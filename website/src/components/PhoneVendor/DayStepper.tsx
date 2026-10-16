import {useState} from 'react';
import type {KeyboardEvent, ReactNode} from 'react';
import clsx from 'clsx';
import {DAY} from '../../data/phoneVendor';
import type {Step} from '../../data/phoneVendor';
import styles from './styles.module.css';

/** What the phone shows at each step. Illustrative: the Android app is not built. */
function Screen({kind}: {kind: Step['screen']}): ReactNode {
  switch (kind) {
    case 'signin':
      return (
        <>
          <p className={styles.sTitle}>Sign in</p>
          <div className={styles.field}>Vendor account</div>
          <div className={styles.field}>••••••••</div>
          <div className={styles.btn}>Continue</div>
          <p className={styles.sNote}>Your agent’s computer is placed in your region.</p>
        </>
      );
    case 'enrol':
      return (
        <>
          <p className={styles.sTitle}>Set up this phone</p>
          <div className={styles.keyRow}>
            <span className={styles.keyIcon} aria-hidden>
              ⚿
            </span>
            <span>Key created in the keystore</span>
          </div>
          <p className={styles.sNote}>The private key never leaves this phone. Only the public key is sent.</p>
          <div className={styles.btn}>Done</div>
        </>
      );
    case 'ask':
      return (
        <>
          <p className={styles.sTitle}>Ask your agent</p>
          <div className={styles.bubble}>Summarise these two contracts</div>
          <div className={styles.working}>
            <span className={styles.dot} />
            Working in a sealed cell
          </div>
          <p className={styles.sNote}>0 outbound connections so far</p>
        </>
      );
    case 'approve':
      return (
        <>
          <p className={styles.sTitle}>Approve?</p>
          <div className={styles.ask}>
            <b>Send this email</b>
            <span>To: legal@example.com</span>
            <span>Subject: Contract summary</span>
          </div>
          <div className={styles.twoBtn}>
            <div className={styles.btnGhost}>Deny</div>
            <div className={styles.btn}>Approve</div>
          </div>
          <p className={styles.sNote}>Confirm with fingerprint. The phone signs this decision.</p>
        </>
      );
    default:
      return (
        <>
          <p className={styles.sTitle}>Done</p>
          <div className={styles.result}>2 files summarised</div>
          <div className={styles.result}>Email sent, approved on this phone</div>
          <p className={styles.sNote}>Audit trail: every approval and signature check</p>
        </>
      );
  }
}

export default function DayStepper(): ReactNode {
  const [i, setI] = useState(0);
  const step = DAY[i];

  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowRight') {
      e.preventDefault();
      setI((n) => Math.min(DAY.length - 1, n + 1));
    } else if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') {
      e.preventDefault();
      setI((n) => Math.max(0, n - 1));
    }
  };

  return (
    <div className={styles.day}>
      <div className={styles.dayList} role="tablist" aria-label="A user’s day" onKeyDown={onKey}>
        {DAY.map((s, n) => (
          <button
            key={s.id}
            role="tab"
            id={`day-tab-${s.id}`}
            aria-selected={n === i}
            aria-controls="day-panel"
            tabIndex={n === i ? 0 : -1}
            className={clsx(styles.dayTab, n === i && styles.dayTabOn)}
            onClick={() => setI(n)}>
            <span className={styles.dayNum}>{n + 1}</span>
            {s.title}
          </button>
        ))}
      </div>
      <div className={styles.dayDetail} role="tabpanel" id="day-panel" aria-labelledby={`day-tab-${step.id}`}>
        <span className={styles.who}>{step.who}</span>
        <p>{step.body}</p>
        <code className={styles.call}>{step.call}</code>
      </div>
      <div className={styles.phone} aria-hidden="true">
        <div className={styles.screen} key={step.id}>
          <Screen kind={step.screen} />
        </div>
      </div>
      <p className={styles.mockNote}>Mock-up of what a vendor’s app would show. Merion does not ship a phone app.</p>
    </div>
  );
}
