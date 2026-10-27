// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! One-click Keep demos: drop an untrusted file into a sealed cell, get an
//! artifact back. Every demo never opens a browser and never CONNECTs; the run
//! fails closed (session frozen, 409) if the audit journal shows an outbound
//! connection.
//!
//! Demos are table-driven. A [`DemoSpec`] names the accepted file types, the
//! fixed guest command that extracts text, and the pure builder in
//! [`crate::demo_builders`] that turns that text into artifacts. Adding a use
//! case means adding a spec and a builder, not a route.

use crate::{
    app::{ApiError, ApiResult},
    audit::AuditPhase,
    demo_builders::{self as b, BuildResult},
    demo_rules::{CustomDemoRecord, CustomDemoSpec, Extractor, MAX_CUSTOM_DEMOS, MAX_SPEC_BYTES},
    goals::{ArtifactRecord, GoalRecord, GoalStatus},
    model::{SessionRecord, SessionStartMode, SessionStartPolicy, SessionStatus},
    AppState,
};
use axum::{
    extract::{Extension, Multipart, Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

/// Everything that differs between demos.
pub(crate) struct DemoSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub blurb: &'static str,
    /// Lower-case file extensions (no dot) an upload may have.
    pub accepts: &'static [&'static str],
    /// Fixed name of the file inside the guest. Never derived from the upload.
    pub guest_file: &'static str,
    /// Fixed guest command; `{path}` is replaced with the guest file path.
    pub extract_cmd: &'static str,
    /// Largest upload accepted, in bytes.
    pub max_bytes: usize,
    /// Shown when the guest extracts nothing.
    pub empty_msg: &'static str,
    pub goal_title: &'static str,
    pub goal_description: &'static str,
    pub build: fn(&str, &str) -> BuildResult,
    /// Built-in input used when no file is uploaded: (filename, bytes).
    pub sample: fn() -> (&'static str, Vec<u8>),
}

const PDF_CMD: &str = "pdftotext -layout {path} - 2>/dev/null | head -c 24000";
/// Photos and scans: tesseract, English, "single column of text of variable sizes" (receipts and bills). The runtime, not the
/// pack, chooses the command; the file name is fixed and tesseract sniffs the image format from the bytes.
const OCR_CMD: &str = "tesseract {path} stdout -l eng --psm 4 2>/dev/null | head -c 24000";
const OCR_EMPTY: &str = "No text could be read from the image: it may be blank, too blurry or too small. Try a sharper, straight-on photo";
const PDF_EMPTY: &str =
    "No text layer: this looks like a scan. Keep reads photos and screenshots (png, jpg) with OCR, so export the pages as images. It never sends page images to a model";
/// Where a script extractor is written inside the cell.
const GUEST_EXTRACT_SCRIPT: &str = "/home/agent/work/extract.mjs";
const TEXT_LIMIT: usize = 200_000;
const JSON_LIMIT: usize = 300_000;
const PDF_LIMIT: usize = 32 * 1024 * 1024;

pub(crate) const DEMOS: &[DemoSpec] = &[
    DemoSpec {
        id: "pdf-brief",
        title: "PDF brief",
        blurb: "Drop a PDF, get a one-page brief.md.",
        accepts: &["pdf"],
        guest_file: "input.pdf",
        extract_cmd: PDF_CMD,
        max_bytes: PDF_LIMIT,
        empty_msg: PDF_EMPTY,
        goal_title: "Brief this PDF",
        goal_description:
            "Read /home/agent/work/input.pdf only. Write artifact brief.md. No browser. No hosts.",
        build: b::pdf_brief,
        sample: || ("lab-sample.pdf", lab_sample_pdf()),
    },
    DemoSpec {
        id: "contract-clauses",
        title: "Contract clauses",
        blurb: "Drop a contract PDF, get the term, renewal, payment, liability and governing-law lines.",
        accepts: &["pdf"],
        guest_file: "input.pdf",
        extract_cmd: PDF_CMD,
        max_bytes: PDF_LIMIT,
        empty_msg: PDF_EMPTY,
        goal_title: "Pull the key clauses",
        goal_description:
            "Read /home/agent/work/input.pdf only. Write artifact clauses.md. No browser. No hosts.",
        build: b::contract_clauses,
        sample: || {
            (
                "sample-contract.pdf",
                simple_pdf(&[
                    "MASTER SERVICES AGREEMENT",
                    "1. Term. The initial term of this Agreement is twelve (12) months from the Effective Date.",
                    "2. Renewal. This Agreement automatically renews for successive one-year terms.",
                    "3. Fees and payment. Invoices are due net 30 from receipt of invoice.",
                    "4. Termination. Either party may terminate for material breach on 30 days notice.",
                    "5. Limitation of liability. Neither party is liable for consequential damages.",
                    "6. Confidentiality. Each party keeps the other's confidential information private.",
                    "7. Governing law. This Agreement is governed by the laws of the State of Delaware.",
                ]),
            )
        },
    },
    DemoSpec {
        id: "security-questionnaire",
        title: "Security questionnaire",
        blurb: "Drop a vendor questionnaire PDF, get each question paired with the answer as written.",
        accepts: &["pdf"],
        guest_file: "input.pdf",
        extract_cmd: PDF_CMD,
        max_bytes: PDF_LIMIT,
        empty_msg: PDF_EMPTY,
        goal_title: "Pair questions with answers",
        goal_description:
            "Read /home/agent/work/input.pdf only. Write artifact answers.md. No browser. No hosts.",
        build: b::security_questionnaire,
        sample: || {
            (
                "sample-questionnaire.pdf",
                simple_pdf(&[
                    "VENDOR SECURITY QUESTIONNAIRE",
                    "1. Do you encrypt customer data at rest?",
                    "Yes. AES-256 on all volumes.",
                    "2. Do you run annual penetration tests?",
                    "3. Describe your incident response process.",
                    "A 24 hour on-call rota with a written runbook.",
                ]),
            )
        },
    },
    DemoSpec {
        id: "meeting-actions",
        title: "Meeting actions",
        blurb: "Drop a transcript (.txt or .vtt), get action items with owners and decisions.",
        accepts: &["txt", "vtt"],
        guest_file: "input.txt",
        extract_cmd: "head -c 200000 {path}",
        max_bytes: TEXT_LIMIT,
        empty_msg: "The transcript is empty",
        goal_title: "Find the action items",
        goal_description:
            "Read /home/agent/work/input.txt only. Write artifact actions.md. No browser. No hosts.",
        build: b::meeting_actions,
        sample: || {
            (
                "sample-meeting.vtt",
                b"WEBVTT\n\n1\n00:00:01.000 --> 00:00:05.000\nDana: I will send the revised quote by Friday.\n\n\
2\n00:00:06.000 --> 00:00:09.000\nSam: We agreed to go ahead with the pilot.\n\n\
3\n00:00:10.000 --> 00:00:13.000\nLee: Action item: review the security questionnaire before next week.\n"
                    .to_vec(),
            )
        },
    },
    DemoSpec {
        id: "log-triage",
        title: "Log triage",
        blurb: "Drop a log file, get error counts, the most repeated errors and error bursts.",
        accepts: &["log", "txt"],
        guest_file: "input.log",
        extract_cmd: "head -c 200000 {path}",
        max_bytes: TEXT_LIMIT,
        empty_msg: "The log file is empty",
        goal_title: "Triage this log",
        goal_description:
            "Read /home/agent/work/input.log only. Write artifact triage.md. No browser. No hosts.",
        build: b::log_triage,
        sample: || {
            (
                "sample.log",
                b"2026-09-25T10:00:01 INFO service started\n\
2026-09-25T10:00:05 ERROR timeout after 30s calling db-1\n\
2026-09-25T10:00:09 ERROR timeout after 45s calling db-2\n\
2026-09-25T10:01:00 WARN slow query 1200ms\n\
2026-09-25T10:02:10 ERROR connection reset by peer\n"
                    .to_vec(),
            )
        },
    },
    DemoSpec {
        id: "sbom-summary",
        title: "SBOM summary",
        blurb: "Drop a CycloneDX, SPDX or SARIF JSON file, get component, license and severity counts.",
        accepts: &["json"],
        guest_file: "input.json",
        extract_cmd: "head -c 300000 {path}",
        max_bytes: JSON_LIMIT,
        empty_msg: "The JSON file is empty",
        goal_title: "Summarise this SBOM",
        goal_description:
            "Read /home/agent/work/input.json only. Write artifact summary.md. No browser. No hosts.",
        build: b::sbom_summary,
        sample: || {
            (
                "sample-bom.json",
                br#"{"bomFormat":"CycloneDX","specVersion":"1.5","components":[
{"name":"left-pad","version":"1.3.0","licenses":[{"license":{"id":"MIT"}}]},
{"name":"internal-lib","version":"0.4.1"}],
"vulnerabilities":[{"id":"CVE-2026-0001","ratings":[{"severity":"high"}]}]}"#
                    .to_vec(),
            )
        },
    },
    DemoSpec {
        id: "csv-clean",
        title: "CSV cleanup",
        blurb: "Drop a CSV, get clean.csv plus a report: blanks and duplicates removed, formula cells neutralised.",
        accepts: &["csv"],
        guest_file: "input.csv",
        extract_cmd: "head -c 300000 {path}",
        max_bytes: JSON_LIMIT,
        empty_msg: "The CSV file is empty",
        goal_title: "Clean this CSV",
        goal_description:
            "Read /home/agent/work/input.csv only. Write artifacts clean.csv and report.md. No browser. No hosts.",
        build: b::csv_clean,
        sample: || {
            (
                "sample.csv",
                b"name,amount,note\nAnn,5,ok\n Ann ,5,ok\n,,\nBob,-3.5,=HYPERLINK(\"http://example.com\")\n"
                    .to_vec(),
            )
        },
    },
];

fn spec_by_id(id: &str) -> Option<&'static DemoSpec> {
    DEMOS.iter().find(|d| d.id == id)
}

fn builtin_ids() -> Vec<&'static str> {
    DEMOS.iter().map(|d| d.id).collect()
}

/// How a resolved demo turns extracted text into artifacts.
enum ResolvedBuild {
    Native(fn(&str, &str) -> BuildResult),
    Rules(Box<CustomDemoSpec>),
}

/// What the runner needs, owned so built-in and custom demos share one path.
struct Resolved {
    id: String,
    accepts: Vec<String>,
    guest_file: String,
    extract_cmd: String,
    /// A fixed script written into the cell before extraction (never from a spec).
    extract_script: Option<&'static str>,
    /// Optional model step, run on the host after extraction.
    model: Option<crate::demo_rules::ModelSpec>,
    max_bytes: usize,
    empty_msg: String,
    goal_title: String,
    goal_description: String,
    build: ResolvedBuild,
    sample: Option<(String, Vec<u8>)>,
}

impl Resolved {
    fn builtin(d: &DemoSpec) -> Self {
        let (name, bytes) = (d.sample)();
        Self {
            id: d.id.into(),
            accepts: d.accepts.iter().map(|e| e.to_string()).collect(),
            guest_file: d.guest_file.into(),
            extract_cmd: d.extract_cmd.into(),
            extract_script: None,
            model: None,
            max_bytes: d.max_bytes,
            empty_msg: d.empty_msg.into(),
            goal_title: d.goal_title.into(),
            goal_description: d.goal_description.into(),
            build: ResolvedBuild::Native(d.build),
            sample: Some((name.to_string(), bytes)),
        }
    }

    fn custom(spec: CustomDemoSpec) -> Self {
        let guest = spec.guest_file();
        let max_bytes = spec.max_bytes();
        let (extract_cmd, empty_msg) = match spec.extract {
            Extractor::Pdftotext => (PDF_CMD.to_string(), PDF_EMPTY),
            Extractor::Ocr => (OCR_CMD.to_string(), OCR_EMPTY),
            Extractor::Text => (format!("head -c {max_bytes} {{path}}"), "The file is empty"),
            Extractor::Html
            | Extractor::Eml
            | Extractor::Docx
            | Extractor::Xlsx
            | Extractor::Pptx => (
                format!("node {GUEST_EXTRACT_SCRIPT} {{path}}"),
                "No text could be extracted: the file is empty, damaged, or not what its extension says",
            ),
        };
        Self {
            id: spec.id.clone(),
            accepts: spec.accepts.clone(),
            guest_file: guest.into(),
            extract_cmd,
            extract_script: spec.extract.script(),
            model: spec.model.clone(),
            max_bytes,
            empty_msg: empty_msg.into(),
            goal_title: spec.title.clone(),
            goal_description: format!(
                "Read /home/agent/work/{guest} only. Write artifact {}. No browser. No hosts.",
                spec.artifact_title()
            ),
            sample: spec
                .sample
                .as_ref()
                .map(|s| (s.filename.clone(), s.text.clone().into_bytes())),
            build: ResolvedBuild::Rules(Box::new(spec)),
        }
    }

    fn build(&self, filename: &str, extract: &str) -> BuildResult {
        match &self.build {
            ResolvedBuild::Native(f) => f(filename, extract),
            ResolvedBuild::Rules(spec) => spec.render(filename, extract),
        }
    }
}

/// What a trigger needs to pre-filter files: accepted extensions and the size cap.
pub(crate) async fn use_case_limits(state: &AppState, id: &str) -> Option<(Vec<String>, usize)> {
    resolve(state, id).await.map(|r| (r.accepts, r.max_bytes))
}

/// Built-ins first, then user-defined use cases.
async fn resolve(state: &AppState, id: &str) -> Option<Resolved> {
    if let Some(d) = spec_by_id(id) {
        return Some(Resolved::builtin(d));
    }
    state
        .store
        .get_demo_spec(id)
        .await
        .map(|r| Resolved::custom(r.spec))
}

/// `GET /v1/demos` — what the console can offer, built-ins and custom.
pub(crate) async fn demo_list(State(state): State<Arc<AppState>>) -> Json<Value> {
    let mut demos: Vec<Value> = DEMOS
        .iter()
        .map(|d| {
            json!({
                "id": d.id,
                "title": d.title,
                "description": d.blurb,
                "accepts": d.accepts,
                "max_bytes": d.max_bytes,
                "browser": "none",
                "egress": "deny",
                "builtin": true,
                "has_sample": true,
                "model": Value::Null,
            })
        })
        .collect();
    for r in state.store.list_demo_specs().await {
        let s = &r.spec;
        demos.push(json!({
            "id": s.id,
            "title": s.title,
            "description": s.description,
            "accepts": s.accepts,
            "max_bytes": s.max_bytes(),
            "browser": "none",
            "egress": "deny",
            "builtin": false,
            "has_sample": s.sample.is_some(),
            "model": s.model.as_ref().map(|m| json!({ "host": m.host(), "model": m.model })),
            "updated_at": r.updated_at,
        }));
    }
    Json(json!({ "demos": demos }))
}

/// `GET /v1/merion/status` (alias `/v1/keep/status`) — what `merionctl doctor` and the console check before
/// a first deploy: Keep mode, trusted signers, FluxVM readiness, demo counts.
pub(crate) async fn merion_status(State(state): State<Arc<AppState>>) -> Json<Value> {
    let fluxvm = state.fluxvm.security_capabilities().await;
    let trust = &state.policy_trust;
    Json(json!({
        "merion_mode": trust.merion_mode,
        "signature_required": trust.merion_mode || trust.require_signature,
        "trusted_signers": trust.trusted_signers.len(),
        "fluxvm": {
            "ready": fluxvm.is_ok(),
            "error": fluxvm.err().map(|e| format!("{e:#}")),
        },
        "demo_template": std::env::var("ZYVOR_DEMO_TEMPLATE").unwrap_or_else(|_| "node22-agent".into()),
        "demos": {
            "builtin": DEMOS.len(),
            "custom": state.store.list_demo_specs().await.len(),
        },
    }))
}

/// `POST /v1/demos` — create or replace a user-defined use case. A spec is
/// data (an extractor enum plus bounded rules), so it cannot widen what the
/// host does; every run is still strictly confined and 0-CONNECT checked.
pub(crate) async fn demo_save(
    State(state): State<Arc<AppState>>,
    Json(spec): Json<CustomDemoSpec>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    spec.validate(&builtin_ids())
        .map_err(ApiError::bad_request)?;
    let size = serde_json::to_vec(&spec)
        .map(|v| v.len())
        .unwrap_or(usize::MAX);
    if size > MAX_SPEC_BYTES {
        return Err(ApiError::bad_request(format!(
            "spec is {size} bytes; the limit is {MAX_SPEC_BYTES}"
        )));
    }
    let existing = state.store.get_demo_spec(&spec.id).await;
    if existing.is_none() && state.store.list_demo_specs().await.len() >= MAX_CUSTOM_DEMOS {
        return Err(ApiError::bad_request(format!(
            "at most {MAX_CUSTOM_DEMOS} custom use cases; delete one first"
        )));
    }
    let now = Utc::now();
    let created = existing.as_ref().map(|r| r.created_at).unwrap_or(now);
    let id = spec.id.clone();
    state
        .store
        .save_demo_spec(CustomDemoRecord {
            spec,
            created_at: created,
            updated_at: now,
        })
        .await
        .map_err(ApiError::internal)?;
    let code = if existing.is_some() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        code,
        Json(json!({ "id": id, "builtin": false, "replaced": existing.is_some() })),
    ))
}

/// `DELETE /v1/demos/{id}` — remove a user-defined use case (never a built-in).
pub(crate) async fn demo_delete(
    State(state): State<Arc<AppState>>,
    Path(demo_id): Path<String>,
) -> ApiResult<StatusCode> {
    if spec_by_id(&demo_id).is_some() {
        return Err(ApiError::bad_request(
            "built-in use cases cannot be deleted",
        ));
    }
    match state.store.delete_demo_spec(&demo_id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(ApiError::not_found(format!(
            "no use case named {demo_id:?}"
        ))),
        Err(e) => Err(ApiError::internal(e)),
    }
}

fn extension(filename: &str) -> String {
    filename
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// `POST /v1/demos/{id}` (multipart field `file`, or `pdf` for the original
/// PDF brief; optional — a built-in sample is used otherwise).
pub(crate) async fn demo_run(
    State(state): State<Arc<AppState>>,
    Extension(principal): Extension<crate::authz::Principal>,
    Path(demo_id): Path<String>,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<Value>)> {
    // A user token runs the use case as that user: their session, artifacts and quota.
    let user = principal.user().map(str::to_string);
    if resolve(&state, &demo_id).await.is_none() {
        return Err(ApiError::not_found(format!("no demo named {demo_id:?}")));
    }
    let mut files: Vec<Upload> = Vec::new();
    let mut total = 0usize;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" || name == "pdf" {
            let filename = field.file_name().map(str::to_string);
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad_request(e.to_string()))?
                .to_vec();
            total += bytes.len();
            if files.len() >= MAX_BATCH_FILES || total > MAX_BATCH_BYTES {
                return Err(ApiError::bad_request(format!(
                    "a batch takes at most {MAX_BATCH_FILES} files and {} MiB in total",
                    MAX_BATCH_BYTES / (1024 * 1024)
                )));
            }
            files.push(Upload { filename, bytes });
        }
    }
    // A zip is a container: unpack what this use case accepts and run each file in its own cell.
    if files.len() == 1
        && files[0]
            .filename
            .as_deref()
            .is_some_and(|n| extension(n) == "zip")
    {
        if let Some((accepts, max_bytes)) = use_case_limits(&state, &demo_id).await {
            if !accepts.iter().any(|e| e == "zip") {
                let inner = expand_zip(&files[0].bytes, &accepts, max_bytes)
                    .map_err(ApiError::bad_request)?;
                return run_batch(state, demo_id, inner, user).await;
            }
        }
    }
    match files.len() {
        0 => run_use_case(state, demo_id, None, None, None, user).await,
        1 => {
            let f = files.remove(0);
            run_use_case(state, demo_id, f.filename, Some(f.bytes), None, user).await
        }
        _ => run_batch(state, demo_id, files, user).await,
    }
}

/// Files inside a zip that the use case accepts, read into memory. Nothing is written to disk
/// and no path from the archive is used: only its base name, for the extension check.
/// Limits: 20 files, 64 MiB in total, each within the use case's size cap;
/// sizes are enforced on what is actually read, not on what the archive claims.
pub(crate) fn expand_zip(
    bytes: &[u8],
    accepts: &[String],
    max_bytes: usize,
) -> Result<Vec<Upload>, String> {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("not a readable zip file: {e}"))?;
    let mut out: Vec<Upload> = Vec::new();
    let mut total = 0usize;
    let mut skipped = 0usize;
    for i in 0..archive.len().min(2000) {
        let Ok(mut entry) = archive.by_index(i) else {
            skipped += 1;
            continue;
        };
        let Some(path) = entry.enclosed_name() else {
            skipped += 1;
            continue;
        };
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if entry.is_dir()
            || name.starts_with('.')
            || path.components().any(|c| c.as_os_str() == "__MACOSX")
            || !accepts.contains(&extension(&name))
        {
            continue;
        }
        if entry.size() as usize > max_bytes {
            skipped += 1;
            continue;
        }
        let mut data = Vec::new();
        if entry
            .by_ref()
            .take(max_bytes as u64 + 1)
            .read_to_end(&mut data)
            .is_err()
            || data.is_empty()
            || data.len() > max_bytes
        {
            skipped += 1;
            continue;
        }
        total += data.len();
        if out.len() >= MAX_BATCH_FILES || total > MAX_BATCH_BYTES {
            return Err(format!(
                "the zip holds more than {MAX_BATCH_FILES} usable files or {} MiB; split it",
                MAX_BATCH_BYTES / (1024 * 1024)
            ));
        }
        out.push(Upload {
            filename: Some(name),
            bytes: data,
        });
    }
    if out.is_empty() {
        return Err(format!(
            "the zip has no .{} files this use case can read{}",
            accepts.join(" / ."),
            if skipped > 0 {
                format!(" ({skipped} entries were skipped as too big, unsafe or unreadable)")
            } else {
                String::new()
            }
        ));
    }
    Ok(out)
}

/// One uploaded file, before it is checked against the use case.
pub(crate) struct Upload {
    pub filename: Option<String>,
    pub bytes: Vec<u8>,
}

pub(crate) const MAX_BATCH_FILES: usize = 20;
pub(crate) const MAX_BATCH_BYTES: usize = 64 * 1024 * 1024;

/// Several files, one sealed cell each, grouped under one `batch_id`. The batch
/// stops at the first frozen session (a 409): something reached for the network.
async fn run_batch(
    state: Arc<AppState>,
    demo_id: String,
    files: Vec<Upload>,
    user: Option<String>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let batch_id = Uuid::new_v4();
    let count = files.len();
    let (mut ok, mut failed, mut connects) = (0usize, 0usize, 0u64);
    let mut results: Vec<Value> = Vec::with_capacity(count);
    let mut stopped = false;
    for f in files {
        let name = f.filename.clone().unwrap_or_default();
        if stopped {
            failed += 1;
            results.push(json!({
                "filename": name, "ok": false, "skipped": true,
                "error": "skipped: an earlier file froze its session"
            }));
            continue;
        }
        match run_use_case(
            state.clone(),
            demo_id.clone(),
            f.filename,
            Some(f.bytes),
            Some(batch_id),
            user.clone(),
        )
        .await
        {
            Ok((_, Json(v))) => {
                ok += 1;
                connects += v["egress_connects"].as_u64().unwrap_or(0);
                results.push(json!({ "filename": name, "ok": true, "result": v }));
            }
            Err(e) => {
                failed += 1;
                stopped = e.status() == StatusCode::CONFLICT;
                results.push(json!({
                    "filename": name, "ok": false,
                    "status": e.status().as_u16(), "error": e.message()
                }));
            }
        }
    }
    let status = if failed == 0 {
        StatusCode::CREATED
    } else {
        StatusCode::MULTI_STATUS
    };
    Ok((
        status,
        Json(json!({
            "batch_id": batch_id,
            "demo": demo_id,
            "count": count,
            "ok": ok,
            "failed": failed,
            "egress_connects": connects,
            "results": results,
        })),
    ))
}

/// Run one use case on one file (or its sample) and tell the operator how it went.
/// Shared by the upload route, batches and triggers.
pub(crate) async fn run_use_case(
    state: Arc<AppState>,
    demo_id: String,
    filename: Option<String>,
    upload: Option<Vec<u8>>,
    batch_id: Option<Uuid>,
    user: Option<String>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let result = run_demo(
        state.clone(),
        demo_id.clone(),
        filename,
        upload,
        batch_id,
        user,
    )
    .await;
    match &result {
        Ok((_, Json(v))) => {
            let artifacts = v["artifacts"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| {
                            Some((
                                x["id"].as_str()?.parse().ok()?,
                                x["title"].as_str()?.to_string(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            crate::notify::run_finished(
                &state,
                crate::notify::RunNotice {
                    demo: demo_id,
                    session_id: v["session_id"].as_str().and_then(|s| s.parse().ok()),
                    outcome: Ok((artifacts, v["egress_connects"].as_u64().unwrap_or(0))),
                },
            );
        }
        // A rejected upload or unknown id is the caller's mistake, not a failed run.
        Err(e) if !matches!(e.status(), StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND) => {
            crate::notify::run_finished(
                &state,
                crate::notify::RunNotice {
                    demo: demo_id,
                    session_id: None,
                    outcome: Err(e.message().to_string()),
                },
            );
        }
        Err(_) => {}
    }
    result
}

/// Run a use case in a fresh cell, then end its session so the cell is released.
///
/// A run used to leave its session "Running" and its cell alive until the sandbox's own lifetime
/// (30 minutes) ran out. On a busy host those cells piled up, each holding memory. Marking the
/// session finished hands the cell to the terminal-session cleanup loop, which deletes it at once.
async fn run_demo(
    state: Arc<AppState>,
    demo_id: String,
    filename: Option<String>,
    upload: Option<Vec<u8>>,
    batch_id: Option<Uuid>,
    user: Option<String>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let mut session_id: Option<Uuid> = None;
    let result = run_demo_inner(
        state.clone(),
        demo_id,
        filename,
        upload,
        batch_id,
        user,
        &mut session_id,
    )
    .await;
    if let Some(id) = session_id {
        finish_session(&state, id, &result).await;
    }
    result
}

/// Mark a use-case session done. A session frozen because its cell tried to reach the network
/// (a 409) is left as it is: the cell is kept for inspection until its own lifetime ends.
async fn finish_session(state: &AppState, id: Uuid, result: &ApiResult<(StatusCode, Json<Value>)>) {
    if matches!(result, Err(e) if e.status() == StatusCode::CONFLICT) {
        return;
    }
    let outcome = state
        .store
        .update_session(id, |s| match result {
            Ok(_) => s.status = SessionStatus::Completed,
            Err(e) => {
                s.status = SessionStatus::Failed;
                s.error = Some(e.message().to_string());
            }
        })
        .await;
    if let Err(error) = outcome {
        tracing::warn!(session = %id, %error, "could not end the use-case session");
    }
}

async fn run_demo_inner(
    state: Arc<AppState>,
    demo_id: String,
    filename: Option<String>,
    upload: Option<Vec<u8>>,
    batch_id: Option<Uuid>,
    user: Option<String>,
    created: &mut Option<Uuid>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    if let Some(u) = user.as_deref() {
        crate::usage::check_run_quota(&state, u, crate::usage::Limits::from_env()).await?;
    }
    let spec = resolve(&state, &demo_id)
        .await
        .ok_or_else(|| ApiError::not_found(format!("no demo named {demo_id:?}")))?;

    let (filename, input) = match upload {
        Some(bytes) => (
            filename.unwrap_or_else(|| spec.guest_file.to_string()),
            bytes,
        ),
        None => match spec.sample.clone() {
            Some(sample) => sample,
            None => {
                return Err(ApiError::bad_request(format!(
                    "the {} use case has no built-in sample; upload a file",
                    spec.id
                )))
            }
        },
    };
    if input.is_empty() {
        return Err(ApiError::bad_request("empty file"));
    }
    if input.len() > spec.max_bytes {
        return Err(ApiError::bad_request(format!(
            "file is {} bytes; the {} demo accepts up to {}",
            input.len(),
            spec.id,
            spec.max_bytes
        )));
    }
    let ext = extension(&filename);
    if !spec.accepts.contains(&ext) {
        return Err(ApiError::bad_request(format!(
            "the {} demo accepts .{} files, not .{}",
            spec.id,
            spec.accepts.join(" / ."),
            if ext.is_empty() { "(none)" } else { &ext }
        )));
    }

    // A model step the vault would refuse fails now, before a cell is created.
    if let Some(ms) = &spec.model {
        crate::model_call::preflight(&state, ms).await?;
    }

    // Validate the input before touching FluxVM: bad uploads fail fast and cheap.
    if let Err(e) = state.fluxvm.security_capabilities().await {
        return Err(ApiError::bad_gateway(format!(
            "Cell engine not ready (/readyz): {e:#}"
        )));
    }

    let template = std::env::var("ZYVOR_DEMO_TEMPLATE").unwrap_or_else(|_| "node22-agent".into());
    let id = Uuid::new_v4();
    let sandbox = state
        .fluxvm
        .create_sandbox(
            format!("{}-{id}", spec.id),
            &template,
            Some(1800),
            state.config.egress_listen.port(),
            &crate::fluxvm::SandboxOptions {
                volumes: &[],
                resources: None,
                confidential: crate::model::Confidential::Off,
                security_profile: state.config.security_profile.as_deref(),
                gpus: None,
            },
        )
        .await
        .map_err(|e| ApiError::bad_gateway(format!("sandbox create: {e:#}")))?;
    let simulated = sandbox.simulated;

    // Confine the cell on the host before any guest work, and fail closed. A use-case cell needs no IP
    // networking (the host reaches it over vsock), so the policy denies everything and needs no gateway.
    // This used to look for the guest's default gateway and skip the policy, silently, when it was not
    // found, which is always the case before the guest has booted: the cell then ran with FluxVM's
    // default-allow policy.
    let policy = crate::confine::deny_all_policy(Some(&id.to_string()), Some(&spec.id));
    // Retried a couple of times: FluxVM's eBPF load occasionally fails once on a busy host. A cell that
    // still cannot be confined is never used.
    if let Err(e) = crate::confine::with_retries(3, std::time::Duration::from_millis(600), || {
        state.fluxvm.set_network_policy(sandbox.id, &policy)
    })
    .await
    {
        let _ = state.fluxvm.delete(sandbox.id).await;
        return Err(ApiError::bad_gateway(format!(
            "could not confine the cell, so it was not used: {e:#}"
        )));
    }

    let now = Utc::now();
    let session = SessionRecord {
        id,
        agent: spec.id.clone(),
        agent_version: "demo".into(),
        sandbox_id: sandbox.id,
        status: SessionStatus::Running,
        input: json!({ "demo": spec.id, "filename": filename, "batch_id": batch_id }),
        created_at: now,
        updated_at: now,
        last_event_seq: 0,
        guest_event_cursor: 0,
        request_id: None,
        start_policy: SessionStartPolicy::PreferWarm,
        start_mode: SessionStartMode::Cold,
        startup_ms: None,
        expires_at: None,
        sandbox_released: false,
        capability_token: format!("demo-{}", Uuid::new_v4()),
        error: None,
        parent_session_id: None,
        user_id: user.clone(),
        tainted_by: vec![],
        writes: 0,
        confidential: sandbox.confidential.clone(),
        agent_paused_reason: None,
        browse: Default::default(),
    };
    state
        .store
        .save_session(session.clone())
        .await
        .map_err(ApiError::internal)?;
    *created = Some(id);

    let goal = GoalRecord {
        id: Uuid::new_v4(),
        title: spec.goal_title.clone(),
        description: spec.goal_description.clone(),
        agent: spec.id.clone(),
        user_id: user.clone(),
        session_id: Some(id),
        status: GoalStatus::Open,
        plan: vec![],
        artifact_ids: vec![],
        allow_hosts: vec![],
        autorun: false,
        planning_session_id: None,
        proposed_plan: None,
        max_attempts: 3,
        created_at: now,
        updated_at: now,
    };
    state
        .store
        .save_goal(goal.clone())
        .await
        .map_err(ApiError::internal)?;

    let guest_path = format!("/home/agent/work/{}", spec.guest_file);
    // A real VM takes seconds to boot; talk to the guest only once its agent answers.
    crate::app::wait_for_guest_agent_ready(&state, sandbox.id)
        .await
        .map_err(|e| ApiError::bad_gateway(format!("the cell did not become ready: {e:#}")))?;
    state
        .fluxvm
        .process(sandbox.id, "mkdir -p /home/agent/work", Some(30))
        .await
        .map_err(|e| ApiError::bad_gateway(format!("mkdir: {e:#}")))?;
    state
        .fluxvm
        .fs_write(sandbox.id, &guest_path, &input, 0o644)
        .await
        .map_err(|e| ApiError::bad_gateway(format!("put-file: {e:#}")))?;

    if let Some(script) = spec.extract_script {
        state
            .fluxvm
            .fs_write(sandbox.id, GUEST_EXTRACT_SCRIPT, script.as_bytes(), 0o644)
            .await
            .map_err(|e| ApiError::bad_gateway(format!("put-script: {e:#}")))?;
    }

    let extract = match state
        .fluxvm
        .process(
            sandbox.id,
            &spec.extract_cmd.replace("{path}", &guest_path),
            // A phone photo can take tesseract tens of seconds; the other extractors finish in one or two.
            Some(if spec.extract_cmd.starts_with("tesseract") {
                120
            } else {
                60
            }),
        )
        .await
    {
        Ok(v) => {
            let stdout = v
                .get("stdout")
                .and_then(Value::as_str)
                .or_else(|| {
                    v.get("data")
                        .and_then(|d| d.get("stdout"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("")
                .trim()
                .to_string();
            if stdout.is_empty() {
                // pdftotext and tesseract print nothing both for an unreadable file and when the tool
                // is not installed (stderr is discarded), so tell the two apart.
                if let Some(tool) = ["pdftotext", "tesseract"]
                    .into_iter()
                    .find(|t| spec.extract_cmd.starts_with(t))
                {
                    let probe = state
                        .fluxvm
                        .process(sandbox.id, &format!("command -v {tool}"), Some(10))
                        .await
                        .ok()
                        .and_then(|v| {
                            v.get("stdout")
                                .and_then(Value::as_str)
                                .map(|s| s.trim().to_string())
                        })
                        .unwrap_or_default();
                    if probe.is_empty() {
                        return Err(ApiError::bad_request(if tool == "tesseract" {
                            "The template has no tesseract. Use a template with tesseract-ocr \
                             (set ZYVOR_DEMO_TEMPLATE) or bake one: see templates/node22-agent"
                        } else {
                            "The template has no pdftotext. Use a template with poppler-utils \
                             (set ZYVOR_DEMO_TEMPLATE) or bake one: see Tutorial 17"
                        }));
                    }
                }
                return Err(ApiError::bad_request(spec.empty_msg.clone()));
            }
            stdout
        }
        Err(e) => {
            let msg = format!("{e:#}");
            if spec.extract_cmd.starts_with("pdftotext")
                && (msg.contains("pdftotext") || msg.contains("127") || msg.contains("not found"))
            {
                return Err(ApiError::bad_request(
                    "Image missing poppler — bake node22-agent with pdftotext",
                ));
            }
            if spec.extract_cmd.starts_with("tesseract")
                && (msg.contains("tesseract") || msg.contains("127") || msg.contains("not found"))
            {
                return Err(ApiError::bad_request(
                    "Image missing tesseract — bake node22-agent with tesseract-ocr",
                ));
            }
            return Err(ApiError::bad_gateway(format!("extract failed: {msg}")));
        }
    };

    let mut built = spec
        .build(&filename, &extract)
        .map_err(ApiError::bad_request)?;
    // The model step runs here on the host. The cell is finished and never had a path to it.
    let mut model_outcome: Option<crate::model_call::ModelOutcome> = None;
    if let Some(ms) = &spec.model {
        let out = crate::model_call::call(&state, &session, &spec.id, ms, &extract).await?;
        if let Some(first) = built.first_mut() {
            first.body.push_str(&format!(
                "\n## Model summary\n*Generated by `{}` at {} from the extracted text. Unlike the \
                 sections above, this part is not extractive.*\n\n{}\n",
                crate::demo_builders::clean_line(&out.model),
                out.host,
                out.text
            ));
        }
        model_outcome = Some(out);
    }
    let mut goal = goal;
    let mut saved: Vec<ArtifactRecord> = Vec::new();
    for a in &built {
        let art = ArtifactRecord {
            id: Uuid::new_v4(),
            kind: a.kind.clone(),
            title: a.title.clone(),
            body: a.body.clone(),
            content_type: Some(a.content_type.into()),
            goal_id: Some(goal.id),
            session_id: Some(id),
            agent: Some(spec.id.clone()),
            metadata: json!({
                "filename": filename,
                "extract_chars": extract.chars().count(),
                "demo": spec.id,
                "batch_id": batch_id,
                "model_host": model_outcome.as_ref().map(|m| m.host.clone()),
            }),
            created_at: Utc::now(),
            expires_at: None,
        };
        state
            .store
            .save_artifact(art.clone())
            .await
            .map_err(ApiError::internal)?;
        goal.artifact_ids.push(art.id);
        saved.push(art);
    }
    goal.status = GoalStatus::Done;
    goal.updated_at = Utc::now();
    let _ = state.store.save_goal(goal.clone()).await;

    let connects = session_egress_connects(&state, id).await;
    if connects > 0 {
        let _ = state
            .store
            .update_session(id, |s| {
                s.agent_paused_reason = Some(crate::model::AgentPausedReason::EbpfDeny);
            })
            .await;
        let _ = state.fluxvm.freeze(sandbox.id).await;
        return Err(ApiError::conflict(format!(
            "demo FAIL: {connects} CONNECT/ebpf events — session frozen"
        )));
    }

    let first = &saved[0];
    let _ = state
        .store
        .audit
        .append(
            Some(id),
            AuditPhase::Performed,
            &format!("demo.{}.done", spec.id.replace('-', "_")),
            Some(spec.id.clone()),
            json!({
                "filename": filename,
                "extract_chars": extract.chars().count(),
                "artifact_id": first.id,
                "egress_connects": connects,
            }),
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "demo": spec.id,
            "batch_id": batch_id,
            "session_id": id,
            "agent": spec.id,
            "goal_id": goal.id,
            "filename": filename,
            "bytes": input.len(),
            "extract_chars": extract.chars().count(),
            "artifact_id": first.id,
            "artifact_title": first.title,
            "artifacts": saved.iter().map(|a| json!({
                "id": a.id,
                "title": a.title,
                "kind": a.kind,
            })).collect::<Vec<_>>(),
            "egress_connects": connects,
            "cockpit_url": format!("/app/merion/{id}"),
            "badge": run_badge(simulated, model_outcome.as_ref().map(|m| m.host.as_str())),
            "model_calls": u32::from(model_outcome.is_some()),
            "model": model_outcome.as_ref().map(|m| json!({
                "host": m.host,
                "model": m.model,
                "request_bytes": m.request_bytes,
                "response_bytes": m.response_bytes,
                "first_use_approved": m.approved_now,
            })),
            "honesty": run_honesty(simulated, model_outcome.as_ref().map(|m| m.host.as_str())),
        })),
    ))
}

/// The evidence badge of a run. A run in the local simulator is `simulated` and says it is not sealed; nothing else about it is claimed.
fn run_badge(simulated: bool, model_host: Option<&str>) -> Value {
    json!({
        "evidence": if simulated { "simulated" } else { "software-test" },
        "sealed": !simulated,
        "operator_can_read": true,
        "proxy": "strict",
        "browser": "none",
        "model": model_host,
    })
}

/// The one-line honesty note shown with a result.
fn run_honesty(simulated: bool, model_host: Option<&str>) -> String {
    if simulated {
        let sent = model_host
            .map(|h| format!(" Extracted text was sent to {h}."))
            .unwrap_or_default();
        return format!(
            "SIMULATED, not sealed: this ran on the operator's own machine with no VM and no network policy. \
             The connection count is not evidence. Use a Keep host with FluxVM for a sealed cell.{sent}"
        );
    }
    match model_host {
        Some(host) => format!(
            "software-test · operator can read · 0 CONNECT from the cell · extracted text sent to {host}"
        ),
        None => "software-test · operator can read · 0 CONNECT".to_string(),
    }
}

pub async fn session_egress_connects(state: &AppState, session_id: Uuid) -> u64 {
    let entries = state
        .store
        .audit
        .list(Some(session_id), 500)
        .await
        .unwrap_or_default();
    entries
        .iter()
        .filter(|e| e.action == "egress.connect" || e.action.starts_with("ebpf."))
        .count() as u64
}

fn lab_sample_pdf() -> Vec<u8> {
    br#"%PDF-1.1
1 0 obj<< /Type /Catalog /Pages 2 0 R >>endobj
2 0 obj<< /Type /Pages /Kids [3 0 R] /Count 1 >>endobj
3 0 obj<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources<< /Font<< /F1 5 0 R >> >> >>endobj
4 0 obj<< /Length 68 >>stream
BT /F1 24 Tf 72 720 Td (Keep lab sample PDF) Tj ET
endstream
endobj
5 0 obj<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>endobj
xref
0 6
0000000000 65535 f
0000000009 00000 n
0000000058 00000 n
0000000115 00000 n
0000000266 00000 n
0000000384 00000 n
trailer<< /Size 6 /Root 1 0 R >>
startxref
461
%%EOF
"#
    .to_vec()
}

/// Minimal single-page text PDF (Helvetica, one line per entry) with a correct
/// xref table, so `pdftotext` reads it without repair.
fn simple_pdf(lines: &[&str]) -> Vec<u8> {
    let mut stream = String::from("BT /F1 10 Tf 40 760 Td 13 TL\n");
    for line in lines.iter().take(50) {
        let esc = line
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        stream.push_str(&format!("({esc}) Tj T*\n"));
    }
    stream.push_str("ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }
    let xref_at = out.len();
    out.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for off in &offsets {
        out.push_str(&format!("{off:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
        objects.len() + 1
    ));
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_are_unique_and_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for d in DEMOS {
            assert!(seen.insert(d.id), "duplicate demo id {}", d.id);
            assert!(!d.accepts.is_empty());
            assert!(d.extract_cmd.contains("{path}"));
            assert!(d.goal_description.contains("No browser"));
            // Guest file name is fixed and safe: never taken from the upload.
            assert!(d
                .guest_file
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.'));
        }
        assert!(
            spec_by_id("pdf-brief").is_some(),
            "original demo id must stay"
        );
        assert!(spec_by_id("nope").is_none());
    }

    #[test]
    fn every_sample_has_an_accepted_extension_and_builds() {
        for d in DEMOS {
            let (name, bytes) = (d.sample)();
            assert!(!bytes.is_empty(), "{} sample empty", d.id);
            assert!(bytes.len() <= d.max_bytes, "{} sample too big", d.id);
            let ext = extension(name);
            assert!(d.accepts.contains(&ext.as_str()), "{} sample .{ext}", d.id);
            // Text samples are what the guest would extract, so the builder must accept them.
            if ext != "pdf" {
                let text = String::from_utf8(bytes).unwrap();
                let out = (d.build)(name, &text).unwrap_or_else(|e| panic!("{}: {e}", d.id));
                assert!(!out.is_empty() && !out[0].body.is_empty());
            }
        }
    }

    #[test]
    fn simple_pdf_has_a_consistent_xref() {
        let pdf = String::from_utf8(simple_pdf(&["Term (initial) \\ test"])).unwrap();
        assert!(pdf.starts_with("%PDF-1.4"));
        let start: usize = pdf
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(pdf[start..].starts_with("xref"));
        assert!(pdf.contains("\\(initial\\)") && pdf.contains("\\\\ test"));
    }

    async fn multipart(file: Option<(&str, &[u8])>) -> Multipart {
        use axum::extract::FromRequest;
        let boundary = "XBOUNDARYX";
        let mut body: Vec<u8> = Vec::new();
        if let Some((name, bytes)) = file {
            body.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; \
                     filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(bytes);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        let req = axum::http::Request::builder()
            .method("POST")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(axum::body::Body::from(body))
            .unwrap();
        Multipart::from_request(req, &()).await.unwrap()
    }

    async fn run_in(
        state: Arc<AppState>,
        id: &str,
        file: Option<(&str, &[u8])>,
    ) -> Result<(StatusCode, Json<Value>), ApiError> {
        demo_run(
            State(state),
            Extension(crate::authz::Principal::Operator),
            Path(id.into()),
            multipart(file).await,
        )
        .await
    }

    async fn run(
        id: &str,
        file: Option<(&str, &[u8])>,
    ) -> Result<(StatusCode, Json<Value>), ApiError> {
        run_in(crate::goals::tests::test_state().await, id, file).await
    }

    #[test]
    fn a_simulated_run_never_claims_to_be_sealed() {
        let b = run_badge(true, None);
        assert_eq!(b["evidence"], "simulated");
        assert_eq!(b["sealed"], false);
        let h = run_honesty(true, Some("api.example"));
        assert!(h.starts_with("SIMULATED, not sealed"), "{h}");
        // the simulated line must not borrow the sealed run's wording
        assert!(
            !h.contains("software-test") && !h.contains("0 CONNECT"),
            "{h}"
        );
        assert!(
            h.ends_with("Extracted text was sent to api.example."),
            "{h}"
        );
    }

    #[test]
    fn a_real_cell_run_keeps_its_software_test_badge() {
        let b = run_badge(false, None);
        assert_eq!(b["evidence"], "software-test");
        assert_eq!(b["sealed"], true);
        assert_eq!(b["operator_can_read"], true);
        assert_eq!(
            run_honesty(false, None),
            "software-test · operator can read · 0 CONNECT"
        );
        assert!(run_honesty(false, Some("m.example")).ends_with("extracted text sent to m.example"));
    }

    fn status_of(e: ApiError) -> StatusCode {
        axum::response::IntoResponse::into_response(e).status()
    }

    #[tokio::test]
    async fn unknown_demo_is_404_and_bad_uploads_are_400_before_fluxvm() {
        assert_eq!(
            status_of(run("nope", None).await.unwrap_err()),
            StatusCode::NOT_FOUND
        );
        // Wrong extension for the CSV demo.
        assert_eq!(
            status_of(
                run("csv-clean", Some(("evil.exe", b"MZ")))
                    .await
                    .unwrap_err()
            ),
            StatusCode::BAD_REQUEST
        );
        // No extension at all.
        assert_eq!(
            status_of(run("log-triage", Some(("noext", b"x"))).await.unwrap_err()),
            StatusCode::BAD_REQUEST
        );
        // Empty file.
        assert_eq!(
            status_of(run("log-triage", Some(("a.log", b""))).await.unwrap_err()),
            StatusCode::BAD_REQUEST
        );
        // Over the size limit for a text demo.
        let big = vec![b'a'; TEXT_LIMIT + 1];
        assert_eq!(
            status_of(
                run("meeting-actions", Some(("a.txt", &big)))
                    .await
                    .unwrap_err()
            ),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn a_valid_upload_reaches_fluxvm_and_fails_closed_when_it_is_down() {
        // FluxVM at 127.0.0.1:1 is unreachable, so a valid request must stop at
        // the readiness check with 502 rather than doing any guest work.
        let err = run("csv-clean", Some(("data.csv", b"a,b\n1,2\n")))
            .await
            .unwrap_err();
        assert_eq!(status_of(err), StatusCode::BAD_GATEWAY);
        let err = run("pdf-brief", None).await.unwrap_err();
        assert_eq!(status_of(err), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn demo_list_names_every_demo_with_deny_egress_and_no_browser() {
        let state = crate::goals::tests::test_state().await;
        let Json(v) = demo_list(State(state)).await;
        let demos = v["demos"].as_array().unwrap();
        assert_eq!(demos.len(), DEMOS.len());
        assert!(demos
            .iter()
            .all(|d| d["egress"] == "deny" && d["browser"] == "none"));
        assert!(demos.iter().any(|d| d["id"] == "pdf-brief"));
    }

    /// The generated sample PDFs must survive the real guest tool. Skipped where
    /// poppler is not installed.
    #[test]
    fn sample_pdfs_round_trip_through_pdftotext_when_available() {
        if std::process::Command::new("pdftotext")
            .arg("-v")
            .output()
            .is_err()
        {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        for (id, needle) in [
            ("contract-clauses", "## Governing law"),
            (
                "security-questionnaire",
                "3 questions found, 1 without an answer",
            ),
        ] {
            let spec = spec_by_id(id).unwrap();
            let (name, bytes) = (spec.sample)();
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let out = std::process::Command::new("pdftotext")
                .args(["-layout"])
                .arg(&path)
                .arg("-")
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            assert!(!text.trim().is_empty(), "{id}: pdftotext extracted nothing");
            let body = (spec.build)(name, &text).unwrap().remove(0).body;
            assert!(
                body.contains(needle),
                "{id}: expected {needle:?} in\n{body}"
            );
        }
    }

    fn custom(id: &str) -> CustomDemoSpec {
        serde_json::from_value(json!({
            "id": id,
            "title": "Invoice check",
            "description": "Totals and repeats",
            "accepts": ["txt"],
            "extract": "text",
            "summary": [
                {"kind": "keyword_sections", "title": "Totals", "keywords": ["total"]},
                {"kind": "stats"}
            ],
            "sample": {"filename": "sample.txt", "text": "Total: 10\nTotal: 20\n"}
        }))
        .unwrap()
    }

    /// The fail-closed rule hangs on this count: any `egress.connect` or `ebpf.*`
    /// audit row for the session means the run is frozen (409), and rows for other
    /// sessions or other actions must not trip it.
    #[tokio::test]
    async fn egress_connects_counts_connect_and_ebpf_rows_for_this_session_only() {
        let state = crate::goals::tests::test_state().await;
        let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(session_egress_connects(&state, mine).await, 0);
        let add = |sid: Uuid, action: &str| {
            let state = state.clone();
            let action = action.to_string();
            async move {
                state
                    .store
                    .audit
                    .append(Some(sid), AuditPhase::Performed, &action, None, json!({}))
                    .await
                    .unwrap();
            }
        };
        add(mine, "demo.pdf_brief.done").await; // not a connect
        add(other, "egress.connect").await; // someone else's connect
        assert_eq!(session_egress_connects(&state, mine).await, 0);
        add(mine, "egress.connect").await;
        add(mine, "ebpf.deny").await;
        add(mine, "ebpf.udp_deny").await;
        assert_eq!(session_egress_connects(&state, mine).await, 3);
    }

    #[tokio::test]
    async fn merion_status_reports_mode_signers_fluxvm_and_demo_counts() {
        let state = crate::goals::tests::test_state().await;
        let Json(v) = merion_status(State(state)).await;
        assert_eq!(v["merion_mode"], false);
        assert_eq!(v["trusted_signers"], 0);
        assert_eq!(v["fluxvm"]["ready"], false, "FluxVM is down in tests");
        assert!(v["fluxvm"]["error"].is_string());
        assert_eq!(v["demos"]["builtin"], DEMOS.len());
        assert_eq!(v["demos"]["custom"], 0);
    }

    #[tokio::test]
    async fn a_custom_use_case_can_be_saved_listed_resolved_and_deleted() {
        let state = crate::goals::tests::test_state().await;
        let (code, Json(first)) = demo_save(State(state.clone()), Json(custom("invoice-check")))
            .await
            .unwrap();
        assert_eq!(
            (code, first["replaced"].clone()),
            (StatusCode::CREATED, json!(false))
        );
        // Saving again replaces it.
        let (code, Json(v)) = demo_save(State(state.clone()), Json(custom("invoice-check")))
            .await
            .unwrap();
        assert_eq!((code, v["replaced"].clone()), (StatusCode::OK, json!(true)));

        let Json(list) = demo_list(State(state.clone())).await;
        let mine = list["demos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == "invoice-check")
            .expect("custom demo is listed");
        assert_eq!(
            (mine["builtin"].clone(), mine["egress"].clone()),
            (json!(false), json!("deny"))
        );

        // It resolves to the same runner shape as a built-in and renders through its rules.
        let r = resolve(&state, "invoice-check").await.unwrap();
        assert_eq!(r.guest_file, "input.txt");
        assert!(r.extract_cmd.starts_with("head -c ") && r.extract_cmd.contains("{path}"));
        let (name, bytes) = r.sample.clone().unwrap();
        let out = r
            .build(&name, std::str::from_utf8(&bytes).unwrap())
            .unwrap();
        assert!(out[0].body.contains("Total: 10") && out[0].title == "summary.md");

        // A valid upload reaches FluxVM (down in tests), so it fails closed with 502.
        let err = run_in(
            state.clone(),
            "invoice-check",
            Some(("a.txt", b"Total: 1\n")),
        )
        .await;
        assert_eq!(status_of(err.unwrap_err()), StatusCode::BAD_GATEWAY);
        // Wrong file type is refused before FluxVM is touched.
        let err = run_in(state.clone(), "invoice-check", Some(("a.pdf", b"%PDF"))).await;
        assert_eq!(status_of(err.unwrap_err()), StatusCode::BAD_REQUEST);
        // With no upload its own sample is used (and then FluxVM is down).
        let err = run_in(state.clone(), "invoice-check", None).await;
        assert_eq!(status_of(err.unwrap_err()), StatusCode::BAD_GATEWAY);

        assert_eq!(
            demo_delete(State(state.clone()), Path("invoice-check".into()))
                .await
                .unwrap(),
            StatusCode::NO_CONTENT
        );
        assert!(resolve(&state, "invoice-check").await.is_none());
        let gone = demo_delete(State(state), Path("invoice-check".into())).await;
        assert_eq!(status_of(gone.unwrap_err()), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn custom_specs_cannot_shadow_or_delete_built_ins_or_exceed_limits() {
        let state = crate::goals::tests::test_state().await;
        let err = demo_save(State(state.clone()), Json(custom("pdf-brief")))
            .await
            .unwrap_err();
        assert_eq!(status_of(err), StatusCode::BAD_REQUEST);
        let err = demo_delete(State(state.clone()), Path("pdf-brief".into()))
            .await
            .unwrap_err();
        assert_eq!(status_of(err), StatusCode::BAD_REQUEST);

        let mut bad = custom("wrong-ext");
        bad.accepts = vec!["pdf".into()]; // text extractor cannot read pdf
        let err = demo_save(State(state.clone()), Json(bad))
            .await
            .unwrap_err();
        assert_eq!(status_of(err), StatusCode::BAD_REQUEST);

        for i in 0..MAX_CUSTOM_DEMOS {
            let (code, Json(_)) = demo_save(State(state.clone()), Json(custom(&format!("uc-{i}"))))
                .await
                .unwrap();
            assert_eq!(code, StatusCode::CREATED);
        }
        let err = demo_save(State(state), Json(custom("one-too-many")))
            .await
            .unwrap_err();
        assert_eq!(status_of(err), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn custom_specs_survive_a_store_reopen() {
        let root = std::env::temp_dir().join(format!("zyvor-demo-specs-{}", Uuid::new_v4()));
        {
            let store = crate::store::Store::open(&root).await.unwrap();
            let now = Utc::now();
            store
                .save_demo_spec(CustomDemoRecord {
                    spec: custom("kept"),
                    created_at: now,
                    updated_at: now,
                })
                .await
                .unwrap();
        }
        let store = crate::store::Store::open(&root).await.unwrap();
        assert_eq!(
            store.get_demo_spec("kept").await.unwrap().spec,
            custom("kept")
        );
        assert!(store.delete_demo_spec("kept").await.unwrap());
        let store = crate::store::Store::open(&root).await.unwrap();
        assert!(store.list_demo_specs().await.is_empty());
    }

    #[test]
    fn extension_is_lowercased_and_missing_is_empty() {
        assert_eq!(extension("A.PDF"), "pdf");
        assert_eq!(extension("noext"), "");
        assert_eq!(extension("a.tar.gz"), "gz");
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in files {
            w.start_file(*name, opts).unwrap();
            w.write_all(bytes).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn csv_only() -> Vec<String> {
        vec!["csv".into()]
    }

    #[test]
    fn zip_expansion_keeps_only_accepted_safe_files() {
        let z = zip_of(&[
            ("a.csv", b"x\n1\n"),
            ("nested/dir/b.CSV", b"y\n2\n"),
            ("notes.txt", b"ignored"),
            (".hidden.csv", b"ignored"),
            ("__MACOSX/c.csv", b"ignored"),
            ("../../evil.csv", b"unsafe path"),
        ]);
        let out = expand_zip(&z, &csv_only(), 1024).unwrap();
        let names: Vec<_> = out.iter().map(|u| u.filename.clone().unwrap()).collect();
        // Only the base name is used, so a hostile path can never reach a file system.
        assert_eq!(names, ["a.csv", "b.CSV"]);
        assert_eq!(out[1].bytes, b"y\n2\n");
    }

    #[test]
    fn zip_expansion_enforces_sizes_on_what_is_read() {
        let big = vec![b'x'; 2000];
        let z = zip_of(&[("big.csv", &big), ("ok.csv", b"a\n1\n")]);
        let out = expand_zip(&z, &csv_only(), 1000).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].filename.as_deref(), Some("ok.csv"));
        let err = expand_zip(&zip_of(&[("big.csv", &big)]), &csv_only(), 1000)
            .err()
            .unwrap();
        assert!(
            err.contains("no .csv files") && err.contains("skipped"),
            "{err}"
        );
    }

    #[test]
    fn zip_expansion_refuses_too_many_files_and_non_zips() {
        let many: Vec<(String, Vec<u8>)> = (0..=MAX_BATCH_FILES)
            .map(|i| (format!("f{i}.csv"), b"a\n1\n".to_vec()))
            .collect();
        let refs: Vec<(&str, &[u8])> = many
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        let err = expand_zip(&zip_of(&refs), &csv_only(), 1024).err().unwrap();
        assert!(err.contains("more than 20"), "{err}");
        assert!(expand_zip(b"PK not really", &csv_only(), 1024).is_err());
        assert!(expand_zip(&zip_of(&[]), &csv_only(), 1024).is_err());
    }
}
