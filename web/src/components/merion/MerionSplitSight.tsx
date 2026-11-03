// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/** Full-bleed Merion cell viewport — split-sight metaphor (a11y vs pixels). No Muse chrome. */
export default function MerionSplitSight() {
  return (
    <div className="merion-mkt-sight" aria-hidden>
      <div className="merion-mkt-sight-plane merion-mkt-sight-agent">
        <div className="merion-mkt-sight-tag">Agent</div>
        <pre className="merion-mkt-sight-a11y">{`heading "Vendor SOW"
  @e1 button "Download PDF"
  @e2 textbox "Notes"
  — a11y refs only`}</pre>
      </div>
      <div className="merion-mkt-sight-plane merion-mkt-sight-ops">
        <div className="merion-mkt-sight-tag">Operator</div>
        <div className="merion-mkt-sight-pixels">
          <div className="merion-mkt-sight-scan" />
          <span>pixels · tabs · screencast</span>
        </div>
      </div>
    </div>
  )
}
