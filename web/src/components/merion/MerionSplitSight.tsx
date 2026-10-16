// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/** Full-bleed Merion cell viewport — split-sight metaphor (a11y vs pixels). No Muse chrome. */
export default function MerionSplitSight() {
  return (
    <div className="keep-mkt-sight" aria-hidden>
      <div className="keep-mkt-sight-plane keep-mkt-sight-agent">
        <div className="keep-mkt-sight-tag">Agent</div>
        <pre className="keep-mkt-sight-a11y">{`heading "Vendor SOW"
  @e1 button "Download PDF"
  @e2 textbox "Notes"
  — a11y refs only`}</pre>
      </div>
      <div className="keep-mkt-sight-plane keep-mkt-sight-ops">
        <div className="keep-mkt-sight-tag">Operator</div>
        <div className="keep-mkt-sight-pixels">
          <div className="keep-mkt-sight-scan" />
          <span>pixels · tabs · screencast</span>
        </div>
      </div>
    </div>
  )
}
