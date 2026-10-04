# Zyvor Fabric docs site

Built with [Docusaurus](https://docusaurus.io/). Serves the live docs at https://zyvorai.github.io/zyvorai-fabric/.

Unlike a hand-curated docs site, this one points directly at the repo's existing `docs/` folder (`docusaurus.config.ts`'s `docs.path: '../docs'`) — every doc in `docs/` becomes a page automatically, sidebar auto-generated from the folder structure. Add/edit docs in `../docs/` as usual; there's no separate copy to keep in sync.

## Local development

```bash
npm install
npm start
```

## Build

```bash
npm run build
npm run serve   # preview the production build locally
```

## Images

The dashboard screenshot on the homepage is **not** duplicated into `website/static/` — `docusaurus.config.ts`'s `staticDirectories` serves `../docs/assets` in place, so the root README and this site both reference the same physical file.

## Deployment

Deployment is automatic: `.github/workflows/pages.yml` builds and publishes this site to GitHub Pages on every push to `main` that touches `website/`, `docs/`, or the workflow file itself. There is no manual `npm run deploy` step — don't use Docusaurus's built-in `deploy` script, it targets a `gh-pages` branch this repo doesn't use.

**Keep marketing:** `/keep` — Muse-caliber product page with Muse → Keep → Fabric → FluxVM stack and versus table (where it runs, policy, cell, model, training, host eBPF, proof on stage, leave). Docs under `/docs/keep/`; Tutorial 17 at `/docs/tutorials/keep-pdf-brief`. Live: https://zyvorai.github.io/zyvorai-fabric/keep.

**Phone makers:** `/keep/phones` — how a phone vendor can use Keep: architecture, a user's day, the approval handshake, isolation, benchmark and a built / reference / not-built list. Copy is in `src/data/phoneVendor.ts` (not `docs/keep/marketing.json`, which the Zyvor web app also imports); diagrams are the SVGs in `docs/assets/keep/`, shared with the GitHub docs.

**Homepage / matrix:** `/` — Fabric-first landing with the compare experience. Default matrix tab is **Fabric vs the field**; also Muse vs Keep · Fabric · FluxVM (stack), FluxVM vs libvirt, animated stack, CONNECT 0 proof, use-case explorer, profile ladder, cockpit mock, receipts, roadmap, and quickstart. Deep links: `?uc=<use-case>`, `?t=<stack|fabric|flux>`, `?f=<security|ops|portability>`, `#matrix`. Source: `src/components/compare/ComparePage.tsx`. Legacy `/compare` redirects to `/` (query + hash preserved). Live: https://zyvorai.github.io/zyvorai-fabric/#matrix.
