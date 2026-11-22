// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! TTY-aware ANSI styling for fabricctl (Cilium-like colorful output).

use clap::ValueEnum;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const GREEN: &str = "\x1b[32m";
pub const RED: &str = "\x1b[31m";
pub const YELLOW: &str = "\x1b[33m";
pub const BLUE: &str = "\x1b[34m";
pub const CYAN: &str = "\x1b[36m";

static COLOR_ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorMode {
    /// Resolve whether ANSI color should be emitted for stdout.
    ///
    /// `always` wins over `NO_COLOR` so scripts and CI can force color.
    /// `auto` honors `NO_COLOR` and only paints a TTY.
    pub fn enabled(self) -> bool {
        match self {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => {
                if std::env::var_os("NO_COLOR").is_some() {
                    return false;
                }
                std::io::stdout().is_terminal()
            }
        }
    }
}

/// Install the process-wide color decision used by table/status helpers.
pub fn set_color_enabled(on: bool) {
    COLOR_ENABLED.store(on, Ordering::Relaxed);
}

pub fn color_enabled() -> bool {
    COLOR_ENABLED.load(Ordering::Relaxed)
}

pub fn paint(on: bool, code: &str, text: &str) -> String {
    if on {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}

pub fn paint_global(code: &str, text: &str) -> String {
    paint(color_enabled(), code, text)
}

/// Color a status / state cell for tables.
pub fn status_cell(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    let code = if lower.contains("run")
        || lower.contains("active")
        || lower.contains("ok")
        || lower.contains("ready")
        || lower.contains("healthy")
        || lower == "forwarded"
        || lower == "true"
    {
        GREEN
    } else if lower.contains("stop")
        || lower.contains("disabled")
        || lower.contains("inactive")
        || lower.contains("pending")
        || lower == "false"
        || lower.contains("audit")
    {
        YELLOW
    } else if lower.contains("error")
        || lower.contains("fail")
        || lower.contains("drop")
        || lower.contains("denied")
        || lower.contains("crash")
    {
        RED
    } else if lower.contains("unknown") || lower.is_empty() {
        DIM
    } else {
        return raw.to_string();
    };
    paint_global(code, raw)
}

/// Cilium-style checklist mark.
pub fn check_ok(msg: &str) -> String {
    format!("{} {}", paint_global(GREEN, "✅"), msg)
}

pub fn check_fail(msg: &str) -> String {
    format!("{} {}", paint_global(RED, "❌"), msg)
}

pub fn check_warn(msg: &str) -> String {
    format!("{} {}", paint_global(YELLOW, "⚠️"), msg)
}

pub fn heading(text: &str) -> String {
    paint_global(BOLD, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_off_has_no_ansi() {
        assert_eq!(paint(false, GREEN, "ok"), "ok");
        assert!(paint(true, GREEN, "ok").contains('\u{1b}'));
    }

    #[test]
    fn status_cell_colors_when_enabled() {
        set_color_enabled(true);
        assert!(status_cell("Running").contains('\u{1b}'));
        assert!(status_cell("DROPPED").contains('\u{1b}'));
        set_color_enabled(false);
        assert_eq!(status_cell("Running"), "Running");
    }

    #[test]
    fn never_mode_disables() {
        assert!(!ColorMode::Never.enabled());
    }
}
