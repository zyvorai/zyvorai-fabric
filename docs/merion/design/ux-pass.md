# UX pass: console and website

**Status:** built. Verified by typecheck, lint, 432 console tests and both production builds; the site was looked at in a browser. The signed-in console pages were **not** seen: they need a backend sign-in this session did not have.

## What was already there

Both surfaces already follow apple.com: SF type with size-based tracking, near-black dark theme, `#2997ff`/`#0071e3` blue, pill buttons, hairline borders, a frosted sticky bar. This pass does not redo that.

## What this adds

Global and cosmetic only; no layout, colour or markup beyond one link.

| Where | Change |
|---|---|
| Console | A "Skip to content" link, shown on keyboard focus |
| Console | One visible keyboard focus ring on links, buttons and fields |
| Console | One easing curve (`cubic-bezier(0.28, 0.11, 0.32, 1)`, 0.24s) for hover, press and colour changes; buttons press to 98% |
| Console | A page eases in (6px, 0.4s) when it mounts |
| Console and site | Headings use `text-wrap: balance` so a title does not strand one word |
| Site | The same focus ring, a tinted text selection, and the shared easing on buttons, nav and menu links |
| Both | Every motion is switched off under `prefers-reduced-motion` |

## Not done

- Resizing page titles. Titles are mostly `h2` at 20px; making them display-sized needs a look at each page, which needs a signed-in console.
- A page-by-page review of the 92 console pages, and a check at phone width.
- Contrast and screen-reader audits.
