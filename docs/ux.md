# Zyvor Fabric web UX conventions

Hybrid surface: public Apple-style marketing pages and a light, sparse authenticated console under `/app`.

Visual reference: [apple.com/airpods](https://www.apple.com/airpods/) — SF Pro typography, high-contrast ink on `#f5f5f7`.

## Surfaces

| Surface | Routes | Shell |
|---------|--------|--------|
| Marketing | `/`, `/product`, `/platform`, `/security` | [`MarketingLayout`](../web/src/components/MarketingLayout.tsx) |
| Sign-in | `/sign-in` (legacy `/login` redirects here) | Standalone card on an aurora and grid backdrop; follows the light and dark theme |
| Console | `/app/*` | [`ConsoleLayout`](../web/src/components/ConsoleLayout.tsx) — top bar + left nav |

## Visual system

Light-first Apple tokens in [`web/src/styles/main.css`](../web/src/styles/main.css):

| Token | Value | Role |
|-------|--------|------|
| `--zf-ink` | `#1d1d1f` | Headlines and primary body |
| `--zf-secondary` | `#333336` | Supporting copy (clearly readable) |
| `--zf-muted` | `#6e6e73` | Captions, table headers, footnotes |
| `--zf-canvas` | `#f5f5f7` | Page background |
| `--zf-surface` | `#ffffff` | Cards and panels |
| `--zf-hairline` | `#d2d2d7` | Borders |
| `--zf-link` | `#0066cc` | Links and primary accents |

Typography: **SF Pro Text / Display** via `-apple-system` / `BlinkMacSystemFont` (same stack as apple.com). Themes `dark` / `steel` / `aurora` are removed.

Buttons: `.zf-btn` / `.zf-btn-primary` / `.zf-btn-ghost` / `.zf-btn-secondary`.

Console table cells stay ink; muted is for captions only. Avoid `#86868b` — too faint on `#f5f5f7`.

## Primitives

| Component | Use when |
|-----------|----------|
| [`EmptyState`](../web/src/components/ui/EmptyState.tsx) | Zero rows; include primary CTA |
| [`ErrorBanner`](../web/src/components/ErrorBanner.tsx) | Page load or action failure with hints |
| [`PageHeader`](../web/src/components/ui/PageHeader.tsx) | Title, description, refresh, primary action |
| [`WizardStepper`](../web/src/components/WizardStepper.tsx) | Multi-step Create VM / wizard flows |
| [`Breadcrumb`](../web/src/components/Breadcrumb.tsx) | Path trail under `/app` |
| [`CopyButton`](../web/src/components/CopyButton.tsx) | Clipboard copy with toast |
| [`PageSkeleton`](../web/src/components/PageSkeleton.tsx) | Suspense fallback for lazy routes |
| [`HelpDialog`](../web/src/components/HelpDialog.tsx) | Shortcuts and About |

## API errors

Daemon returns `{ "error": "…", "error_code": "operation_failed" }` on failure.

| Utility | Use when |
|---------|----------|
| [`formatHttpErrorBody`](../web/src/utils/apiError.ts) | Parsing non-OK `fetch` bodies |
| [`formatUserError`](../web/src/utils/apiError.ts) | Any `catch` shown in toasts or banners |
| [`toastFailure`](../web/src/utils/toastError.ts) | `toastFailure(toast, 'Label', e)` shorthand |

Wire [`daemonHints.ts`](../web/src/utils/daemonHints.ts) via `ErrorBanner` `hints={hintsForError(loadError, 'vm')}` on Dashboard, VM list/detail, Storage, Network, and Operations pages.

## Console chrome

- **Command palette** — `Ctrl/Cmd+K`; fuzzy search VMs and pages (`/app/...` routes)
- **Sequence shortcuts** — `g` then `d`/`v`/`n`/`s`/`c`/`l`/`b`/`i`/`e` → `/app/...`
- **Help** — `?`
- **Connection** — live WebSocket badge in the top bar

## Create VM

Canonical route: **`/app/create`**.

## Interfaces

Human UI is **web only**. CLI (`fabricctl`), Kubernetes operator, and Terraform remain. The former terminal UI (`zyvorctl-tui`) has been removed.
