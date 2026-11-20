# Contributing Guide

Thank you for your interest in contributing to Zyvor Fabric. This document covers
the development workflow, code style conventions, testing requirements, and
pull request process.

---

## Table of Contents

1. [Development Environment Setup](#development-environment-setup)
2. [Building the Project](#building-the-project)
3. [Running Tests](#running-tests)
4. [Code Style](#code-style)
5. [Architecture Guidelines](#architecture-guidelines)
6. [Adding a New API Endpoint](#adding-a-new-api-endpoint)
7. [Adding a New Crate](#adding-a-new-crate)
8. [Pull Request Process](#pull-request-process)
9. [Commit Message Format](#commit-message-format)

---

## Development Environment Setup

### Prerequisites

- Rust 1.75 or later (install via [rustup](https://rustup.rs))
- Linux (x86_64) with systemd 254+
- Development headers: `openssl-devel`, `pam-devel`, `systemd-devel`
- Node.js 20+ and npm (for web UI development)
- Git

### Initial Setup

```bash
# Clone the repository
git clone https://github.com/zyvorai/fabric.git
cd zyvor-fabric

# Install Rust toolchain components
rustup component add clippy rustfmt

# Build everything to verify the environment
cd backend && cargo build

# Run the test suite
cargo test

# Set up the web UI
cd ../web && npm install
```

### Editor Setup

For VS Code with rust-analyzer:

```json
{
  "rust-analyzer.linkedProjects": ["backend/Cargo.toml"],
  "rust-analyzer.check.command": "clippy"
}
```

---

## Building the Project

```bash
cd backend

# Full debug build (all 53 crates)
cargo build

# Release build (optimized)
cargo build --release

# Fast compile check (no codegen, faster than build)
cargo check

# Build a single crate
cargo build -p vm-model
cargo build -p state-store

# Check for warnings with clippy
cargo clippy -- -W clippy::all
```

See [building.md](building.md) for detailed build instructions including
the web UI.

---

## Running Tests

```bash
cd backend

# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p vm-model
cargo test -p state-store
cargo test -p security

# Run a specific test by name
cargo test test_create_vm

# Run tests with output
cargo test -- --nocapture

# Run integration tests only
cargo test -p Zyvor Fabric --test '*'
```

### Catch-up coverage (FluxVM boundary)

CI also runs an explicit **Catch-up coverage** job (see root `CONTRIBUTING.md` and
`.github/workflows/ci.yml`): FluxVM stub smoke, Rust serde/HTTP contracts for
migration receivers / dataplane pod fields / QGA / agent-runtime, and Vitest API
path filters. Fabric E2E sources `scripts/ci-feat-catchup-stub.sh` against the
same stub. Lab-only verify: `scripts/feat-catchup-verify.sh`.

Agent sessions have their own workflow, `.github/workflows/agent-runtime.yml`.
The `sessions` job runs `agent-runtime/tests/session-ci.sh` with Node, Rust,
and Go. It does not need FluxVM. A push to `main` also runs
`.github/workflows/lab-deploy.yml`, which SSHes to the lab host with
`LAB_DEPLOY_KEY` and runs `scripts/deploy-remote.sh --quick --e2e --verify-apis`.

### Test Requirements

- All tests must pass before submitting a PR
- Zero compiler warnings are required
- New features should include unit tests
- API endpoint changes should include integration tests

---

## Code Style

### Rust Conventions

Zyvor Fabric follows standard Rust conventions with these specific preferences:

**Formatting**: Use `rustfmt` with default settings. Run `cargo fmt` before
committing.

**Naming**:
- Crate names: `kebab-case` (e.g., `vm-model`, `state-store`)
- Module names: `snake_case` (e.g., `network_policy.rs`)
- Types: `PascalCase` (e.g., `VMState`, `CreateVMRequest`)
- Functions: `snake_case` (e.g., `list_vms`, `validate_vm_name`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `MAX_WS_CONNECTIONS`)

**Error Handling**:
- Use `anyhow::Result` for functions that can fail in multiple ways
- Use `thiserror` for crate-specific error types
- Never `unwrap()` in production code; use `unwrap_or_else` with recovery
- Return structured JSON errors from API handlers

**Logging**:
- Use `tracing` macros: `tracing::info!`, `tracing::warn!`, `tracing::error!`
- Include structured fields: `tracing::info!(vm_name = %name, "Starting VM")`
- Use `debug!` for detailed operational information
- Use `warn!` for recoverable anomalies
- Use `error!` for conditions that require operator attention
- Never log secrets, passwords, or JWT tokens

**Documentation**:
- All public functions must have a doc comment (`///`)
- Include `# Errors` section for functions returning `Result`
- Include `# Panics` section if a function can panic

**Security**:
- Validate all user input before use
- Use `validate_vm_name()` for VM names, `validate_entity_name()` for others
- Sanitize error messages for non-admin users
- Never include file paths in errors returned to non-admin users

### API Conventions

- All API endpoints are under `/api/v1/`
- Use plural nouns for resource collections: `/app/vms`, `/app/templates`, `/app/backups`
- Use nested resources for relationships: `/app/vms/{name}/snapshots`
- Use POST for actions: `/app/vms/{name}/start`, `/app/vms/{name}/stop`
- Return paginated responses: `{ items: [...], total, offset, limit }`
- Return `201 Created` for successful creation
- Return `404 Not Found` for missing resources
- Return `409 Conflict` for state conflicts (e.g., starting an already-running VM)
- Return `422 Unprocessable Entity` for validation errors

---

## Architecture Guidelines

### Adding New Functionality

When adding new functionality, follow the established pattern:

1. **Define data models** in the appropriate crate (usually `vm-model` or a
   domain-specific crate)
2. **Implement business logic** in a domain crate (e.g., `encryption`,
   `network-policy`)
3. **Add state persistence** via `StateStore::save_entity` / `get_entity`
4. **Create API handlers** in a new module under `Zyvor Fabric/src/api/`
5. **Register routes** in `Zyvor Fabric/src/server.rs` `build_router()`
6. **Add auth extractors** (`RequireRead`, `RequireWrite`, `RequireAdmin`)
7. **Write audit log entries** for state-changing operations
8. **Emit SSE events** for real-time notifications
9. **Add tests** for both the domain crate and API handlers

### Concurrency

- Use `Arc<AppState>` for shared state access across handlers
- Acquire per-VM locks via `state.vm_lock(name)` for state-changing operations
- Use `tokio::sync::RwLock` for read-heavy shared state
- Use `std::sync::Mutex` for simple, non-async critical sections
- Prefer `tokio::task::spawn_blocking` for CPU-bound or blocking I/O work

### State Management

- All persistent state goes through the `StateStore`
- Use atomic writes (write to `.tmp`, then `rename`) for crash safety
- Validate entity IDs to prevent path traversal
- Keep the in-memory VM cache in sync with the filesystem state

---

## Adding a New API Endpoint

Step-by-step example: adding a `GET /api/v1/vms/{name}/health` endpoint.

### 1. Create or update the API module

```rust
// Zyvor Fabric/src/api/vm_health.rs
use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use std::sync::Arc;
use security::RequireRead;
use crate::server::AppState;
use crate::validation::validate_vm_name;

pub async fn get_vm_health(
    RequireRead(_claims): RequireRead,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_vm_name(&name) {
        return (status, Json(json!({ "error": msg }))).into_response();
    }

    // Business logic here
    (StatusCode::OK, Json(json!({ "status": "healthy" }))).into_response()
}
```

### 2. Register the module

```rust
// Zyvor Fabric/src/api/mod.rs
pub mod vm_health;
```

### 3. Add the route

```rust
// Zyvor Fabric/src/server.rs, inside build_router()
.route("/vms/{name}/health", get(api::vm_health::get_vm_health))
```

### 4. Add tests

```rust
// Zyvor Fabric/tests/vm_health_test.rs
#[tokio::test]
async fn test_get_vm_health() {
    // Set up test AppState, make request, assert response
}
```

---

## Adding a New Crate

1. Create the crate directory under `backend/`:
   ```bash
   cd backend && cargo init --lib my-new-crate
   ```

2. Add it to `backend/Cargo.toml` workspace members:
   ```toml
   members = [
       # ... existing members
       "my-new-crate",
   ]
   ```

3. Use workspace dependencies:
   ```toml
   # my-new-crate/Cargo.toml
   [dependencies]
   serde.workspace = true
   anyhow.workspace = true
   ```

4. Add it as a dependency to `Zyvor Fabric/Cargo.toml`:
   ```toml
   my-new-crate = { path = "../my-new-crate" }
   ```

---

## Pull Request Process

1. **Create a feature branch** from `main`:
   ```bash
   git checkout -b feature/my-feature
   ```

2. **Make your changes** following the code style and architecture guidelines

3. **Verify locally**:
   ```bash
   cd backend
   cargo fmt -- --check    # Check formatting
   cargo clippy             # Check for lint warnings
   cargo test               # Run all tests
   cargo check              # Verify compilation
   ```

4. **Commit** with a descriptive message (see format below)

5. **Push** and open a pull request against `main`

6. **PR requirements**:
   - All CI checks pass (build, test, clippy, fmt)
   - Zero compiler warnings
   - Zero test failures
   - Code review approval
   - New code has appropriate test coverage

---

## Commit Message Format

Use descriptive, multi-line commit messages:

```
<type>: <short summary in imperative mood>

<body: explain what changed and why, not how>

<optional: list notable changes>
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `refactor`: Code restructuring without behavior change
- `docs`: Documentation changes
- `test`: Test additions or fixes
- `perf`: Performance improvement
- `security`: Security fix or hardening
- `chore`: Build, tooling, or dependency updates

Examples:

```
feat: Add per-VM health check endpoint

Adds GET /api/v1/vms/{name}/health that returns the health status
of a VM based on its current state and recent error history.

- New api module: vm_health.rs
- Requires RequireRead authentication
- Returns JSON with status and last_check timestamp
```

```
fix: Prevent path traversal in entity ID validation

The entity ID validator was not checking for null bytes, which could
allow path traversal on certain filesystems. Added \0 to the
rejected character set.
```
