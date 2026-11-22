// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use std::path::PathBuf;

/// Unified error type for all cgroup v2 operations.
#[derive(Debug, thiserror::Error)]
pub enum CgroupError {
    #[error("cgroup path not found: {0}")]
    NotFound(PathBuf),

    #[error("controller {controller:?} not available at {path}")]
    ControllerNotAvailable { controller: String, path: PathBuf },

    #[error("failed to read {path}: {source}")]
    ReadFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write {path}: {source}")]
    WriteFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse {path}: {detail} (content: {content:?})")]
    ParseError {
        path: PathBuf,
        content: String,
        detail: String,
    },

    #[error("invalid value for {field}: {value:?} (expected {expected})")]
    InvalidValue {
        field: String,
        value: String,
        expected: String,
    },
}

pub type Result<T> = std::result::Result<T, CgroupError>;
