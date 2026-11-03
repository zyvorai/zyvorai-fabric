# Bold, dark-first UX: foundation

**Status:** phase 0 built (tokens, primitives, seeded mock, gate). Phases 1 to 3 (Merion pages, console shell and dashboard, sign-in and marketing) are still to do.

The [UX plan](ux-plan.md) made the console calm and consistent. This work makes it bold: dark first, depth and glow, graphics that carry meaning, and motion that is purposeful. It builds on the same token system, motion variables and accessibility gate.

## Rules

- **Dark is the default** for a first visit (`index.html`); a stored choice, either way, still wins. Light stays fully supported and the gate checks both.
- **Inline SVG and CSS only**, on the `--zf-*` tokens. No new runtime dependency.
- **Reduced motion:** every animation has a `prefers-reduced-motion` fallback, and `main.tsx` sets `data-paused` on `<html>` while the tab is hidden so the animations stop.
- **Colour is never the only signal.** A state shown by a halo or a dot also has text (`StatusPulse` requires a `label`; `Donut` carries an sr-only table).
- Text colours are tokens chosen for 4.5:1 on their usual backgrounds in both themes (`--zf-danger-text`, `--zf-warn-text`, `--zf-proof-text`, `--zf-brand-text`). Use them for text, not the saturated state colours.

## Tokens (`web/src/styles/main.css`)

Brand orange (`--zf-brand`) and a cool secondary (`--zf-cool`); state colours for proof, warning and danger; a shadow scale (`--zf-shadow-1..3`); halos (`--zf-glow-brand|proof|warn|danger`); a card sheen; a larger radius (`--zf-radius-lg`). New keyframes: `zf-flow`, `zf-ring`, `zf-draw`, `zf-aurora-drift`.

## Primitives (`web/src/components/ui`)

`GlowCard` (tone, interactive), `AuroraBackdrop`, `StatusPulse`, `AnimatedNumber`, `Donut`, `FlowLine`, `Illustration` (no-vms, no-sessions, all-clear, sealed). `/app/ui-kit` shows them all; it is not in the navigation and exists to look at, screenshot and axe-check them.

## Seeded mock

`web/mock-data.mjs` is shared by `npm run dev:mock` and the screenshot/a11y harness, so pages render populated: six VMs, five sessions, and a session cockpit with a pending changeset, a failed apply and a budget near its cap. The baseline images used to be all empty states, which hid how tables and cards look. With data, the gate found real contrast failures in the VM cards (tags, Start, Stop and Pause buttons, error text, CPU and memory percentages), now fixed.

## Checks

`npm run typecheck`, `npm run lint`, `npm test`, `npm run build`, `npm run a11y` (22 pages, light and dark, 1440 and 390 px), and `node baseline-shots.mjs <dir>` for screenshots.
