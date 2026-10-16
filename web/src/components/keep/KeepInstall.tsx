// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react'
import marketing from '../../../../docs/merion/marketing.json'

/** Tabbed, copy-to-clipboard install strip. Steps come from docs/merion/marketing.json. */
export default function KeepInstall() {
  const [tab, setTab] = useState(0)
  const [copied, setCopied] = useState(false)
  const step = marketing.install[tab]
  const text = step.commands.join('\n')

  const copy = () => {
    navigator.clipboard
      ?.writeText(text)
      .then(() => {
        setCopied(true)
        window.setTimeout(() => setCopied(false), 1500)
      })
      .catch(() => {})
  }

  return (
    <div className="keep-mkt-install" id="install">
      <div className="keep-mkt-install-tabs" role="tablist">
        {marketing.install.map((s, i) => (
          <button
            key={s.label}
            role="tab"
            aria-selected={i === tab}
            className={i === tab ? 'is-on' : undefined}
            onClick={() => setTab(i)}
          >
            {s.label}
          </button>
        ))}
      </div>
      <div className="keep-mkt-install-code">
        <pre>
          <code>{text}</code>
        </pre>
        <button className="keep-mkt-install-copy" onClick={copy} aria-label="Copy commands">
          {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
      <p className="keep-mkt-install-note">{step.note}</p>
    </div>
  )
}
