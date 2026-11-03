# UX plan: making the console and site feel like apple.com

**Status:** step 0 (`npm run dev:mock`) and the motion layer of step 5 are built; the rest is still plan. The [UX pass](ux-pass.md) shipped the shared motion, focus and skip-link rules. This plan is what comes next, in order, and what would tell us each step worked.

## Where we are (measured, not guessed)

- About 98 console page entries; **86 files use `PageHeader`**, so one change there reaches most page titles.
- Page titles are mostly 20px `h2`s of mixed weight; the shared rule already forces weight 600.
- **1,453 hardcoded colour utilities** (`bg-[#…]`, `text-[#…]`, `border-[#…]`) and 91 files using `bg-white` or `text-[#1d1d1f]`. Dark mode works because `main.css` redirects them, which hides the debt and breaks on any colour that is not on the redirect list.
- 83 text sizes of 10-11px.
- Confirmation uses the app's own dialog hook (not browser dialogs), so no gap there.
- **Not known:** how the signed-in pages actually look. They need a backend sign-in, and nothing here was seen at phone width.

## Baseline (taken with `web/baseline-shots.mjs`, images in `docs/ux/baseline/`)

20 pages × light/dark × 1440px/390px = 80 images, rendered from the production build with the mock API (so every list is empty).

- **Fixed:** all 20 pages scrolled sideways at 390px. Cause: `.zf-btn` sets `display`, which beats Tailwind's `hidden`, so the topbar's Search button never hid on phones. It now hides under 640px and the topbar gets tighter gaps. The Network tab bar scrolls inside itself, and the Create VM image links wrap. Re-run: no page scrolls sideways in any of the 80 captures.
- **Fixed:** phone page titles truncated ("Virt… Ma…"). `PageHeader` now wraps its actions below the title on phones, uses 26px titles there (32px from 640px up) and truncates only on wider screens.
- **Fixed (step 3):** 81 of 83 text sizes under 12px are now 12px (plus 5 marketing label sizes in `main.css`); only the two `⌘K` key chips stay small. Baseline re-run: still no sideways scroll.
- **Step 1:** only 7 page files lacked `PageHeader`. Edge Dataplane now uses it; Console and VM details keep their own header (status, back controls) with the shared title style; Dashboard is the hero; Login, SignIn and NotFound are standalone.
- **Done (step 2):** `web/codemod-tokens.py` mapped the literal hex utilities and `bg-white` onto the `--zf-*` tokens across 95 files, including opacity tints (`bg-[#0066cc]/10` is now `bg-[var(--zf-link)]/10`), and the dark-mode redirect block in `main.css` is deleted. Light mode is pixel-identical on the baseline apart from ~0.1% noise. Side effects, all in dark mode: hover/focus variants (`hover:bg-[#0077ed]`), `divide-*` and `placeholder-*` now follow the theme (the redirect never covered them), tints use the dark link blue, and `#0071e3` (9 uses) became the link token, a one-shade change in light. About 20 other hex colours (terminal and mock-window palettes, status colours) are left on purpose.
- **Done (step 6):** under 640px tables become stacked cards (the header row hides and `useTableLabels` copies each column header onto its cells as `data-label`; a table opts out with `data-no-cards`). Under 900px the nav is a bottom sheet with a backdrop, grab handle and Escape to close. Buttons, tabs, selects and inputs are at least 44px on phones and touch screens, and inputs are 16px so iOS does not zoom. Checked by script on the 20 baseline pages: no sideways scroll and no target under 44px. Desktop captures are pixel-identical. Not verified on real data: the empty mock has no table rows, so cards were checked on an injected table, not on each page's real table; wide matrix tables may want `data-no-cards`.\n- **Done (step 7):** `npm run a11y` (`web/a11y-check.mjs`, in the CI `web` job) runs axe-core (WCAG 2.0/2.1 A and AA) on the 20 baseline pages in both themes at desktop and phone width, and checks that the first focused control shows a ring. It fails on any serious or critical violation. The first run found 212 (contrast, unlabelled inputs/selects/icon buttons, links with no name, brand label mismatch); all fixed: new `--zf-link-fill` (#0071e3 in dark so white text passes) and `--zf-success-text` tokens, white text restored on solid blue fills, labels tied to Settings fields, aria-labels on the breadcrumb home link, Logs level select and clear button. Limits: it runs against the empty mock, so states with data (tables, charts, dialogs) are not covered; the focus check only tests the first control per page.\n- **Done (step 4):** the dashboard has one primary action (Create VM), a promoted ⌘K search, four large tiles (VMs, Running, Alerts, Storage used) with sparklines, and the six subsystem cards under "System status" below the charts. The sparklines cover the current visit only (no history API). Axe is clean on it in both themes.\n- **Done (step 5):** loading shows a skeleton instead of a spinner or bare "Loading…" text on about 35 pages and panels: `PageLoading` (title, toolbar and a table or card grid, same height as `PageHeader` so nothing jumps) and `PanelLoading` for tabs and panels under a header that is already on screen. Both announce `role=status` with a screen-reader label. Skeleton blocks use the shared shimmer and stand still under reduced motion; chart skeleton bars no longer re-roll random heights on every render. Sidebar links use View Transitions for route changes. Spinners remain only where an action is running (submit, analyse). Not done: lists do not animate out, only in; the network-security modal tabs still say "Loading…".\n- **Done (step 8):** `/start` is the docs landing, in the home page's hero, section and closing-band style, with Start here / Learn / Look it up card grids; `/` opens search (the plugin default was Cmd/Ctrl+K); doc pages show a "last updated" line from git (CI checkouts use `fetch-depth: 0`). `npm run check` in `website/` (also in the CI `website` job) serves `build/` like GitHub Pages (gzip, cache headers), then for 8 pages in both themes at desktop and phone width checks: no sideways scroll, no serious or critical axe violation, the page really renders in the requested theme; and Lighthouse accessibility and performance of at least 95 (median of three runs; the stock desktop config is used for desktop). Last run: accessibility 98-100, performance 98-100. The first run found real problems on existing pages too: the primary blue failed contrast (now #0066cc light, #4da3ff dark, with separate darker fills for buttons), in-text links relied on colour alone (now underlined), Prism's light code colours fell under 4.5:1 (overridden), and the Cockpit mock greys on black. Measuring honestly mattered: a bare file server without gzip scored performance 69-80 for the same pages, and my first "light" runs were dark because Docusaurus namespaces its theme storage key.
- **Audit (after step 8):** the axe gates only see empty-state pages, so a source scan found 58 icon-only buttons with no accessible name and 242 fields whose visible label was not tied to them; `web/codemod-a11y-names.py` names them (icon-based labels such as "Delete" and "Close", repeated across rows; row-specific names would be better).
- Dark mode renders cleanly on all 20 pages; no unreadable text seen in the images checked (dashboard, VMs).
- Not yet looked at one by one: the other pages' images; empty lists mean tables and cards are not exercised until the mock is seeded.

## Steps

**0. See it first (enabler).** Add a dev-only mock API so the console renders every page with no backend, then take light, dark and 390px screenshots of the 20 busiest pages as a baseline. Without this, every later step is blind. Done when `npm run dev:mock` opens the dashboard signed in. *Built: `web/mock-api.ts`; lists answer `[]`, so pages render empty states until seeded. Baseline taken: see below.*

**1. One page header.** Make `PageHeader` the display title: 28-32px, weight 600, tight tracking, a one-line muted subtitle, actions on the right that wrap on a phone. Move the pages that do not use it (about a dozen). One component change, 86+ pages. Done when every page's `h1` comes from it.

**2. Tokens, not hex.** A codemod maps the 1,453 utilities onto the `--zf-*` tokens, then the dark-mode redirect block is deleted. Done when a grep for `[#` in `pages/` and `components/` is empty and dark mode looks the same in the baseline screenshots.

**3. Type floor.** Nothing under 12px for readable text (the 83 small sizes become 12px, kept only for kbd chips). Done when the baseline shows no text a person has to squint at.

**4. Dashboard as a product page.** Four large numbers (VMs, running, alerts, storage) with a sparkline each, one primary action, and the command palette promoted as the way in. Done when the first screen has one clear next action.

**5. Motion that means something.** Route changes use the View Transitions API where the browser has it; loading shows a skeleton shaped like the page (11 pages do today) instead of a spinner; lists animate in and out. All of it stays off under reduced motion.

**6. Phone width.** Tables become cards under 640px, the sidebar becomes a bottom sheet, tap targets are 44px. Done when no page scrolls sideways at 390px.

**7. Accessibility gate in CI.** axe on the baseline pages (no serious or critical), visible focus, labelled controls, contrast checked in both themes.

**8. The site.** A docs landing in the same style as the home page, search that opens with `/`, a "last updated" line, and the same checks (phone width, axe, Lighthouse accessibility and performance of at least 95).

## How we'll know

Screenshot diffs of the baseline pages on every change, axe clean, Lighthouse scores for the site, and a short human review of the dashboard, a list page, a detail page and the sign-in page in both themes.

## Risks

- A token codemod can change a colour by a shade; the screenshots exist to catch that.
- Restyling `PageHeader` changes 86 pages at once, so it ships alone.
- Every step above step 0 is unverified until step 0 exists.
