# Social assets

| File | What it is | Rebuild |
|---|---|---|
| `fabric-share-card.html` / `.png` | 1200×630 light card: README hero (light theme) and the website Open Graph image (`website/static/img/social-card.png`, kept byte-identical) | `./docs/social/build-social-card.sh` |
| `fabric-share-card-dark.html` / `.png` | 1200×630 dark twin, used by the README `<picture>` on dark themes. **Not** copied to the website | same script |
| `fabric-social-card.html` / `.jpg` | 1600×900 (16:9) card for LinkedIn and X, in the suite brochure look: install → create → network → secure → operate | same script |

Needs Google Chrome and macOS `sips` (nothing to install). Override the browser with `CHROME=/path/to/chrome`.

```bash
./docs/social/build-social-card.sh
cmp docs/social/fabric-share-card.png website/static/img/social-card.png   # must be identical
```

## Palette (apple.com blue / white)

| Role | Light | Dark |
|---|---|---|
| Background | `#ffffff` → `#f5f5f7`, subtle blue radial wash | `#000000` → `#0b0b0f`, blue wash |
| Text | ink `#1d1d1f`, secondary `#6e6e73` | `#f5f5f7`, `#a1a1a6` |
| Blue | `#0071e3` → `#2997ff` (accent text `#0066cc`) | `#0a84ff` → `#64b0ff` |
| Cards / hairline | white, `#d2d2d7`, soft blue-tinted shadow | `#1d1d1f`, `#2c2c2e` |

Orange `#ff6a2a` appears exactly once per card: the dot on the AI node (share cards) and beside "AI" in the last step
(brochure card). The brochure card uses an inline blue Zyvor mark instead of the orange favicon so no other orange is
introduced. Type is Helvetica Neue with Menlo for labels.

Copy follows [docs/POSITIONING.md](../POSITIONING.md) and the project README. The version string (`v0.3.0`) matches
`backend/Cargo.toml` and the `zyvor-fabric/v0.3.0` tag; update it in all three HTML files when releasing. Licence
wording follows `LICENSE` (Zyvor Production License v1.0).

## GitHub social preview

The repository's "Social preview" image (Settings → Social preview) cannot be set through the API or `gh`. After
merging a redesign, upload `docs/social/fabric-share-card.png` there by hand.
