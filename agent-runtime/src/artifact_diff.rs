// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Line diff between two artifact bodies (LCS, bounded).

use serde::Serialize;

/// Above this many lines per side the diff is refused rather than run (LCS is O(n·m)).
pub const MAX_DIFF_LINES: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "op", content = "line", rename_all = "lowercase")]
pub enum DiffLine {
    Same(String),
    Add(String),
    Del(String),
}

#[derive(Debug, Serialize)]
pub struct DiffSummary {
    pub added: usize,
    pub removed: usize,
    pub unchanged: usize,
}

/// Diff `a` (older) against `b` (newer). `None` when either side exceeds [`MAX_DIFF_LINES`].
pub fn diff_lines(a: &str, b: &str) -> Option<Vec<DiffLine>> {
    let x: Vec<&str> = a.lines().collect();
    let y: Vec<&str> = b.lines().collect();
    if x.len() > MAX_DIFF_LINES || y.len() > MAX_DIFF_LINES {
        return None;
    }
    let (n, m) = (x.len(), y.len());
    // lcs[i][j] = LCS length of x[i..] and y[j..]
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if x[i] == y[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut out = Vec::with_capacity(n.max(m));
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if x[i] == y[j] {
            out.push(DiffLine::Same(x[i].into()));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            out.push(DiffLine::Del(x[i].into()));
            i += 1;
        } else {
            out.push(DiffLine::Add(y[j].into()));
            j += 1;
        }
    }
    out.extend(x[i..].iter().map(|l| DiffLine::Del((*l).into())));
    out.extend(y[j..].iter().map(|l| DiffLine::Add((*l).into())));
    Some(out)
}

pub fn summarize(lines: &[DiffLine]) -> DiffSummary {
    let mut s = DiffSummary {
        added: 0,
        removed: 0,
        unchanged: 0,
    };
    for l in lines {
        match l {
            DiffLine::Same(_) => s.unchanged += 1,
            DiffLine::Add(_) => s.added += 1,
            DiffLine::Del(_) => s.removed += 1,
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_all_same() {
        let d = diff_lines("a\nb", "a\nb").unwrap();
        let s = summarize(&d);
        assert_eq!((s.added, s.removed, s.unchanged), (0, 0, 2));
    }

    #[test]
    fn detects_add_remove_and_keeps_order() {
        let d = diff_lines("a\nb\nc", "a\nx\nc\nd").unwrap();
        assert_eq!(
            d,
            vec![
                DiffLine::Same("a".into()),
                DiffLine::Del("b".into()),
                DiffLine::Add("x".into()),
                DiffLine::Same("c".into()),
                DiffLine::Add("d".into()),
            ]
        );
    }

    #[test]
    fn empty_sides() {
        assert_eq!(summarize(&diff_lines("", "a\nb").unwrap()).added, 2);
        assert_eq!(summarize(&diff_lines("a\nb", "").unwrap()).removed, 2);
    }

    #[test]
    fn refuses_oversized_input() {
        let big = "x\n".repeat(MAX_DIFF_LINES + 1);
        assert!(diff_lines(&big, "a").is_none());
    }
}
