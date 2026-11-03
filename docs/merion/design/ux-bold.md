# Bold, dark-first UX: foundation

**Status:** all three phases are built: phase 0 (tokens, primitives, seeded mock, gate), phase 1 (the Merion pages), phase 2 (the dashboard and shell touches) and phase 3 (sign-in and the marketing home).

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

## Phase 3: sign-in and the marketing home

- **Marketing home**: the grey placeholder "console" is now `ConsoleMock`, a picture of the dashboard built from the same pieces as the real one (the fleet map, the donut, the load bars) on an invented fleet (`lib/sampleFleet.ts`), in a browser frame and labelled "Example data". A screen reader gets one description of it, not its parts. The hero has the aurora behind it, and the "See the platform" and "Meet Merion" buttons are centred under their text (they were stuck at the left edge).
- **Sign-in**: the fleet map sits behind the credentials card as a faint, `aria-hidden` backdrop (`PremiumLoginShell`'s `backdrop` prop). It is behind the card, not in the hero chapter, because the username field is focused on load and that scrolls the hero away: people land on the card. It is sized so the ring of nodes falls outside the card and its text, is off below 700 px wide, and its flow lines stop for reduced motion. `LiveFabric` has a `decorative` mode (no role, label or titles) for this.

### The other marketing pages

Product, Platform and Security were headlines over blocks of text. They now share a hero with the aurora (`MarketingHero`, which fades the glow out toward its bottom edge) and end in the same inverted call-to-action band as the home page (`CtaBand`).
- **Product**: a `FeatureRow` per feature (words, chips, a picture), alternating sides on wide screens and stacked on a phone, the words always first in reading order. The pictures are `components/marketing/visuals.tsx`: the live fleet map, a floating IP over a bridge over three VMs, a primary volume replicating with a backup, and the finished sealed-cell pipeline with "0 CONNECT, sealed". They are decorative (`aria-hidden`); the words and chips say everything. The chips are the feature lists from the existing copy, split into pills.
- **Platform**: the four ways in drawn around one daemon, with an icon card per surface.
- **Security**: a shield, and three pillars (Access, Network security, Compliance) with their items as pills.
- The Merion page is already a rich page with its own storyboard, comparison table and install box, so it was left alone (it passes the marketing gate, and the contrast and keyboard fixes from the sign-in change covered it).

The marketing gate then found a real bug: the sealed-cell pipeline made `/product` scroll sideways on a phone, because its connector line had an intrinsic width that made every step 104px wide. The connectors now shrink, which also tightens the Merion home page.

Not done: the Docusaurus `website/` is unchanged.

## Console pages: Virtual Machines

The VM list got the same treatment as the dashboard, on the same shared pieces:
- **`FleetBar`** replaces the text line above the list with a proportional bar and a count per state. The old line counted running, stopped and paused and left failed VMs out entirely; the bar counts every VM, and it says "N of M" when a search or tag filter narrows the list. Only a failed VM is red (the old line drew stopped red).
- **Live load on running cards** (`VMLoad`): CPU and memory bars from the same per-VM metrics poll the dashboard uses (every 10 s, running VMs only). `vmLoadFrom` in `lib/fleet.ts` is the one place memory bytes are set against the VM's own allocation; the dashboard uses it too. A stopped VM or one with no reading shows nothing, never 0%.
- A failed VM's card has a danger edge. The empty state shows the illustration (`EmptyState` has a new optional `illustration` prop).

Not done here: the table view, VM details, create and storage pages.

## Console pages: Network

- **`NetworkMap`** ("How it connects") draws four columns: interfaces; bonds, VLANs, VXLANs and macvtaps; bridges; taps. A line joins two things where the configuration does: a bond's members, a VLAN, macvtap or VXLAN to its parent, a network file's `bridge` or `bond`, a tap to its bridge (`lib/networkMap.ts`, pure and tested; nothing is inferred). A node is outlined by its networkd state (green when clearly working, amber for degraded, grey otherwise; nothing is red, because a down link is not an error); a line moves only when both ends are up; something referred to but not listed is drawn dashed and labelled "(not listed)". The picture is `aria-hidden`; the same connections are a text list for a screen reader. On a narrow screen it scrolls inside its own keyboard-reachable box. It draws nothing when nothing is configured.
- **Physical NICs are always drawn**, even when no bond, VLAN or bridge refers to them (the Network page passes the network files it classes as physical, at most 12, and the card says how many more were not drawn). Found by opening the page on the lab host, which has two bridges and a VXLAN but no bonds, VLANs or taps: the map had no lines and an empty Interfaces column.
- **Two state vocabularies.** Links report networkd's own states (`routable`, `carrier`, `enslaved`, `degraded`, `no-carrier`), while bridges, VXLANs and network files report plain `up`, `down` and `unknown`, which the invented seed never used. `netTone` knows both; before this, a real host's working interface that reported `up` was drawn grey. Seed data written from the types alone cannot show this; real responses can.
- The seed now has networkd data (links, bonds, bridges, VLANs, macvtaps, taps, VXLANs, network files, port forwards), so the page and its tables render populated in the gate.

### The gate now clicks through tabs

axe only sees the tab that is open when a page loads, so a page's other tabs were never checked. `PAGE_TABS` in `page-harness.mjs` lists a page's tabs; `a11y-check.mjs` clicks each (a button with exactly that text) and scans again. Network is the first. That found, in tabs nobody had checked: "Create" buttons in a different saturated colour per tab (cyan, green, orange, pink, indigo, purple, yellow) with theme-dependent text that could not pass in both themes, now the one primary button colour; green and red text and badges at 3.3 to 4.4:1; and a sort control with no accessible name. To prove the gate, one fixed badge was put back and the gate named it.

Not done here: the table view of VMs, VM details, create and storage pages; the Docusaurus `website/`.

### What clicking through 14 more pages' tabs found

Three tabs **crashed the page** against the mock (`network` > SR-IOV and `notifications` > History, whose paths do not end in "s", so the mock answered an object where the API sends a list; `edge-dataplane` > Services, whose status object the mock left without `north_south_interfaces`): all three were mock gaps, now seeded, and they had hidden every tab after them. (A path that does not end in "s" is the usual cause: the mock's default answer is `[]` only for paths that do.) Real problems: three terminal-style editors with no accessible name (`TerminalTextarea` now names itself from its frame title, which fixes every use), a library select with no name, dark-theme green, red and amber text on themed cards and table cells (system, lifecycle), chips and table cells at 4.25:1 on a faint tint (network status), and the "Colorful" packet-flow panel, which is meant to be dark in either theme but was remapped to a white panel with near-white text in light theme by the stylesheet's rule that remaps hard-coded colour classes (inline styles are not remapped). ### VM details

VM details is in the gate as the seeded route `vms/web-01` (31 + 1 seeded pages, 12 tabs). It found, and this fixes: two tabs that crashed against the mock (Advanced, whose boot, display, CPU-model, watchdog, serial, firmware and CPU-affinity endpoints were unseeded, and Logs, whose paths the mock answered with `{}`); **14 selects with no accessible name** on the Advanced, Dataplane and port-forward forms (each now carries its heading's text as its label); danger-red text on a fixed pale red button background (3.2:1; fixed dark red now, since the background is fixed), amber text meant for a dark surface on a pale amber note (1.4:1), and green and amber metric numbers on themed cards in dark theme. A pattern fixed across the codebase: link-blue text on a faint link-coloured tint (the "active toggle" look) cannot reach 4.5:1 in light theme, and cannot at any tint dark enough to look like a hover, so those 11 uses (VM details, the VM list, create VM) now use the theme's ink colour on the tint.

## The gate and crashed pages

`openPage` in `page-harness.mjs` fails when a page renders the error boundary. Before that, a page that threw looked clean to axe, because an error page has no violations. Five pages (`backups`, `analytics`, `audit`, `system`, `migrations`) threw against the mock's empty answers and so were never really checked; their endpoints are now seeded in `mock-data.mjs` and they are in `PAGES`, so every console page in the gate renders with data. `UNSEEDED_PAGES` is empty; if a page is added that the mock cannot feed, list it there (with a reason) rather than letting the gate pass an error page.

Seeding them found real problems, now fixed: the audit page's action and error text and counters used mid-tone green and red (3.4 to 4.4:1), the analytics time-range select had no accessible name (a critical axe finding), and the analytics "info" badge used the link colour on a pale blue that fails in dark theme.

## How much of the console the gate covers

The gate now covers 90 of the console's roughly 100 routes (the rest are detail pages with a route parameter), in light and dark, desktop and phone (`page-harness.mjs`):
- **`SEEDED_PAGES`** (31): the mock feeds them data, so they render populated and tables, cards and charts are checked.
- **`EMPTY_STATE_PAGES`** (59): they render fine against the mock's empty answers but have no seed data, so they are checked as empty states (structure, labels, and the contrast of what is drawn). Seed one in `mock-data.mjs` and move it to `SEEDED_PAGES` to check it with data. They settle for 0.7 s instead of 1.8 s, since there is nothing to wait for.
- **`UNSEEDED_PAGES`** (none now): a page that throws against the empty mock cannot be checked, and would be listed here, and printed by `npm run a11y` on every run, instead of letting the gate pass an error page. The last eight (certificates, compliance, drs, federation, lifecycle, migration-report, replication, site-recovery) were seeded in `mock-data.mjs`.
- **`PAGE_TABS`** (17 pages: vms/web-01, network, system, certificates, drs, lifecycle, replication, site-recovery, ai, content-library, distributed-storage, edge-dataplane, encryption, fault-tolerance, network-security, notifications, merion/history): a page's other tabs are clicked and scanned too, because axe only sees the tab that is open on load. The list is curated by hand (the tab labels are scattered across arrays, translations and components, and a heuristic that clicked "button groups" would also click Start and Delete). A label that carries a count ("Rules (3)", "Optimization1") is matched without it. Two guards stop a stale list passing silently: a configured tab the page does not have fails the gate (**"tab not found"**), and a click that throws the page into the error boundary fails it (**"tab crashed the page"**) and the page is reopened so the other tabs are still checked. An error page has no axe violations, so without the second guard a crashing tab looked clean. `--pages=a,b` runs just those pages while fixing them.

Widening it from about 25 pages found 36 violations in the 59 new ones: mid-tone green, amber and red text at 2.9 to 4.4:1 (the same three Tailwind classes in 55 files, now the contrast-safe tokens everywhere), three unlabelled sliders on the cost estimator, an unlabelled select on the event stream, a link-blue notification chip on a fixed pale background, and white text on the theme's ink colour in the timeline's active filter.

Populating the last eight found more: a warning-coloured progress bar with white text (2.05:1), a danger bar at 3.5:1, an unlabelled slider, a fixed pale panel whose heading turned near-white in dark theme, solid ink-coloured chips and avatars with white text (ink is light in dark theme, so the text must be the canvas colour), and table header cells at 4.25:1 on a tinted row (every table's header now uses the secondary text colour). One rule to remember: text colours like `text-emerald-700` are right on a fixed pale chip and wrong on a themed surface, so they cannot be swept globally; the `text-*-600` ones, which were used on themed surfaces, could.

The run takes about 7 minutes in one process, so `npm run a11y` runs the two themes at once (`a11y-parallel.mjs`, two processes on two ports).

## Checks

`npm run typecheck`, `npm run lint`, `npm test`, `npm run build`, `npm run a11y` (22 pages, light and dark, 1440 and 390 px), and `node baseline-shots.mjs <dir>` for screenshots.
