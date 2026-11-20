# Machina Desktop (Tauri v2)

macOS/Linux/Windows shell for **Zyvor Fabric** — v0.1 workbench with VM dashboard.

## Prerequisites

- Node.js 18+
- Rust toolchain
- macOS: Xcode CLT (for Tauri)

## Development

```bash
cd integrations/machina/desktop
npm install
npm run tauri dev
```

Connect to a running `zyvor-fabricd` instance. Use **Dashboard**, **Live events** (SSE), or **AI Copilot** (v0.1 rule-based).

## Build

```bash
npm run tauri build
```

## Architecture

- **Frontend:** React + Vite (`src/`)
- **Backend:** Tauri Rust commands (`src-tauri/`) calling `zyvor-fabric-sdk`

Companion CLI: [../client](../client/) (`machina-fabric`).
