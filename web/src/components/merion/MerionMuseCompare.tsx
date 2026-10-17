// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import marketing from '../../../../docs/merion/marketing.json'

/**
 * Meta Muse vs Merion (+ Fabric / FluxVM) — marketing only. Rows live in
 * docs/merion/marketing.json, shared with the Merion README and the docs site; every
 * row is stated in docs/merion/MERION.md.
 */
const ROWS: { label: string; muse: string; merion: string }[] = marketing.compare

export default function MerionMuseCompare() {
  return (
    <div className="merion-mkt-versus">
      <p className="merion-mkt-stack-lede">
        Muse got the threat model right: treat the model as compromised the moment it reads a
        webpage. Merion is the version you run, read and take with you.
      </p>

      <ol className="merion-mkt-stack" aria-label="Who does what">
        <li>
          <span className="merion-mkt-stack-name">Meta Muse</span>
          <span className="merion-mkt-stack-role">
            A closed personal-agent product that runs on Meta’s cloud.
          </span>
        </li>
        <li>
          <span className="merion-mkt-stack-name">Merion</span>
          <span className="merion-mkt-stack-role">
            The open product layer: policy, vault, approvals, browser and demos.
          </span>
        </li>
        <li>
          <span className="merion-mkt-stack-name">Fabric</span>
          <span className="merion-mkt-stack-role">
            The control plane: console, sign-in, agents and sessions, and the front door to Merion’s
            APIs.
          </span>
        </li>
        <li>
          <span className="merion-mkt-stack-name">FluxVM</span>
          <span className="merion-mkt-stack-role">
            The hypervisor: it runs the cell and enforces the network rules on the host.
          </span>
        </li>
      </ol>
      <p className="merion-mkt-stage-line">
        Merion is not a third hypervisor. Merion is Fabric’s agent runtime plus a FluxVM cell.
      </p>

      <div className="merion-mkt-versus-head" aria-hidden>
        <span />
        <span>Meta Muse</span>
        <span>Merion</span>
      </div>
      <ul className="merion-mkt-versus-rows">
        {ROWS.map((r) => (
          <li key={r.label}>
            <span className="merion-mkt-versus-label">{r.label}</span>
            <span className="merion-mkt-versus-muse">{r.muse}</span>
            <span className="merion-mkt-versus-merion">{r.merion}</span>
          </li>
        ))}
      </ul>
    </div>
  )
}
