# Build Guide

This document covers how to build the Zyvor Fabric project from source, including
all backend crates and the React web UI.

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Building the Backend](#building-the-backend)
3. [Building Individual Crates](#building-individual-crates)
4. [Running Tests](#running-tests)
5. [Building the Web UI](#building-the-web-ui)
6. [Development Server](#development-server)
7. [Cross-Compilation](#cross-compilation)
8. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### Rust Toolchain

Install Rust 1.75 or later via [rustup](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Verify installation
rustc --version   # Must be 1.75+
cargo --version
```

### System Dependencies

#### Fedora / RHEL / CentOS

```bash
sudo dnf install -y \
  gcc \
  make \
  openssl-devel \
  pam-devel \
  systemd-devel \
  sqlite-devel \
  dbus-devel \
  pkg-config
```

#### Debian / Ubuntu

```bash
sudo apt install -y \
  build-essential \
  libssl-dev \
  libpam0g-dev \
  libsystemd-dev \
  libsqlite3-dev \
  libdbus-1-dev \
  pkg-config
```

#### Arch Linux

```bash
sudo pacman -S \
  base-devel \
  openssl \
  pam \
  systemd-libs \
  sqlite \
  dbus \
  pkgconf
```

### Node.js (for Web UI)

Install Node.js 20+ via your package manager or [nvm](https://github.com/nvm-sh/nvm):

```bash
# Via nvm
nvm install 20
nvm use 20

# Or via package manager (Fedora)
sudo dnf install -y nodejs npm

# Verify
node --version   # Must be 20+
npm --version
```

### Optional Tools

```bash
# Clippy (Rust linter) - usually included with rustup
rustup component add clippy

# rustfmt (code formatter)
rustup component add rustfmt

# cargo-watch (auto-rebuild on file changes)
cargo install cargo-watch
```

---

## Building the Backend

### Full Build

```bash
cd backend

# Debug build (faster compilation, larger binary, debug symbols)
cargo build

# Release build (slower compilation, optimized binary)
cargo build --release
```

The debug build produces binaries in `target/debug/`:
- `Zyvor Fabric` -- the main daemon
- `fabricctl` -- the CLI client

The release build produces optimized binaries in `target/release/`.

### Quick Compile Check

For a fast feedback loop during development, use `cargo check` which skips
code generation:

```bash
cargo check
```

This is significantly faster than `cargo build` and catches all compilation
errors and warnings.

### Warnings

The project must compile with zero warnings. Verify with:

```bash
cargo build 2>&1 | grep -c "warning"
# Should output: 0
```

---

## Building Individual Crates

Each of the 53 crates can be built and tested independently:

```bash
cd backend

# Build a specific crate
cargo build -p vm-model
cargo build -p state-store
cargo build -p security
cargo build -p zyvor-fabric-vm-driver
cargo build -p Zyvor Fabric-driver-core
cargo build -p networking
cargo build -p network-policy
cargo build -p service-mesh
cargo build -p traffic-shaping

# Check a specific crate (faster than build)
cargo check -p vm-model
```

### Common Crate Build Order

When working on a specific feature, you typically need to build the crates
in dependency order:

```
vm-model (no dependencies within workspace)
    |
    v
state-store (depends on vm-model)
    |
    v
security (standalone)
    |
    v
Zyvor Fabric-driver-core (depends on vm-model)
    |
    v
zyvor-fabric-vm-driver (depends on vm-model)
    |
    v
Zyvor Fabric (depends on everything)
```

---

## Running Tests

### Full Test Suite

```bash
cd backend
cargo test
```

### Individual Crate Tests

```bash
# Core crates
cargo test -p vm-model
cargo test -p state-store
cargo test -p security

# Networking crates
cargo test -p network-policy
cargo test -p service-mesh
cargo test -p dns-policy

# Management crates
cargo test -p encryption
cargo test -p resource-pools
cargo test -p certificate-manager
```

### Running Specific Tests

```bash
# Run tests matching a name pattern
cargo test test_create_vm
cargo test test_validate

# Run tests in a specific module
cargo test -p Zyvor Fabric routes::tests

# Run tests with stdout output visible
cargo test -- --nocapture

# Run tests in a single thread (useful for debugging)
cargo test -- --test-threads=1
```

### Integration Tests

```bash
# Run integration tests for the main daemon
cargo test -p zyvor-fabricd --test '*'

# Note: Some integration tests may require root access for KVM and
# network bridge operations, plus a running `fluxvm serve` instance
# for VM driver operations.
```

---

## Building the Web UI

### Development Build

```bash
cd zyvor-fabric/web

# Install dependencies
npm install

# Start the development server (hot reload)
npm run dev
```

The Vite development server starts on `http://localhost:5173` and proxies
API requests to `http://127.0.0.1:9095`.

### Production Build

```bash
cd zyvor-fabric/web

# Type-check and build for production
npm run build
```

The production build outputs static files to `web/dist/`, which Zyvor Fabric
serves directly via `tower-http::ServeDir`.

### Linting and Testing

```bash
cd zyvor-fabric/web

# Run ESLint
npm run lint

# Run Vitest tests
npm run test

# Preview the production build locally
npm run preview
```

---

## Development Server

For a complete development setup with hot-reloading on both backend and
frontend:

### Terminal 1: Backend

```bash
cd backend

# Option A: Manual rebuild
cargo build && sudo ./target/debug/zyvor-fabricd

# Option B: Auto-rebuild with cargo-watch
cargo watch -x build
# Then in another terminal:
sudo ./target/debug/zyvor-fabricd
```

### Terminal 2: Frontend

```bash
cd zyvor-fabric/web
npm run dev
```

### Configuration for Development

Create `backend/configs/zyvor-fabricd.toml` (the daemon checks this
path before `/etc/zyvor-fabricd/zyvor-fabricd.toml`):

```toml
[daemon]
listen = "127.0.0.1:9095"
cors_origins = ["http://127.0.0.1:9095", "http://localhost:5173"]

[storage]
path = "/tmp/zyvor-fabricd-dev"
image_path = "/tmp/zyvor-fabricd-dev/images"

[network]
bridge = "br0"

[auth]
enabled = false  # Disable auth for quick local development
```

---

## Cross-Compilation

### Building for a Different Target

```bash
# Add the target
rustup target add x86_64-unknown-linux-musl

# Build with musl for static linking
cargo build --release --target x86_64-unknown-linux-musl
```

Note: Cross-compilation requires the appropriate system libraries for the
target. The PAM and D-Bus dependencies may need special handling for
musl targets.

---

## Troubleshooting

### Common Build Errors

**Missing `pam-devel` / `libpam0g-dev`**:
```
error: could not find system library 'pam'
```
Solution: Install the PAM development headers for your distribution.

**Missing `systemd-devel` / `libsystemd-dev`**:
```
error: could not find system library 'libsystemd'
```
Solution: Install the systemd development headers.

**Missing `dbus-devel` / `libdbus-1-dev`**:
```
error: failed to run custom build command for `dbus`
```
Solution: Install the D-Bus development headers.

**Rust version too old**:
```
error: edition 2021 is not supported
```
Solution: Update Rust with `rustup update`.

**Out of disk space during build**:
```
error: No space left on device
```
Solution: The full debug build can use 5+ GB in `target/`. Clean with
`cargo clean` or increase disk space.

### Build Performance

- Use `cargo check` instead of `cargo build` for fast feedback
- Use `sccache` for build caching across clean builds
- Consider using `mold` as the linker for faster linking:
  ```bash
  # Install mold
  sudo dnf install -y mold   # Fedora
  sudo apt install -y mold   # Ubuntu

  # Configure cargo to use mold
  mkdir -p .cargo
  echo '[target.x86_64-unknown-linux-gnu]
  linker = "clang"
  rustflags = ["-C", "link-arg=-fuse-ld=mold"]' > .cargo/config.toml
  ```
