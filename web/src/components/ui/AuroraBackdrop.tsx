// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/** A slow drifting brand-and-cool glow behind a hero or card. Decorative: it is aria-hidden, never carries meaning, and
    stops for reduced motion and when the tab is hidden. Put it first inside a `relative overflow-hidden` parent. */
export function AuroraBackdrop() {
  return <div className="zf-aurora" aria-hidden="true" />
}
