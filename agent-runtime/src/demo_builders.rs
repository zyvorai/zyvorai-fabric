// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Pure artifact builders for the one-click Keep demos.
//!
//! Every builder is extractive: it reads text the guest already pulled out of an
//! untrusted file and writes a summary on the host. Nothing here calls a model,
//! opens the network, or executes anything found in the input. Untrusted lines
//! are always passed through [`clean_line`] before they reach markdown, so a
//! hostile document cannot smuggle markup or links into an artifact.

use serde_json::Value;
use std::collections::BTreeMap;

/// One artifact a demo produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoArtifact {
    /// Artifact kind stored with the record (`brief`, `clauses`, `csv`, ...).
    pub kind: String,
    /// File-style title shown in the console (`brief.md`, `clean.csv`).
    pub title: String,
    pub content_type: &'static str,
    pub body: String,
}

impl DemoArtifact {
    fn markdown(kind: &str, title: &str, body: String) -> Self {
        Self {
            kind: kind.into(),
            title: title.into(),
            content_type: "text/markdown",
            body,
        }
    }
}

pub type BuildResult = Result<Vec<DemoArtifact>, String>;

/// Collapse whitespace, drop control characters, defang markup, and cap length.
pub fn clean_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len().min(240));
    let mut last_space = false;
    for ch in s.chars() {
        let ch = match ch {
            '`' => '\'',
            '<' => '(',
            '>' => ')',
            '|' => '/',
            c if c.is_control() => ' ',
            c => c,
        };
        if ch.is_whitespace() {
            if !last_space && !out.is_empty() {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(ch);
            last_space = false;
        }
        if out.chars().count() >= 220 {
            out.push('…');
            break;
        }
    }
    out.trim().to_string()
}

fn bullet_list(items: &[String], empty: &str) -> String {
    if items.is_empty() {
        format!("- {empty}")
    } else {
        items
            .iter()
            .map(|l| format!("- {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ── pdf-brief ───────────────────────────────────────────────────────────────

pub fn pdf_brief(filename: &str, extract: &str) -> BuildResult {
    let preview: String = extract.chars().take(1200).collect();
    let important: Vec<String> = extract
        .lines()
        .map(str::trim)
        .filter(|l| l.len() > 40)
        .take(5)
        .map(str::to_string)
        .collect();
    let body = format!(
        r#"# brief.md

## Summary
Local extract of `{filename}` inside a Keep cell with `egress_mode: deny` and
FluxVM `deny_udp` + gateway-only L4 pin. No browser. No CONNECT.

## Important
{important}

## Quotes
```
{preview}
```

## Missing
Model socket optional for stage demos — this brief is extractive so a 502
never forces a browser fallback.
"#,
        filename = clean_line(filename),
        important = bullet_list(&important, "(short document)"),
    );
    Ok(vec![DemoArtifact::markdown("brief", "brief.md", body)])
}

// ── contract-clauses ────────────────────────────────────────────────────────

const CLAUSE_TOPICS: &[(&str, &[&str])] = &[
    (
        "Term",
        &[
            "initial term",
            "term of this",
            "effective date",
            "commencement",
        ],
    ),
    ("Renewal", &["renew", "auto-renew", "automatically extend"]),
    ("Termination", &["terminat", "cancel"]),
    (
        "Payment",
        &["payment", "invoice", "fees", "net 30", "net 45", "net 60"],
    ),
    (
        "Liability",
        &["liability", "indemnif", "limitation of", "consequential"],
    ),
    ("Confidentiality", &["confidential", "non-disclosure"]),
    ("Governing law", &["governing law", "jurisdiction", "venue"]),
    (
        "Data protection",
        &[
            "personal data",
            "data protection",
            "gdpr",
            "security incident",
        ],
    ),
];

pub fn contract_clauses(filename: &str, extract: &str) -> BuildResult {
    let mut found: Vec<(&str, Vec<String>)> = Vec::new();
    let mut missing: Vec<&str> = Vec::new();
    for (topic, needles) in CLAUSE_TOPICS {
        let hits: Vec<String> = extract
            .lines()
            .map(str::trim)
            .filter(|l| l.len() > 20)
            .filter(|l| {
                let lower = l.to_lowercase();
                needles.iter().any(|n| lower.contains(n))
            })
            .take(3)
            .map(clean_line)
            .collect();
        if hits.is_empty() {
            missing.push(topic);
        } else {
            found.push((topic, hits));
        }
    }
    let mut body = format!(
        "# clauses.md\n\nKeyword extraction from `{}`. This is a reading aid, not legal advice: \
         check every line against the original.\n\n",
        clean_line(filename)
    );
    body.push_str(&format!(
        "**{} of {} topics found.**\n\n",
        found.len(),
        CLAUSE_TOPICS.len()
    ));
    for (topic, hits) in &found {
        body.push_str(&format!("## {topic}\n{}\n\n", bullet_list(hits, "")));
    }
    body.push_str("## Not found\n");
    body.push_str(&bullet_list(
        &missing.iter().map(|m| m.to_string()).collect::<Vec<_>>(),
        "(every topic had a match)",
    ));
    body.push('\n');
    Ok(vec![DemoArtifact::markdown("clauses", "clauses.md", body)])
}

// ── security-questionnaire ──────────────────────────────────────────────────

fn looks_like_question(line: &str) -> bool {
    let l = line.trim();
    if l.len() < 12 {
        return false;
    }
    let lower = l.to_lowercase();
    let stripped = lower.trim_start_matches(|c: char| {
        c.is_ascii_digit() || matches!(c, '.' | ')' | '-' | ' ' | ':')
    });
    l.ends_with('?')
        || [
            "do you", "does ", "is there", "are ", "describe", "please ", "how ",
        ]
        .iter()
        .any(|p| stripped.starts_with(p))
}

pub fn security_questionnaire(filename: &str, extract: &str) -> BuildResult {
    let lines: Vec<&str> = extract.lines().map(str::trim).collect();
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < lines.len() && rows.len() < 40 {
        if looks_like_question(lines[i]) {
            let q = clean_line(lines[i]);
            let mut j = i + 1;
            let mut answer = String::new();
            while j < lines.len() && !looks_like_question(lines[j]) && answer.chars().count() < 200
            {
                if !lines[j].is_empty() {
                    if !answer.is_empty() {
                        answer.push(' ');
                    }
                    answer.push_str(lines[j]);
                }
                j += 1;
            }
            let answer = clean_line(&answer);
            rows.push((
                q,
                if answer.is_empty() {
                    "(no answer found)".into()
                } else {
                    answer
                },
            ));
            i = j;
        } else {
            i += 1;
        }
    }
    let unanswered = rows
        .iter()
        .filter(|(_, a)| a == "(no answer found)")
        .count();
    let mut body = format!(
        "# answers.md\n\nQuestions and the text that follows them in `{}`. \
         {} questions found, {} without an answer.\n\n",
        clean_line(filename),
        rows.len(),
        unanswered
    );
    if rows.is_empty() {
        body.push_str(
            "No question-shaped lines found. The PDF may be a scan with no text layer.\n",
        );
    } else {
        body.push_str("| # | Question | Answer as written |\n|---|---|---|\n");
        for (n, (q, a)) in rows.iter().enumerate() {
            body.push_str(&format!("| {} | {q} | {a} |\n", n + 1));
        }
    }
    Ok(vec![DemoArtifact::markdown("answers", "answers.md", body)])
}

// ── meeting-actions ─────────────────────────────────────────────────────────

const ACTION_MARKERS: &[&str] = &[
    "action item",
    "action:",
    "todo",
    "to-do",
    "to do:",
    "follow up",
    "follow-up",
    "will send",
    "will share",
    "will review",
    "will update",
    "will draft",
    "needs to",
    "need to",
    "deadline",
    "due ",
    "by friday",
    "by monday",
    "by tuesday",
    "by wednesday",
    "by thursday",
    "by eod",
    "next step",
];
const DECISION_MARKERS: &[&str] = &[
    "decided",
    "agreed",
    "decision:",
    "we will go with",
    "approved",
];

/// `Name: text` speaker prefix, capped so a long sentence with a colon is not a name.
fn speaker_split(line: &str) -> (Option<String>, &str) {
    if let Some((head, rest)) = line.split_once(':') {
        let words = head.split_whitespace().count();
        if (1..=3).contains(&words)
            && head.chars().count() <= 30
            && head
                .chars()
                .all(|c| c.is_alphabetic() || c == ' ' || c == '.' || c == '-')
            && !head.to_lowercase().starts_with("action")
        {
            return (Some(clean_line(head)), rest.trim());
        }
    }
    (None, line)
}

pub fn meeting_actions(filename: &str, extract: &str) -> BuildResult {
    let mut actions: Vec<(String, String)> = Vec::new();
    let mut decisions: Vec<String> = Vec::new();
    for raw in extract.lines() {
        let line = raw.trim();
        // Skip WebVTT scaffolding: header, cue numbers, timestamps.
        if line.is_empty()
            || line.eq_ignore_ascii_case("webvtt")
            || line.contains("-->")
            || line.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        let (speaker, text) = speaker_split(line);
        let lower = text.to_lowercase();
        if DECISION_MARKERS.iter().any(|m| lower.contains(m)) && decisions.len() < 15 {
            decisions.push(clean_line(text));
        }
        if ACTION_MARKERS.iter().any(|m| lower.contains(m)) && actions.len() < 30 {
            actions.push((speaker.unwrap_or_else(|| "—".into()), clean_line(text)));
        }
    }
    let mut body = format!(
        "# actions.md\n\nLines from `{}` that read like commitments. Nothing was sent or scheduled: \
         these are candidates for you to confirm.\n\n## Action items\n",
        clean_line(filename)
    );
    if actions.is_empty() {
        body.push_str("- (none found)\n");
    } else {
        body.push_str("| Owner | Item |\n|---|---|\n");
        for (o, t) in &actions {
            body.push_str(&format!("| {o} | {t} |\n"));
        }
    }
    body.push_str("\n## Decisions\n");
    body.push_str(&bullet_list(&decisions, "(none found)"));
    body.push('\n');
    Ok(vec![DemoArtifact::markdown("actions", "actions.md", body)])
}

// ── log-triage ──────────────────────────────────────────────────────────────

/// Find an ISO-ish `YYYY-MM-DD[T ]HH:MM:SS` in a line; returns it as written.
fn find_timestamp(line: &str) -> Option<String> {
    let b = line.as_bytes();
    if b.len() < 19 {
        return None;
    }
    for i in 0..=b.len() - 19 {
        let w = &b[i..i + 19];
        let ok = w[0..4].iter().all(u8::is_ascii_digit)
            && w[4] == b'-'
            && w[5..7].iter().all(u8::is_ascii_digit)
            && w[7] == b'-'
            && w[8..10].iter().all(u8::is_ascii_digit)
            && (w[10] == b'T' || w[10] == b' ')
            && w[11..13].iter().all(u8::is_ascii_digit)
            && w[13] == b':'
            && w[14..16].iter().all(u8::is_ascii_digit)
            && w[16] == b':'
            && w[17..19].iter().all(u8::is_ascii_digit);
        if ok {
            return Some(String::from_utf8_lossy(w).into_owned());
        }
    }
    None
}

fn log_level(line: &str) -> Option<&'static str> {
    let upper = line.to_uppercase();
    for (needle, name) in [
        ("FATAL", "FATAL"),
        ("CRITICAL", "FATAL"),
        ("ERROR", "ERROR"),
        ("WARNING", "WARN"),
        ("WARN", "WARN"),
        ("INFO", "INFO"),
        ("DEBUG", "DEBUG"),
    ] {
        if upper.contains(needle) {
            return Some(name);
        }
    }
    None
}

/// Replace timestamps and digit runs so repeated messages group together.
pub(crate) fn normalise_message(line: &str) -> String {
    let no_ts = match find_timestamp(line) {
        Some(ts) => line.replacen(&ts, "", 1),
        None => line.to_string(),
    };
    let mut out = String::new();
    let mut in_digits = false;
    for ch in no_ts.chars() {
        if ch.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            in_digits = false;
            out.push(ch);
        }
    }
    clean_line(&out)
}

pub fn log_triage(filename: &str, extract: &str) -> BuildResult {
    let mut levels: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut messages: BTreeMap<String, u64> = BTreeMap::new();
    let mut per_minute: BTreeMap<String, u64> = BTreeMap::new();
    let (mut first, mut last): (Option<String>, Option<String>) = (None, None);
    let mut total = 0u64;
    for line in extract.lines().map(str::trim).filter(|l| !l.is_empty()) {
        total += 1;
        let ts = find_timestamp(line);
        if let Some(t) = &ts {
            if first.is_none() {
                first = Some(t.clone());
            }
            last = Some(t.clone());
        }
        if let Some(level) = log_level(line) {
            *levels.entry(level).or_default() += 1;
            if level == "ERROR" || level == "FATAL" {
                *messages.entry(normalise_message(line)).or_default() += 1;
                if let Some(t) = &ts {
                    *per_minute.entry(t[..16].to_string()).or_default() += 1;
                }
            }
        }
    }
    let mut top: Vec<(&String, &u64)> = messages.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let mut bursts: Vec<(&String, &u64)> = per_minute.iter().collect();
    bursts.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));

    let mut body = format!(
        "# triage.md\n\nLog summary for `{}`: {total} lines.\n\n## By level\n| Level | Lines |\n|---|---|\n",
        clean_line(filename)
    );
    for level in ["FATAL", "ERROR", "WARN", "INFO", "DEBUG"] {
        if let Some(n) = levels.get(level) {
            body.push_str(&format!("| {level} | {n} |\n"));
        }
    }
    if levels.is_empty() {
        body.push_str("| (no levels recognised) | 0 |\n");
    }
    body.push_str(&format!(
        "\n## Window\nFirst timestamp: {}  \nLast timestamp: {}\n",
        first.as_deref().unwrap_or("(none found)"),
        last.as_deref().unwrap_or("(none found)"),
    ));
    body.push_str("\n## Most repeated errors\n");
    if top.is_empty() {
        body.push_str("- (no error lines)\n");
    } else {
        for (msg, n) in top.iter().take(5) {
            body.push_str(&format!("- {n}× {msg}\n"));
        }
    }
    body.push_str("\n## Error bursts (per minute)\n");
    if bursts.is_empty() {
        body.push_str("- (no timestamped errors)\n");
    } else {
        for (minute, n) in bursts.iter().take(3) {
            body.push_str(&format!("- {minute}: {n} errors\n"));
        }
    }
    Ok(vec![DemoArtifact::markdown("triage", "triage.md", body)])
}

// ── sbom-summary ────────────────────────────────────────────────────────────

fn count_table(title: &str, counts: &BTreeMap<String, u64>, limit: usize) -> String {
    let mut rows: Vec<(&String, &u64)> = counts.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let mut out = format!("## {title}\n| Name | Count |\n|---|---|\n");
    if rows.is_empty() {
        out.push_str("| (none) | 0 |\n");
    }
    for (name, n) in rows.into_iter().take(limit) {
        out.push_str(&format!("| {} | {n} |\n", clean_line(name)));
    }
    out
}

pub fn sbom_summary(filename: &str, extract: &str) -> BuildResult {
    let doc: Value = serde_json::from_str(extract)
        .map_err(|e| format!("not valid JSON ({e}); is the file over the demo size limit?"))?;
    let mut body = format!("# summary.md\n\nSummary of `{}`.\n\n", clean_line(filename));
    let mut licenses: BTreeMap<String, u64> = BTreeMap::new();
    let mut severities: BTreeMap<String, u64> = BTreeMap::new();

    if doc.get("bomFormat").and_then(Value::as_str) == Some("CycloneDX") {
        let comps = doc.get("components").and_then(Value::as_array);
        let n = comps.map_or(0, Vec::len);
        let mut unlicensed = 0u64;
        for c in comps.into_iter().flatten() {
            let mut any = false;
            for l in c
                .get("licenses")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = l
                    .pointer("/license/id")
                    .or_else(|| l.pointer("/license/name"))
                    .or_else(|| l.get("expression"))
                    .and_then(Value::as_str);
                if let Some(name) = name {
                    *licenses.entry(name.to_string()).or_default() += 1;
                    any = true;
                }
            }
            if !any {
                unlicensed += 1;
            }
        }
        for v in doc
            .get("vulnerabilities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let sev = v
                .pointer("/ratings/0/severity")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            *severities.entry(sev.to_lowercase()).or_default() += 1;
        }
        body.push_str(&format!(
            "**Format:** CycloneDX {}  \n**Components:** {n}  \n**Without a license:** {unlicensed}\n\n",
            doc.get("specVersion").and_then(Value::as_str).unwrap_or("")
        ));
        body.push_str(&count_table("Licenses", &licenses, 10));
        body.push('\n');
        body.push_str(&count_table("Vulnerabilities by severity", &severities, 10));
    } else if let Some(v) = doc.get("spdxVersion").and_then(Value::as_str) {
        let pkgs = doc.get("packages").and_then(Value::as_array);
        let n = pkgs.map_or(0, Vec::len);
        let mut unlicensed = 0u64;
        for p in pkgs.into_iter().flatten() {
            match p
                .get("licenseConcluded")
                .or_else(|| p.get("licenseDeclared"))
                .and_then(Value::as_str)
            {
                Some(l) if l != "NOASSERTION" && l != "NONE" => {
                    *licenses.entry(l.to_string()).or_default() += 1;
                }
                _ => unlicensed += 1,
            }
        }
        body.push_str(&format!(
            "**Format:** {}  \n**Packages:** {n}  \n**Without a license:** {unlicensed}\n\n",
            clean_line(v)
        ));
        body.push_str(&count_table("Licenses", &licenses, 10));
    } else if let Some(runs) = doc.get("runs").and_then(Value::as_array) {
        let mut rules: BTreeMap<String, u64> = BTreeMap::new();
        let mut tools: Vec<String> = Vec::new();
        for run in runs {
            if let Some(t) = run.pointer("/tool/driver/name").and_then(Value::as_str) {
                tools.push(clean_line(t));
            }
            for r in run
                .get("results")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let level = r.get("level").and_then(Value::as_str).unwrap_or("warning");
                *severities.entry(level.to_lowercase()).or_default() += 1;
                if let Some(id) = r.get("ruleId").and_then(Value::as_str) {
                    *rules.entry(id.to_string()).or_default() += 1;
                }
            }
        }
        body.push_str(&format!(
            "**Format:** SARIF  \n**Tools:** {}\n\n",
            if tools.is_empty() {
                "(unnamed)".into()
            } else {
                tools.join(", ")
            }
        ));
        body.push_str(&count_table("Findings by level", &severities, 10));
        body.push('\n');
        body.push_str(&count_table("Most frequent rules", &rules, 10));
    } else {
        return Err("unrecognised JSON: expected CycloneDX, SPDX or SARIF".into());
    }
    Ok(vec![DemoArtifact::markdown("summary", "summary.md", body)])
}

// ── csv-clean ───────────────────────────────────────────────────────────────

/// Minimal RFC 4180 reader: quoted fields, doubled quotes, embedded newlines.
pub fn parse_csv(input: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cell.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() => in_quotes = true,
            ',' => row.push(std::mem::take(&mut cell)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            c => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

fn csv_quote(cell: &str) -> String {
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

/// A leading `=`, `+`, `-` or `@` can run as a formula in a spreadsheet. A plain
/// signed number is not a formula.
fn is_formula_cell(cell: &str) -> bool {
    let c = cell.trim_start();
    match c.chars().next() {
        Some('=') | Some('@') => true,
        Some('+') | Some('-') => c[1..].trim().parse::<f64>().is_err(),
        _ => false,
    }
}

pub fn csv_clean(filename: &str, extract: &str) -> BuildResult {
    let parsed = parse_csv(extract);
    if parsed.is_empty() {
        return Err("empty CSV".into());
    }
    let width = parsed[0].len();
    let (mut blank, mut dupes, mut ragged) = (0u64, 0u64, 0u64);
    let mut flagged: Vec<(usize, usize, String)> = Vec::new();
    let mut flagged_total = 0u64;
    let mut seen = std::collections::HashSet::new();
    let mut out_rows: Vec<Vec<String>> = Vec::new();
    for (ri, row) in parsed.iter().enumerate() {
        let trimmed: Vec<String> = row.iter().map(|c| c.trim().to_string()).collect();
        if trimmed.iter().all(String::is_empty) {
            blank += 1;
            continue;
        }
        if ri > 0 && trimmed.len() != width {
            ragged += 1;
        }
        let mut cells = trimmed;
        for (ci, cell) in cells.iter_mut().enumerate() {
            if is_formula_cell(cell) {
                flagged_total += 1;
                if flagged.len() < 20 {
                    flagged.push((ri + 1, ci + 1, clean_line(cell)));
                }
                // Neutralise: a leading apostrophe makes spreadsheets read it as text.
                *cell = format!("'{cell}");
            }
        }
        if ri > 0 && !seen.insert(cells.clone()) {
            dupes += 1;
            continue;
        }
        out_rows.push(cells);
    }
    let clean = out_rows
        .iter()
        .map(|r| r.iter().map(|c| csv_quote(c)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    let mut report = format!(
        "# report.md\n\nCleanup of `{}`.\n\n| Check | Result |\n|---|---|\n\
         | Rows in (incl. header) | {} |\n| Rows out | {} |\n| Blank rows dropped | {blank} |\n\
         | Exact duplicates dropped | {dupes} |\n| Rows with a different column count | {ragged} |\n\
         | Formula-like cells neutralised | {flagged_total} |\n\n",
        clean_line(filename),
        parsed.len(),
        out_rows.len(),
    );
    report.push_str("## Formula-like cells\n");
    if flagged.is_empty() {
        report.push_str("- (none)\n");
    } else {
        for (r, c, v) in &flagged {
            report.push_str(&format!("- row {r}, column {c}: `{v}`\n"));
        }
    }
    report.push_str(
        "\nCells starting with `=`, `+`, `-` or `@` were prefixed with an apostrophe in \
         `clean.csv` so a spreadsheet reads them as text instead of running them.\n",
    );
    Ok(vec![
        DemoArtifact {
            kind: "csv".into(),
            title: "clean.csv".into(),
            content_type: "text/csv",
            body: clean,
        },
        DemoArtifact::markdown("report", "report.md", report),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(a: BuildResult) -> String {
        a.unwrap().into_iter().next().unwrap().body
    }

    #[test]
    fn clean_line_defangs_markup_and_caps_length() {
        let c = clean_line("  <script>alert(1)</script>\t`x` | y \n");
        assert!(!c.contains('<') && !c.contains('`') && !c.contains('|'));
        assert!(clean_line(&"a".repeat(1000)).chars().count() <= 221);
    }

    #[test]
    fn pdf_brief_keeps_the_existing_shape() {
        let b = only(pdf_brief(
            "x.pdf",
            "A long enough line to count as important content here",
        ));
        assert!(b.starts_with("# brief.md"));
        assert!(b.contains("- A long enough line"));
        assert!(only(pdf_brief("x.pdf", "short")).contains("(short document)"));
    }

    #[test]
    fn contract_clauses_finds_and_reports_missing_topics() {
        let text = "1. Term. The initial term of this Agreement is twelve months.\n\
                    2. Payment. Invoices are due net 30 from receipt of invoice.\n\
                    3. Governing law. This Agreement is governed by the laws of Delaware.";
        let b = only(contract_clauses("c.pdf", text));
        assert!(
            b.contains("## Term") && b.contains("## Payment") && b.contains("## Governing law")
        );
        assert!(b.contains("## Not found") && b.contains("- Liability"));
    }

    #[test]
    fn questionnaire_pairs_questions_with_answers() {
        let text = "1. Do you encrypt data at rest?\nYes, AES-256 on all volumes.\n\
                    2. Do you run annual penetration tests?\n\
                    3. Describe your incident response process.\nA 24 hour on-call rota.";
        let b = only(security_questionnaire("q.pdf", text));
        assert!(b.contains("3 questions found, 1 without an answer"));
        assert!(b.contains("AES-256") && b.contains("(no answer found)"));
    }

    #[test]
    fn meeting_actions_reads_vtt_and_finds_owners() {
        let text = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:04.000\nDana: I will send the revised quote by Friday.\n\n\
                    2\n00:00:05.000 --> 00:00:08.000\nSam: We agreed to go ahead with the pilot.";
        let b = only(meeting_actions("m.vtt", text));
        assert!(b.contains("| Dana |") && b.contains("revised quote"));
        assert!(b.contains("## Decisions") && b.contains("agreed to go ahead"));
        assert!(!b.contains("-->"));
    }

    #[test]
    fn log_triage_counts_levels_groups_errors_and_finds_bursts() {
        let text = "2026-09-25T10:00:01 INFO started\n\
                    2026-09-25T10:00:05 ERROR timeout after 30s calling db-1\n\
                    2026-09-25T10:00:09 ERROR timeout after 45s calling db-2\n\
                    2026-09-25T10:01:00 WARN slow query\n";
        let b = only(log_triage("a.log", text));
        assert!(b.contains("| ERROR | 2 |") && b.contains("| WARN | 1 |"));
        assert!(b.contains("2× ")); // both timeouts collapse to one message
        assert!(b.contains("2026-09-25T10:00: 2 errors"));
        assert!(b.contains("First timestamp: 2026-09-25T10:00:01"));
    }

    #[test]
    fn sbom_summary_reads_cyclonedx_spdx_and_sarif() {
        let cdx = r#"{"bomFormat":"CycloneDX","specVersion":"1.5","components":[
            {"name":"a","licenses":[{"license":{"id":"MIT"}}]},{"name":"b"}],
            "vulnerabilities":[{"ratings":[{"severity":"high"}]}]}"#;
        let b = only(sbom_summary("s.json", cdx));
        assert!(b.contains("**Components:** 2") && b.contains("**Without a license:** 1"));
        assert!(b.contains("| MIT | 1 |") && b.contains("| high | 1 |"));

        let spdx = r#"{"spdxVersion":"SPDX-2.3","packages":[{"licenseConcluded":"Apache-2.0"},{"licenseConcluded":"NOASSERTION"}]}"#;
        assert!(only(sbom_summary("s.json", spdx)).contains("**Packages:** 2"));

        let sarif = r#"{"runs":[{"tool":{"driver":{"name":"scanner"}},"results":[{"ruleId":"R1","level":"error"},{"ruleId":"R1"}]}]}"#;
        let b = only(sbom_summary("s.json", sarif));
        assert!(b.contains("scanner") && b.contains("| R1 | 2 |"));

        assert!(sbom_summary("s.json", "{}").is_err());
        assert!(sbom_summary("s.json", "not json").is_err());
    }

    #[test]
    fn csv_parser_handles_quotes_and_newlines() {
        let rows = parse_csv("a,b\r\n\"x,1\",\"he said \"\"hi\"\"\"\n\"multi\nline\",z");
        assert_eq!(rows[1], vec!["x,1", "he said \"hi\""]);
        assert_eq!(rows[2], vec!["multi\nline", "z"]);
    }

    #[test]
    fn csv_clean_neutralises_formulas_and_drops_dupes() {
        let text =
            "name,amount,note\n Ann ,5,ok\nAnn,5,ok\n,,\nBob,-3.5,=HYPERLINK(\"http://x\")\n";
        let out = csv_clean("d.csv", text).unwrap();
        assert_eq!(out.len(), 2);
        let (csv, report) = (&out[0].body, &out[1].body);
        assert_eq!(csv.lines().count(), 3); // header + Ann + Bob
        assert!(csv.contains("'=HYPERLINK"));
        assert!(csv.contains("-3.5") && !csv.contains("'-3.5")); // signed number is not a formula
        assert!(report.contains("Exact duplicates dropped | 1"));
        assert!(report.contains("Blank rows dropped | 1"));
        assert!(report.contains("neutralised | 1"));
    }
}
