# Bold, dark-first UX: foundation

**Status:** phase 0 (tokens, primitives, seeded mock, gate), phase 1 (the Merion pages) and phase 2 (the dashboard and shell touches) are built. Phase 3 (sign-in and marketing) is still to do.

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

## Phase 1: the Merion session page

The page used to be one 3,200 px column of plain cards with the approval near the bottom. Now it is two columns: what needs a person first (the approval, then failed applies, then what happened), and a rail of proof and context (egress seal, model budget, attestation, goal, evidence). On a phone the proof and budget move up under the approval.

- **`EgressSeal`**: "0 CONNECT, sealed" in the proof colour for a reported zero, red and "not sealed" with the count otherwise, and grey "not reported" when the runtime did not send a count, which is never shown as sealed. Drop reasons are bars when the runtime sends them in a known shape.
- **`ChangesetDiff`**: command and exit code, a proportional added/modified/deleted bar with counts, the files by kind, "Replayed on approval" next to "Recorded, never replayed", the masking note, and the expiry. The summary now carries `replayable` from `side_effects`.
- **`BudgetGauges`**: one meter per cap (`role="meter"`), amber from the warning share, red at the cap, with the reading in text.
- **`RunTimeline`**: the run as a vertical timeline from the report. Each entry is worded Done, Held or Failed as well as coloured; failure wins, so "FluxVM could not apply it" is never drawn as done.
- Pure helpers (`lib/merionView.ts`, `lib/changeset.ts`) have their own tests; the components are rendered with `react-dom/server` in `components/merion/__tests__`.

### Replay, history and home

- **`ReplayScrubber`** (in `SessionReplay`): a track with a marker per recorded step, coloured by how it ended, over a native range input; play/pause steps forward once a second and stops on the last step; the step the scrubber is on is described in words with a Done/Held/Failed chip; the signature verdict is a chip. Markers are spaced evenly by step, not by time, because the range input moves a step at a time and a time-spaced marker would not be where the thumb lands. Time offsets are printed instead.
- **`StatusChip`**: a status as a dot and its words, coloured from the words (`timelineTone`) unless given. Used for the audit phase and the approval status on the history page.
- **`SealedPipeline`** on the Merion home: your file, the sealed cell, the extraction, the artifact. The working node pulses; done is green with a check; a failure is red with a cross and never shows a finished artifact. **`DropZone`** is a dashed target that is also a button; the picker stays the keyboard route.
- The history page is now covered by the gate (it is in `PAGES` with seeded runs, approvals, audit and triggers).

Not done: the history Runs and Triggers tabs are still plain lists; an expiry countdown ring (the runtime sends a deadline but not the full lifetime, so there is nothing to draw the ring against), a content diff (contents are masked on the host by design), and the replay scrubber, history table and Merion home, which are the next part of phase 1.

## Phase 2: the dashboard and the shell

- **`LiveFabric`** replaces the decorative constellation in the dashboard hero: a core with one node per real VM (up to eight, running first, then failed, paused, the rest), each in the colour of its state. A running VM's line moves; the others are still. The svg carries one sentence ("6 virtual machines: 3 running, ...") and each node has a title; the VM table below still lists every VM. Hidden on phones.
- **Fleet card**: a `Donut` of running, paused, stopped, failed and other, with the same numbers listed in text; the card gets a danger halo only when a VM has failed.
- **Per-VM load** in the table: CPU and memory bars (`LoadBar`, `role="meter"`, amber from 50%, red from 80%) from the metrics the page already polls for the charts. There is no per-VM history, so there are no per-VM sparklines; a VM with no reading shows a dash, never zero.
- **Colour language**: only a failed VM is red. `StatusBadge` now draws `stopped` as a neutral grey (it was red, as if an error); `stateTone` in `lib/fleet.ts` is the one mapping the graphic, donut and bars share.
- **Onboarding** (`GettingStarted`): an illustration, an icon per step, numbered steps, and the first one carries the brand halo.
- **Shell**: the active sidebar item gets a brand edge (the weight and text still mark it), and the "Live" pill uses the shared pulse.

## The gate and crashed pages

`openPage` in `page-harness.mjs` now fails when a page renders the error boundary. Before that, a page that threw looked clean to axe, because an error page has no violations. Five pages (`backups`, `analytics`, `audit`, `system`, `migrations`) throw against the mock's empty answers; they are listed in `UNSEEDED_PAGES`, are not checked, and `npm run a11y` prints that. Seed each one and move it back into `PAGES`.

## Checks

`npm run typecheck`, `npm run lint`, `npm test`, `npm run build`, `npm run a11y` (22 pages, light and dark, 1440 and 390 px), and `node baseline-shots.mjs <dir>` for screenshots.
