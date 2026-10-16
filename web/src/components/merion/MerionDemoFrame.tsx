// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/** Real output of scripts/merion-e2e.sh, condensed (public/merion/demo-static.svg). */
export default function MerionDemoFrame() {
  return (
    <figure className="keep-mkt-demo">
      <img
        src="/merion/demo-static.svg"
        alt="Real output of ./scripts/merion-e2e.sh: 40 checks passed, 0 failed"
        loading="lazy"
      />
      <figcaption>
        Real output of <code>./scripts/merion-e2e.sh</code>, condensed. No KVM needed.
      </figcaption>
    </figure>
  )
}
