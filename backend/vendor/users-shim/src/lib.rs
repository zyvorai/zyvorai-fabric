// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Patches the unmaintained `users` crate to `uzers`, its actively
//! maintained fork with an identical public API, via `[patch.crates-io]`
//! in `backend/Cargo.toml`. Only exists because `uzers` never published a
//! 0.10.x release, so it can't satisfy `pam`'s hardcoded `users = "^0.10"`
//! requirement directly as a version-renamed patch.

pub use uzers::*;
