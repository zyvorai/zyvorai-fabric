// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

const NODES = [
  { x: 168, y: 34, color: '#60a5fa' },
  { x: 176, y: 92, color: '#34d399' },
  { x: 150, y: 140, color: '#a78bfa' },
  { x: 96, y: 156, color: '#f472b6' },
  { x: 46, y: 132, color: '#22d3ee' },
  { x: 22, y: 78, color: '#fb923c' },
]
const CORE = { x: 100, y: 90 }

interface FabricGraphicProps {
  /** Fades everything toward decorative/ambient use rather than a focal illustration. */
  ambient?: boolean
}

/** Abstract "fabric" constellation -- a core node radiating out to satellite VM
    nodes. Pure inline SVG, theme-agnostic, no external assets. Reused as a small
    focal graphic (onboarding) and as a large faint ambient backdrop (dashboard hero). */
export function FabricGraphic({ ambient = false }: FabricGraphicProps) {
  return (
    <svg viewBox="0 0 200 176" className="w-full h-full" aria-hidden="true" style={ambient ? { opacity: 0.5 } : undefined}>
      <defs>
        <radialGradient id="fabricCoreGlow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="#93c5fd" stopOpacity="0.9" />
          <stop offset="60%" stopColor="#60a5fa" stopOpacity="0.35" />
          <stop offset="100%" stopColor="#60a5fa" stopOpacity="0" />
        </radialGradient>
        {NODES.map((n, i) => (
          <radialGradient key={i} id={`fabricNode${i}`} cx="35%" cy="30%" r="70%">
            <stop offset="0%" stopColor="#fff" stopOpacity="0.9" />
            <stop offset="35%" stopColor={n.color} stopOpacity="0.95" />
            <stop offset="100%" stopColor={n.color} stopOpacity="0.6" />
          </radialGradient>
        ))}
      </defs>
      {NODES.map((n, i) => (
        <line key={i} x1={CORE.x} y1={CORE.y} x2={n.x} y2={n.y} stroke={n.color} strokeOpacity="0.35" strokeWidth="1.5" />
      ))}
      <circle cx={CORE.x} cy={CORE.y} r="34" fill="url(#fabricCoreGlow)" />
      <circle cx={CORE.x} cy={CORE.y} r="9" fill="#dbeafe" className="fabric-core-pulse" />
      {NODES.map((n, i) => (
        <circle key={i} cx={n.x} cy={n.y} r="6" fill={`url(#fabricNode${i})`} className="fabric-node-pulse" style={{ animationDelay: `${i * 0.35}s` }} />
      ))}
    </svg>
  )
}
