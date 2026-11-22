// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! Skills: small immutable bundles of instructions and helper files (a
//! `SKILL.md` plus scripts or reference files) that an agent can read at run
//! time. A skill without a `scope` is *base* and mounts at
//! `/opt/zyvor/skills/<name>`. A skill with a `scope` mounts at
//! `/opt/zyvor/skills-scoped/<name>` and only for agents whose `skill_scope`
//! the operator's scope file allows to use it.
//!
//! Like agent deployments, a skill version is the SHA-256 of its content, and
//! an agent pins exact versions when it is deployed, so republishing a skill
//! never changes what an already deployed agent sees.

use anyhow::{bail, Context, Result};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};
use tokio::fs;

pub const MAX_SKILL_FILES: usize = 32;
pub const MAX_SKILL_FILE_BYTES: usize = 512 * 1024;
pub const MAX_SKILL_TOTAL_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_AGENT_SKILLS: usize = 16;
const MAX_DESCRIPTION_CHARS: usize = 500;
const MAX_SCOPE_CHARS: usize = 40;

pub const BASE_MOUNT: &str = "/opt/zyvor/skills";
pub const SCOPED_MOUNT: &str = "/opt/zyvor/skills-scoped";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    /// Relative path inside the skill, e.g. `SKILL.md` or `scripts/run.sh`.
    pub path: String,
    pub content_base64: String,
    #[serde(default)]
    pub executable: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublishSkillRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Absent means a base skill available to every agent.
    #[serde(default)]
    pub scope: Option<String>,
    pub files: Vec<SkillFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFileInfo {
    pub path: String,
    pub size: usize,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub name: String,
    pub version: String,
    pub digest_sha256: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    pub files: Vec<SkillFileInfo>,
    pub created_at: DateTime<Utc>,
}

/// What is stored on disk for one version: the record plus the file contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillBundle {
    pub record: SkillRecord,
    pub files: Vec<SkillFile>,
}

/// Operator policy: which skill scopes each agent `skill_scope` may mount.
///
/// ```json
/// {"scopes": {"prod": ["prod"], "internal-test": ["prod", "internal-test"]}}
/// ```
///
/// Base skills (no scope) need no entry. With no file, scoped skills are
/// unusable: the policy fails closed.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillScopes {
    #[serde(default)]
    scopes: HashMap<String, Vec<String>>,
}

impl SkillScopes {
    pub async fn load(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let raw = fs::read(path)
            .await
            .with_context(|| format!("reading skill scopes file {}", path.display()))?;
        serde_json::from_slice(&raw)
            .with_context(|| format!("parsing skill scopes file {}", path.display()))
    }

    pub fn from_map(scopes: HashMap<String, Vec<String>>) -> Self {
        Self { scopes }
    }

    /// May an agent with `agent_scope` mount a skill tagged `skill_scope`?
    pub fn allows(&self, agent_scope: Option<&str>, skill_scope: Option<&str>) -> bool {
        let Some(skill_scope) = skill_scope else {
            return true;
        };
        agent_scope
            .and_then(|a| self.scopes.get(a))
            .is_some_and(|allowed| allowed.iter().any(|s| s == skill_scope))
    }
}

/// Split `name` or `name@version`.
pub fn split_ref(reference: &str) -> (&str, Option<&str>) {
    match reference.split_once('@') {
        Some((name, version)) => (name, Some(version)),
        None => (reference, None),
    }
}

pub struct SkillStore {
    root: PathBuf,
}

impl SkillStore {
    pub async fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).await?;
        Ok(Self { root })
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// Validate and store a skill version; publishing identical content again
    /// is a no-op that returns the existing version.
    pub async fn publish(&self, req: PublishSkillRequest) -> Result<SkillRecord> {
        validate_name_component(&req.name, "skill name", 80)?;
        if let Some(scope) = &req.scope {
            validate_name_component(scope, "skill scope", MAX_SCOPE_CHARS)?;
        }
        if let Some(description) = &req.description {
            if description.chars().count() > MAX_DESCRIPTION_CHARS {
                bail!("description may not exceed {MAX_DESCRIPTION_CHARS} characters");
            }
        }
        let decoded = validate_files(&req.files)?;

        let mut hasher = Sha256::new();
        for part in [req.name.as_str(), req.scope.as_deref().unwrap_or("")] {
            hasher.update(part.as_bytes());
            hasher.update([0x1f]);
        }
        let sorted: BTreeMap<&str, (&[u8], bool)> = req
            .files
            .iter()
            .zip(&decoded)
            .map(|(f, bytes)| (f.path.as_str(), (bytes.as_slice(), f.executable)))
            .collect();
        for (path, (bytes, executable)) in &sorted {
            hasher.update(path.as_bytes());
            hasher.update([0, u8::from(*executable)]);
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        }
        let digest = hex::encode(hasher.finalize());
        let version = digest[..12].to_string();

        let dir = self.dir(&req.name);
        let path = dir.join(format!("{version}.json"));
        if let Ok(raw) = fs::read(&path).await {
            if let Ok(existing) = serde_json::from_slice::<SkillBundle>(&raw) {
                self.set_current(&req.name, &version).await?;
                return Ok(existing.record);
            }
        }

        let record = SkillRecord {
            name: req.name.clone(),
            version: version.clone(),
            digest_sha256: digest,
            description: req.description,
            scope: req.scope,
            files: req
                .files
                .iter()
                .zip(&decoded)
                .map(|(f, bytes)| SkillFileInfo {
                    path: f.path.clone(),
                    size: bytes.len(),
                    executable: f.executable,
                })
                .collect(),
            created_at: Utc::now(),
        };
        fs::create_dir_all(&dir).await?;
        let bundle = SkillBundle {
            record: record.clone(),
            files: req.files,
        };
        crate::store::atomic_write(&path, &serde_json::to_vec(&bundle)?).await?;
        self.set_current(&req.name, &version).await?;
        Ok(record)
    }

    async fn set_current(&self, name: &str, version: &str) -> Result<()> {
        crate::store::atomic_write(&self.dir(name).join("current"), version.as_bytes()).await
    }

    pub async fn current_version(&self, name: &str) -> Option<String> {
        let raw = fs::read_to_string(self.dir(name).join("current"))
            .await
            .ok()?;
        let version = raw.trim();
        (!version.is_empty()).then(|| version.to_string())
    }

    /// Load one version; `None` means the current version.
    pub async fn get(&self, name: &str, version: Option<&str>) -> Result<SkillBundle> {
        validate_name_component(name, "skill name", 80)?;
        let version = match version {
            Some(v) => v.to_string(),
            None => self
                .current_version(name)
                .await
                .with_context(|| format!("skill '{name}' not found"))?,
        };
        if !version.bytes().all(|b| b.is_ascii_hexdigit()) || version.len() > 64 {
            bail!("invalid skill version '{version}'");
        }
        let raw = fs::read(self.dir(name).join(format!("{version}.json")))
            .await
            .with_context(|| format!("skill '{name}' version '{version}' not found"))?;
        serde_json::from_slice(&raw).context("decoding stored skill")
    }

    /// The current version of every skill.
    pub async fn list(&self) -> Result<Vec<SkillRecord>> {
        let mut out = Vec::new();
        let mut rd = fs::read_dir(&self.root).await?;
        while let Some(entry) = rd.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Ok(bundle) = self.get(&name, None).await {
                out.push(bundle.record);
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    pub async fn versions(&self, name: &str) -> Result<Vec<SkillRecord>> {
        validate_name_component(name, "skill name", 80)?;
        let mut out = Vec::new();
        let mut rd = fs::read_dir(self.dir(name))
            .await
            .with_context(|| format!("skill '{name}' not found"))?;
        while let Some(entry) = rd.next_entry().await? {
            let file = entry.file_name().to_string_lossy().into_owned();
            let Some(version) = file.strip_suffix(".json") else {
                continue;
            };
            if let Ok(bundle) = self.get(name, Some(version)).await {
                out.push(bundle.record);
            }
        }
        out.sort_by_key(|r| r.created_at);
        Ok(out)
    }

    pub async fn delete(&self, name: &str) -> Result<bool> {
        validate_name_component(name, "skill name", 80)?;
        match fs::remove_dir_all(self.dir(name)).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Turn an agent's `skills` list (`name` or `name@version`) into exact
    /// `name@version` pins, checking that each exists and that `scopes` lets an
    /// agent with `agent_scope` use it.
    pub async fn pin(
        &self,
        refs: &[String],
        agent_scope: Option<&str>,
        scopes: &SkillScopes,
    ) -> Result<Vec<String>> {
        if refs.len() > MAX_AGENT_SKILLS {
            bail!("an agent may use at most {MAX_AGENT_SKILLS} skills");
        }
        let mut pinned = Vec::with_capacity(refs.len());
        let mut seen = HashSet::new();
        for reference in refs {
            let (name, version) = split_ref(reference);
            if !seen.insert(name.to_string()) {
                bail!("skill '{name}' is listed more than once");
            }
            let bundle = self.get(name, version).await?;
            if !scopes.allows(agent_scope, bundle.record.scope.as_deref()) {
                bail!(
                    "skill '{name}' is scoped to '{}', which this agent's skill_scope {} is not allowed to use",
                    bundle.record.scope.as_deref().unwrap_or(""),
                    agent_scope.map_or("(none)".to_string(), |s| format!("'{s}'")),
                );
            }
            pinned.push(format!("{name}@{}", bundle.record.version));
        }
        Ok(pinned)
    }
}

/// The files to write into a sandbox for `bundle`, as (guest path, bytes, mode).
pub fn mount_plan(bundle: &SkillBundle) -> Result<Vec<(String, Vec<u8>, u32)>> {
    let root = if bundle.record.scope.is_some() {
        SCOPED_MOUNT
    } else {
        BASE_MOUNT
    };
    bundle
        .files
        .iter()
        .map(|f| {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&f.content_base64)
                .with_context(|| format!("decoding {}", f.path))?;
            let mode = if f.executable { 0o555 } else { 0o444 };
            Ok((
                format!("{root}/{}/{}", bundle.record.name, f.path),
                bytes,
                mode,
            ))
        })
        .collect()
}

/// `INDEX.json` content for one mount root: what is there and at which version.
pub fn index_json(bundles: &[&SkillBundle]) -> Vec<u8> {
    let items: Vec<_> = bundles
        .iter()
        .map(|b| {
            let root = if b.record.scope.is_some() {
                SCOPED_MOUNT
            } else {
                BASE_MOUNT
            };
            serde_json::json!({
                "name": b.record.name,
                "version": b.record.version,
                "description": b.record.description,
                "path": format!("{root}/{}", b.record.name),
            })
        })
        .collect();
    serde_json::to_vec_pretty(&serde_json::json!({"skills": items})).unwrap_or_default()
}

fn validate_name_component(value: &str, what: &str, max: usize) -> Result<()> {
    if value.is_empty() || value.len() > max {
        bail!("{what} must be 1..={max} characters");
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || value.starts_with('.')
    {
        bail!("{what} may contain only ASCII letters, digits, '.', '_' and '-', and may not start with '.'");
    }
    Ok(())
}

/// Check every file and return the decoded contents in the same order.
fn validate_files(files: &[SkillFile]) -> Result<Vec<Vec<u8>>> {
    if files.is_empty() || files.len() > MAX_SKILL_FILES {
        bail!("a skill needs between 1 and {MAX_SKILL_FILES} files");
    }
    if !files.iter().any(|f| f.path == "SKILL.md") {
        bail!("a skill must contain a top-level SKILL.md");
    }
    let mut seen = HashSet::new();
    let mut total = 0usize;
    let mut decoded = Vec::with_capacity(files.len());
    for file in files {
        let path = &file.path;
        if path.is_empty()
            || path.len() > 128
            || path.starts_with('/')
            || !path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
            || path
                .split('/')
                .any(|c| c.is_empty() || c == "." || c == "..")
        {
            bail!("invalid skill file path {path:?}: use a relative path of [A-Za-z0-9._/-] with no empty, '.' or '..' parts");
        }
        if !seen.insert(path.as_str()) {
            bail!("duplicate skill file path {path:?}");
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&file.content_base64)
            .with_context(|| format!("{path}: content_base64 is not valid base64"))?;
        if bytes.len() > MAX_SKILL_FILE_BYTES {
            bail!("{path} exceeds {MAX_SKILL_FILE_BYTES} bytes");
        }
        total += bytes.len();
        if total > MAX_SKILL_TOTAL_BYTES {
            bail!("skill exceeds {MAX_SKILL_TOTAL_BYTES} bytes in total");
        }
        decoded.push(bytes);
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn file(path: &str, content: &str) -> SkillFile {
        SkillFile {
            path: path.into(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(content),
            executable: false,
        }
    }

    fn request(name: &str, scope: Option<&str>) -> PublishSkillRequest {
        PublishSkillRequest {
            name: name.into(),
            description: Some("does things".into()),
            scope: scope.map(str::to_string),
            files: vec![
                file("SKILL.md", "# skill"),
                file("scripts/run.sh", "echo hi"),
            ],
        }
    }

    async fn store() -> (SkillStore, PathBuf) {
        let root = std::env::temp_dir().join(format!("zyvor-skills-{}", Uuid::new_v4()));
        (SkillStore::open(&root).await.unwrap(), root)
    }

    fn scopes(pairs: &[(&str, &[&str])]) -> SkillScopes {
        SkillScopes::from_map(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
                .collect(),
        )
    }

    #[tokio::test]
    async fn publish_is_content_addressed_and_idempotent() {
        let (store, root) = store().await;
        let a = store.publish(request("notes", None)).await.unwrap();
        let b = store.publish(request("notes", None)).await.unwrap();
        assert_eq!(a.version, b.version);
        assert_eq!(a.digest_sha256.len(), 64);

        let mut changed = request("notes", None);
        changed.files[0] = file("SKILL.md", "# different");
        let c = store.publish(changed).await.unwrap();
        assert_ne!(a.version, c.version);
        assert_eq!(store.current_version("notes").await.unwrap(), c.version);
        // The old version is still retrievable by pin.
        assert_eq!(
            store
                .get("notes", Some(&a.version))
                .await
                .unwrap()
                .record
                .version,
            a.version
        );
        assert_eq!(store.versions("notes").await.unwrap().len(), 2);
        let _ = fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn version_ignores_file_order_but_not_scope_or_mode() {
        let (store, root) = store().await;
        let a = store.publish(request("s", None)).await.unwrap();
        let mut reversed = request("s", None);
        reversed.files.reverse();
        assert_eq!(store.publish(reversed).await.unwrap().version, a.version);
        assert_ne!(
            store
                .publish(request("s", Some("prod")))
                .await
                .unwrap()
                .version,
            a.version
        );
        let mut exec = request("s", None);
        exec.files[1].executable = true;
        assert_ne!(store.publish(exec).await.unwrap().version, a.version);
        let _ = fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn rejects_invalid_skills() {
        let (store, root) = store().await;
        for bad_path in ["../x", "/abs", "a//b", "a/./b", "a b", "", "a/../b", "café"] {
            let mut req = request("s", None);
            req.files.push(file(bad_path, "x"));
            assert!(store.publish(req).await.is_err(), "path {bad_path:?}");
        }
        let mut no_skill_md = request("s", None);
        no_skill_md.files.remove(0);
        assert!(store.publish(no_skill_md).await.is_err());
        let mut dup = request("s", None);
        dup.files.push(file("SKILL.md", "again"));
        assert!(store.publish(dup).await.is_err());
        let mut big = request("s", None);
        big.files
            .push(file("big.bin", &"x".repeat(MAX_SKILL_FILE_BYTES + 1)));
        assert!(store.publish(big).await.is_err());
        let mut many = request("s", None);
        for i in 0..MAX_SKILL_FILES {
            many.files.push(file(&format!("f{i}"), "x"));
        }
        assert!(store.publish(many).await.is_err());
        for bad_name in ["", ".hidden", "a/b", "a b", "../x"] {
            assert!(
                store.publish(request(bad_name, None)).await.is_err(),
                "name {bad_name:?}"
            );
        }
        assert!(store
            .publish(request("s", Some("bad scope")))
            .await
            .is_err());
        let mut bad_b64 = request("s", None);
        bad_b64.files[0].content_base64 = "***".into();
        assert!(store.publish(bad_b64).await.is_err());
        let _ = fs::remove_dir_all(root).await;
    }

    #[test]
    fn scope_matrix() {
        let policy = scopes(&[
            ("prod", &["prod"]),
            ("internal-test", &["prod", "internal-test"]),
        ]);
        // Base skills need no scope at all.
        assert!(policy.allows(None, None));
        assert!(policy.allows(Some("unknown"), None));
        // Scoped skills need an agent scope that the policy maps to them.
        assert!(policy.allows(Some("prod"), Some("prod")));
        assert!(!policy.allows(Some("prod"), Some("internal-test")));
        assert!(policy.allows(Some("internal-test"), Some("internal-test")));
        assert!(!policy.allows(None, Some("prod")));
        assert!(!policy.allows(Some("unknown"), Some("prod")));
        // No policy file means scoped skills are unusable.
        assert!(!SkillScopes::default().allows(Some("prod"), Some("prod")));
    }

    #[tokio::test]
    async fn pin_resolves_versions_and_enforces_scope() {
        let (store, root) = store().await;
        let base = store.publish(request("base-skill", None)).await.unwrap();
        let secret = store
            .publish(request("secret-skill", Some("internal-test")))
            .await
            .unwrap();
        let policy = scopes(&[("internal-test", &["internal-test"]), ("prod", &["prod"])]);

        let pinned = store
            .pin(
                &["base-skill".into(), "secret-skill".into()],
                Some("internal-test"),
                &policy,
            )
            .await
            .unwrap();
        assert_eq!(
            pinned,
            vec![
                format!("base-skill@{}", base.version),
                format!("secret-skill@{}", secret.version)
            ]
        );
        // Explicit versions are honoured, and re-pinning a pin is stable.
        assert_eq!(
            store
                .pin(&pinned, Some("internal-test"), &policy)
                .await
                .unwrap(),
            pinned
        );

        // Wrong or missing agent scope cannot pin the scoped skill.
        assert!(store
            .pin(&["secret-skill".into()], Some("prod"), &policy)
            .await
            .is_err());
        assert!(store
            .pin(&["secret-skill".into()], None, &policy)
            .await
            .is_err());
        // Unknown skill / version, duplicates, and too many skills are refused.
        assert!(store.pin(&["nope".into()], None, &policy).await.is_err());
        assert!(store
            .pin(&["base-skill@deadbeef".into()], None, &policy)
            .await
            .is_err());
        assert!(store
            .pin(&["base-skill".into(), "base-skill".into()], None, &policy)
            .await
            .is_err());
        let many: Vec<String> = (0..=MAX_AGENT_SKILLS).map(|i| format!("s{i}")).collect();
        assert!(store.pin(&many, None, &policy).await.is_err());
        let _ = fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn pinned_version_survives_a_republish() {
        let (store, root) = store().await;
        let policy = SkillScopes::default();
        let v1 = store.publish(request("evolving", None)).await.unwrap();
        let pinned = store
            .pin(&["evolving".into()], None, &policy)
            .await
            .unwrap();
        let mut next = request("evolving", None);
        next.files[0] = file("SKILL.md", "# v2");
        store.publish(next).await.unwrap();
        let (name, version) = split_ref(&pinned[0]);
        let bundle = store.get(name, version).await.unwrap();
        assert_eq!(bundle.record.version, v1.version);
        let _ = fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn mount_plan_separates_base_and_scoped_and_sets_modes() {
        let (store, root) = store().await;
        store.publish(request("base-skill", None)).await.unwrap();
        let mut scoped = request("scoped-skill", Some("prod"));
        scoped.files[1].executable = true;
        store.publish(scoped).await.unwrap();

        let base = store.get("base-skill", None).await.unwrap();
        let plan = mount_plan(&base).unwrap();
        assert!(plan
            .iter()
            .any(|(p, _, m)| p == "/opt/zyvor/skills/base-skill/SKILL.md" && *m == 0o444));

        let scoped = store.get("scoped-skill", None).await.unwrap();
        let plan = mount_plan(&scoped).unwrap();
        assert!(plan
            .iter()
            .all(|(p, _, _)| p.starts_with("/opt/zyvor/skills-scoped/scoped-skill/")));
        assert!(plan
            .iter()
            .any(|(p, b, m)| p.ends_with("scripts/run.sh") && b == b"echo hi" && *m == 0o555));

        let index: serde_json::Value = serde_json::from_slice(&index_json(&[&base])).unwrap();
        assert_eq!(index["skills"][0]["name"], "base-skill");
        assert_eq!(index["skills"][0]["path"], "/opt/zyvor/skills/base-skill");
        let _ = fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn scope_policy_loads_from_a_file_and_defaults_closed() {
        let path = std::env::temp_dir().join(format!("zyvor-scopes-{}.json", Uuid::new_v4()));
        fs::write(&path, r#"{"scopes": {"prod": ["prod"]}}"#)
            .await
            .unwrap();
        let policy = SkillScopes::load(Some(&path)).await.unwrap();
        assert!(policy.allows(Some("prod"), Some("prod")));
        assert!(!policy.allows(Some("prod"), Some("other")));
        fs::write(&path, "not json").await.unwrap();
        assert!(SkillScopes::load(Some(&path)).await.is_err());
        assert!(
            SkillScopes::load(Some(Path::new("/nonexistent/scopes.json")))
                .await
                .is_err()
        );
        assert!(!SkillScopes::load(None)
            .await
            .unwrap()
            .allows(Some("prod"), Some("prod")));
        let _ = fs::remove_file(path).await;
    }

    #[tokio::test]
    async fn list_and_delete() {
        let (store, root) = store().await;
        store.publish(request("b", None)).await.unwrap();
        store.publish(request("a", None)).await.unwrap();
        let names: Vec<_> = store
            .list()
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, ["a", "b"]);
        assert!(store.delete("a").await.unwrap());
        assert!(!store.delete("a").await.unwrap());
        assert!(store.get("a", None).await.is_err());
        let _ = fs::remove_dir_all(root).await;
    }
}
