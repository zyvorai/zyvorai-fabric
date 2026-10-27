// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

use crate::audit::AuditLog;
use crate::demo_rules::CustomDemoRecord;
use crate::devices::DeviceRecord;
use crate::goals::{ArtifactRecord, GoalRecord};
use crate::model::{
    AgentRecord, ApprovalKind, ApprovalRecord, ApprovalStatus, DeployAgentRequest, GrantScope,
    LoopRecord, ScheduleRecord, SessionEvent, SessionRecord, SessionStatus, WarmSandboxRecord,
    WarmSandboxState, WebhookRecord, WorkstationRecord,
};
use crate::model_call::ModelGrant;
use crate::skills::SkillStore;
use crate::triggers::TriggerRecord;
use anyhow::{bail, Context, Result};
use base64::Engine;
use chrono::Utc;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::RwLock,
};
use uuid::Uuid;

pub struct Store {
    root: PathBuf,
    agents: RwLock<HashMap<String, AgentRecord>>,
    sessions: RwLock<HashMap<Uuid, SessionRecord>>,
    warm_sandboxes: RwLock<HashMap<Uuid, WarmSandboxRecord>>,
    schedules: RwLock<HashMap<Uuid, ScheduleRecord>>,
    workstations: RwLock<HashMap<Uuid, WorkstationRecord>>,
    webhooks: RwLock<HashMap<Uuid, WebhookRecord>>,
    loops: RwLock<HashMap<Uuid, LoopRecord>>,
    approvals: RwLock<HashMap<Uuid, ApprovalRecord>>,
    goals: RwLock<HashMap<Uuid, GoalRecord>>,
    artifacts: RwLock<HashMap<Uuid, ArtifactRecord>>,
    /// User-defined one-click demos (declarative specs), keyed by id.
    demo_specs: RwLock<HashMap<String, CustomDemoRecord>>,
    /// What starts a use case without an upload: signed webhooks and watched folders.
    triggers: RwLock<HashMap<Uuid, TriggerRecord>>,
    /// Approved (use case, endpoint, model, credential) combinations for model-assisted use cases.
    model_grants: RwLock<HashMap<String, ModelGrant>>,
    /// Per user: tokens issued before this instant are refused (a phone was lost, say).
    token_floors: RwLock<HashMap<String, chrono::DateTime<chrono::Utc>>>,
    /// Enrolled phones, keyed `user/device`.
    devices: RwLock<HashMap<String, DeviceRecord>>,
    /// Operator-set auto-approval policies, keyed `user/agent`.
    changeset_policies: RwLock<HashMap<String, crate::changeset_policy::StoredPolicy>>,
    /// Tamper-evident record of planned, approved, denied and performed actions.
    pub audit: AuditLog,
    /// Immutable, content-addressed skill bundles agents can mount.
    pub skills: SkillStore,
    /// Conversation threads and their messages, per user.
    pub threads: crate::threads::ThreadStore,
    /// Opt-in personal memory, per user.
    pub memory: crate::memory::MemoryStore,
    /// Opt-in suggestions agents make to a person, per user.
    pub suggestions: crate::suggestions::SuggestionStore,
    /// Receipts for approved actions, and the answers to repeated keyed requests.
    pub receipts: crate::receipts::ReceiptStore,
    /// A person's own connections to outside accounts (refresh tokens, write-only).
    pub connections: crate::connections::ConnectionStore,
}

impl Store {
    pub async fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("agents")).await?;
        fs::create_dir_all(root.join("sessions")).await?;
        let audit = AuditLog::open(root.join("audit.jsonl")).await?;
        let skills = SkillStore::open(root.join("skills")).await?;
        let threads = crate::threads::ThreadStore::open(root.join("threads")).await?;
        let memory = crate::memory::MemoryStore::open(root.join("memory")).await?;
        let suggestions =
            crate::suggestions::SuggestionStore::open(root.join("suggestions")).await?;
        let receipts = crate::receipts::ReceiptStore::open(root.join("receipts")).await?;
        let connections =
            crate::connections::ConnectionStore::open(root.join("connections")).await?;

        let store = Self {
            audit,
            skills,
            threads,
            memory,
            suggestions,
            receipts,
            connections,
            root,
            agents: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            warm_sandboxes: RwLock::new(HashMap::new()),
            schedules: RwLock::new(HashMap::new()),
            workstations: RwLock::new(HashMap::new()),
            webhooks: RwLock::new(HashMap::new()),
            loops: RwLock::new(HashMap::new()),
            approvals: RwLock::new(HashMap::new()),
            goals: RwLock::new(HashMap::new()),
            artifacts: RwLock::new(HashMap::new()),
            demo_specs: RwLock::new(HashMap::new()),
            triggers: RwLock::new(HashMap::new()),
            model_grants: RwLock::new(HashMap::new()),
            token_floors: RwLock::new(HashMap::new()),
            devices: RwLock::new(HashMap::new()),
            changeset_policies: RwLock::new(HashMap::new()),
        };
        store.load().await?;
        Ok(store)
    }

    async fn load(&self) -> Result<()> {
        let mut agents = HashMap::new();
        let mut rd = fs::read_dir(self.root.join("agents")).await?;
        while let Some(entry) = rd.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let current = entry.path().join("current.json");
            if let Ok(raw) = fs::read(&current).await {
                if let Ok(record) = serde_json::from_slice::<AgentRecord>(&raw) {
                    agents.insert(record.name.clone(), record);
                }
            }
        }
        *self.agents.write().await = agents;

        let mut sessions = HashMap::new();
        let mut rd = fs::read_dir(self.root.join("sessions")).await?;
        while let Some(entry) = rd.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let path = entry.path().join("session.json");
            if let Ok(raw) = fs::read(&path).await {
                if let Ok(record) = serde_json::from_slice::<SessionRecord>(&raw) {
                    sessions.insert(record.id, record);
                }
            }
        }
        *self.sessions.write().await = sessions;

        let warm_path = self.root.join("warm-pools.json");
        let warm_records = match fs::read(&warm_path).await {
            Ok(raw) => serde_json::from_slice::<Vec<WarmSandboxRecord>>(&raw)
                .context("decoding warm-pools.json")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        *self.warm_sandboxes.write().await = warm_records
            .into_iter()
            .map(|record| (record.sandbox_id, record))
            .collect();
        *self.schedules.write().await = read_id_map(&self.root.join("schedules.json")).await?;
        *self.workstations.write().await =
            read_id_map(&self.root.join("workstations.json")).await?;
        *self.webhooks.write().await = read_id_map(&self.root.join("webhooks.json")).await?;
        *self.loops.write().await = read_id_map(&self.root.join("loops.json")).await?;
        *self.approvals.write().await = read_id_map(&self.root.join("approvals.json")).await?;
        *self.goals.write().await = read_id_map(&self.root.join("goals.json")).await?;
        *self.artifacts.write().await = read_id_map(&self.root.join("artifacts.json")).await?;
        *self.triggers.write().await = read_id_map(&self.root.join("triggers.json")).await?;
        *self.token_floors.write().await = match fs::read(self.root.join("user_token_floors.json"))
            .await
        {
            Ok(raw) => serde_json::from_slice(&raw).context("decoding user_token_floors.json")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => return Err(e.into()),
        };
        let devices = match fs::read(self.root.join("devices.json")).await {
            Ok(raw) => serde_json::from_slice::<Vec<DeviceRecord>>(&raw)
                .context("decoding devices.json")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        *self.devices.write().await = devices
            .into_iter()
            .map(|d| (format!("{}/{}", d.user_id, d.device_id), d))
            .collect();
        let policies = match fs::read(self.root.join("changeset_policies.json")).await {
            Ok(raw) => serde_json::from_slice::<Vec<crate::changeset_policy::StoredPolicy>>(&raw)
                .context("decoding changeset_policies.json")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        *self.changeset_policies.write().await = policies
            .into_iter()
            .map(|p| (format!("{}/{}", p.user_id, p.agent), p))
            .collect();
        let grants = match fs::read(self.root.join("model_grants.json")).await {
            Ok(raw) => serde_json::from_slice::<Vec<ModelGrant>>(&raw)
                .context("decoding model_grants.json")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        *self.model_grants.write().await = grants.into_iter().map(|g| (g.key.clone(), g)).collect();
        let specs = match fs::read(self.root.join("demo_specs.json")).await {
            Ok(raw) => serde_json::from_slice::<Vec<CustomDemoRecord>>(&raw)
                .context("decoding demo_specs.json")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        *self.demo_specs.write().await =
            specs.into_iter().map(|r| (r.spec.id.clone(), r)).collect();
        Ok(())
    }

    pub async fn deploy_agent(&self, req: DeployAgentRequest) -> Result<AgentRecord> {
        validate_name(&req.name)?;
        if req.name.contains("..") {
            bail!("path traversal");
        }
        let bundle = base64::engine::general_purpose::STANDARD
            .decode(req.bundle_base64.as_bytes())
            .context("bundle_base64 is not valid base64")?;
        if bundle.is_empty() {
            bail!("agent bundle is empty");
        }
        if bundle.len() > 16 * 1024 * 1024 {
            bail!("agent bundle exceeds 16 MiB");
        }

        // The deployment version covers both executable bytes and policy. A manifest-only
        // change (for example granting a new credential) must never mutate an existing
        // session's immutable security contract.
        let mut hasher = Sha256::new();
        hasher.update(&bundle);
        hasher.update([0]);
        hasher.update(serde_json::to_vec(&req.manifest)?);
        let digest = hex::encode(hasher.finalize());
        let version = digest[..12].to_string();
        if version.contains("..") {
            bail!("path traversal");
        }
        let record = AgentRecord {
            name: req.name.clone(),
            version: version.clone(),
            digest_sha256: digest,
            manifest: req.manifest,
            created_at: Utc::now(),
        };

        let dir = self
            .root
            .join("agents")
            .join(&req.name)
            .join(&version)
            .to_string_lossy()
            .into_owned();
        let current = self
            .root
            .join("agents")
            .join(&req.name)
            .join("current.json")
            .to_string_lossy()
            .into_owned();
        if dir.contains("..") {
            bail!("path traversal in agent directory");
        } else if current.contains("..") {
            bail!("path traversal in current pointer");
        } else {
            fs::create_dir_all(&dir).await?;
            atomic_write(Path::new(&dir).join("bundle.mjs"), &bundle).await?;
            atomic_write(
                Path::new(&dir).join("record.json"),
                &serde_json::to_vec_pretty(&record)?,
            )
            .await?;
            atomic_write(&current, &serde_json::to_vec_pretty(&record)?).await?;
        }

        self.agents
            .write()
            .await
            .insert(record.name.clone(), record.clone());
        Ok(record)
    }

    pub async fn list_agents(&self) -> Vec<AgentRecord> {
        let mut out: Vec<_> = self.agents.read().await.values().cloned().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub async fn get_agent(&self, name: &str) -> Option<AgentRecord> {
        self.agents.read().await.get(name).cloned()
    }

    /// Load one immutable historical deployment version. Sessions pin this
    /// record so a later deploy cannot change their egress or runtime policy.
    pub async fn get_agent_version(&self, name: &str, version: &str) -> Result<AgentRecord> {
        if name.contains("..") {
            bail!("path traversal");
        }
        if version.contains("..") {
            bail!("path traversal");
        }
        let path = self
            .root
            .join("agents")
            .join(name)
            .join(version)
            .join("record.json")
            .to_string_lossy()
            .into_owned();
        let raw = if path.contains("..") {
            bail!("path traversal");
        } else {
            fs::read(&path)
                .await
                .with_context(|| format!("reading agent record {path}"))?
        };
        serde_json::from_slice(&raw).context("decoding immutable agent record")
    }

    pub async fn agent_bundle(&self, name: &str, version: &str) -> Result<Vec<u8>> {
        if name.contains("..") {
            bail!("path traversal");
        }
        if version.contains("..") {
            bail!("path traversal");
        }
        let path = self
            .root
            .join("agents")
            .join(name)
            .join(version)
            .join("bundle.mjs");
        fs::read(&path)
            .await
            .with_context(|| format!("reading agent bundle {}", path.display()))
    }

    pub async fn save_session(&self, record: SessionRecord) -> Result<()> {
        let id = record.id.to_string();
        if id.contains("..") {
            bail!("path traversal");
        }
        let dir = self
            .root
            .join("sessions")
            .join(&id)
            .to_string_lossy()
            .into_owned();
        if dir.contains("..") {
            bail!("path traversal");
        } else {
            fs::create_dir_all(&dir).await?;
            atomic_write(
                Path::new(&dir).join("session.json"),
                &serde_json::to_vec_pretty(&record)?,
            )
            .await?;
        }
        self.sessions.write().await.insert(record.id, record);
        Ok(())
    }

    pub async fn get_session(&self, id: Uuid) -> Option<SessionRecord> {
        self.sessions.read().await.get(&id).cloned()
    }

    pub async fn list_sessions(&self) -> Vec<SessionRecord> {
        let mut out: Vec<_> = self.sessions.read().await.values().cloned().collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        out
    }

    pub async fn find_session_by_request_id(
        &self,
        agent: &str,
        request_id: &str,
    ) -> Option<SessionRecord> {
        self.sessions
            .read()
            .await
            .values()
            .find(|s| s.agent == agent && s.request_id.as_deref() == Some(request_id))
            .cloned()
    }

    pub async fn count_non_terminal_for_agent(&self, agent: &str) -> usize {
        self.sessions
            .read()
            .await
            .values()
            .filter(|s| s.agent == agent && !s.status.is_terminal())
            .count()
    }

    /// Record that `id` read from an untrusted `host`. True if this is new.
    pub async fn taint_session(&self, id: Uuid, host: &str) -> Result<bool> {
        let already = self
            .get_session(id)
            .await
            .is_none_or(|s| s.tainted_by.iter().any(|h| h == host));
        if already {
            return Ok(false);
        }
        self.update_session(id, |s| {
            if !s.tainted_by.iter().any(|h| h == host) {
                s.tainted_by.push(host.to_string());
            }
        })
        .await?;
        Ok(true)
    }

    /// Clear a session's taint, returning the hosts that had caused it.
    pub async fn untaint_session(&self, id: Uuid) -> Result<Vec<String>> {
        let before = self
            .get_session(id)
            .await
            .map(|s| s.tainted_by)
            .unwrap_or_default();
        self.update_session(id, |s| s.tainted_by.clear()).await?;
        Ok(before)
    }

    /// Non-terminal sessions of one agent that belong to `user_id`.
    pub async fn count_non_terminal_for_user(&self, agent: &str, user_id: &str) -> usize {
        self.sessions
            .read()
            .await
            .values()
            .filter(|s| {
                s.agent == agent && !s.status.is_terminal() && s.user_id.as_deref() == Some(user_id)
            })
            .count()
    }

    pub async fn update_session<F>(&self, id: Uuid, f: F) -> Result<SessionRecord>
    where
        F: FnOnce(&mut SessionRecord),
    {
        let updated = {
            let mut sessions = self.sessions.write().await;
            let record = sessions
                .get_mut(&id)
                .with_context(|| format!("session {id} not found"))?;
            f(record);
            record.updated_at = Utc::now();
            record.clone()
        };
        self.persist_session(&updated).await?;
        Ok(updated)
    }

    /// Atomically move a session from one status to another. Returns `None`
    /// when another concurrent operation already changed the status.
    pub async fn compare_and_set_status(
        &self,
        id: Uuid,
        expected: SessionStatus,
        next: SessionStatus,
    ) -> Result<Option<SessionRecord>> {
        let updated = {
            let mut sessions = self.sessions.write().await;
            let record = sessions
                .get_mut(&id)
                .with_context(|| format!("session {id} not found"))?;
            if record.status != expected {
                return Ok(None);
            }
            record.status = next;
            record.updated_at = Utc::now();
            record.clone()
        };
        self.persist_session(&updated).await?;
        Ok(Some(updated))
    }

    async fn persist_session(&self, record: &SessionRecord) -> Result<()> {
        let path = self
            .root
            .join("sessions")
            .join(record.id.to_string())
            .join("session.json");
        atomic_write(&path, &serde_json::to_vec_pretty(record)?).await
    }

    pub async fn append_event(
        &self,
        id: Uuid,
        kind: impl Into<String>,
        data: serde_json::Value,
    ) -> Result<SessionEvent> {
        let event = {
            let mut sessions = self.sessions.write().await;
            let record = sessions
                .get_mut(&id)
                .with_context(|| format!("session {id} not found"))?;
            record.last_event_seq += 1;
            record.updated_at = Utc::now();
            SessionEvent {
                session_id: id,
                seq: record.last_event_seq,
                kind: kind.into(),
                data,
                timestamp: Utc::now(),
            }
        };

        let id_s = id.to_string();
        if id_s.contains("..") {
            bail!("path traversal");
        }
        let dir = self
            .root
            .join("sessions")
            .join(&id_s)
            .to_string_lossy()
            .into_owned();
        if dir.contains("..") {
            bail!("path traversal");
        } else {
            fs::create_dir_all(&dir).await?;
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(Path::new(&dir).join("events.jsonl"))
                .await?;
            file.write_all(&serde_json::to_vec(&event)?).await?;
            file.write_all(b"\n").await?;
            file.flush().await?;
        }

        if let Some(record) = self.get_session(id).await {
            self.persist_session(&record).await?;
        }
        Ok(event)
    }

    /// Deletes the event log (`events.jsonl`) of sessions that ended before `cutoff`. The session record stays, and reading events
    /// of such a session returns none. Safe to repeat. Returns how many logs were deleted.
    pub async fn purge_ended_session_events(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<usize> {
        let ended: Vec<Uuid> = self
            .sessions
            .read()
            .await
            .values()
            .filter(|s| s.status.is_terminal() && s.updated_at < cutoff)
            .map(|s| s.id)
            .collect();
        let mut purged = 0;
        for id in ended {
            let path = self
                .root
                .join("sessions")
                .join(id.to_string())
                .join("events.jsonl");
            match fs::remove_file(&path).await {
                Ok(()) => purged += 1,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(purged)
    }

    pub async fn events_after(&self, id: Uuid, after: u64) -> Result<Vec<SessionEvent>> {
        let path = self
            .root
            .join("sessions")
            .join(id.to_string())
            .join("events.jsonl");
        let file = match fs::File::open(path).await {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut lines = BufReader::new(file).lines();
        let mut out = Vec::new();
        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let event: SessionEvent = serde_json::from_str(&line)?;
            if event.seq > after {
                out.push(event);
            }
        }
        Ok(out)
    }

    pub async fn list_warm_sandboxes(&self) -> Vec<WarmSandboxRecord> {
        let mut out: Vec<_> = self.warm_sandboxes.read().await.values().cloned().collect();
        out.sort_by_key(|a| a.created_at);
        out
    }

    pub async fn list_warm_sandboxes_for(
        &self,
        agent: &str,
        version: &str,
    ) -> Vec<WarmSandboxRecord> {
        let mut out: Vec<_> = self
            .warm_sandboxes
            .read()
            .await
            .values()
            .filter(|record| record.agent == agent && record.agent_version == version)
            .cloned()
            .collect();
        out.sort_by_key(|a| a.created_at);
        out
    }

    pub async fn save_warm_sandbox(&self, record: WarmSandboxRecord) -> Result<()> {
        let mut warm = self.warm_sandboxes.write().await;
        warm.insert(record.sandbox_id, record);
        self.persist_warm_sandboxes(&warm).await
    }

    /// Atomically reserve the oldest ready sandbox for one session. The
    /// claiming record stays durable until the session record is persisted;
    /// restart reconciliation can therefore distinguish an owned sandbox from
    /// an abandoned claim.
    pub async fn claim_warm_sandbox(
        &self,
        agent: &str,
        version: &str,
        worker_digest_sha256: &str,
        runtime_port: u16,
        session_id: Uuid,
    ) -> Result<Option<WarmSandboxRecord>> {
        let mut warm = self.warm_sandboxes.write().await;
        let candidate = warm
            .values()
            .filter(|record| {
                record.agent == agent
                    && record.agent_version == version
                    && record.worker_digest_sha256 == worker_digest_sha256
                    && record.runtime_port == runtime_port
                    && record.state == WarmSandboxState::Ready
            })
            .min_by_key(|record| record.created_at)
            .map(|record| record.sandbox_id);
        let Some(sandbox_id) = candidate else {
            return Ok(None);
        };
        let record = warm
            .get_mut(&sandbox_id)
            .context("warm sandbox disappeared while claiming")?;
        record.state = WarmSandboxState::Claiming;
        record.claimed_by = Some(session_id);
        record.updated_at = Utc::now();
        let claimed = record.clone();
        self.persist_warm_sandboxes(&warm).await?;
        Ok(Some(claimed))
    }

    /// Atomically move a ready pool item into a transient reconciliation
    /// state. Session claims only select `Ready`, so a health-check can
    /// safely pause/delete the VM without racing a concurrent claim.
    pub async fn begin_warm_reconcile(
        &self,
        sandbox_id: Uuid,
    ) -> Result<Option<WarmSandboxRecord>> {
        let mut warm = self.warm_sandboxes.write().await;
        let Some(record) = warm.get_mut(&sandbox_id) else {
            return Ok(None);
        };
        match record.state {
            WarmSandboxState::Claiming => return Ok(None),
            WarmSandboxState::Ready => {
                record.state = WarmSandboxState::Reconciling;
                record.updated_at = Utc::now();
            }
            WarmSandboxState::Reconciling => {}
        }
        let reserved = record.clone();
        self.persist_warm_sandboxes(&warm).await?;
        Ok(Some(reserved))
    }

    /// Return a health-checked pool item to the claimable state. A persisted
    /// `Reconciling` record is also recovered this way after a process crash.
    pub async fn finish_warm_reconcile(&self, sandbox_id: Uuid) -> Result<()> {
        let mut warm = self.warm_sandboxes.write().await;
        let record = warm
            .get_mut(&sandbox_id)
            .with_context(|| format!("warm sandbox {sandbox_id} not found"))?;
        if record.state == WarmSandboxState::Reconciling {
            record.state = WarmSandboxState::Ready;
            record.updated_at = Utc::now();
            self.persist_warm_sandboxes(&warm).await?;
        }
        Ok(())
    }

    pub async fn forget_warm_sandbox(&self, sandbox_id: Uuid) -> Result<Option<WarmSandboxRecord>> {
        let mut warm = self.warm_sandboxes.write().await;
        let removed = warm.remove(&sandbox_id);
        self.persist_warm_sandboxes(&warm).await?;
        Ok(removed)
    }

    async fn persist_warm_sandboxes(&self, warm: &HashMap<Uuid, WarmSandboxRecord>) -> Result<()> {
        let mut records: Vec<_> = warm.values().cloned().collect();
        records.sort_by_key(|a| a.created_at);
        atomic_write(
            &self.root.join("warm-pools.json"),
            &serde_json::to_vec_pretty(&records)?,
        )
        .await
    }

    pub async fn list_schedules(&self) -> Vec<ScheduleRecord> {
        let mut out: Vec<_> = self.schedules.read().await.values().cloned().collect();
        out.sort_by_key(|record| record.created_at);
        out
    }

    pub async fn save_schedule(&self, record: ScheduleRecord) -> Result<()> {
        let mut map = self.schedules.write().await;
        map.insert(record.id, record);
        self.persist_vec("schedules.json", &map.values().cloned().collect::<Vec<_>>())
            .await
    }

    pub async fn delete_schedule(&self, id: Uuid) -> Result<bool> {
        let mut map = self.schedules.write().await;
        if map.remove(&id).is_none() {
            return Ok(false);
        }
        self.persist_vec("schedules.json", &map.values().cloned().collect::<Vec<_>>())
            .await?;
        Ok(true)
    }

    pub async fn list_workstations(&self) -> Vec<WorkstationRecord> {
        let mut out: Vec<_> = self.workstations.read().await.values().cloned().collect();
        out.sort_by_key(|record| record.created_at);
        out
    }

    pub async fn find_workstation(&self, agent: &str, user_id: &str) -> Option<WorkstationRecord> {
        self.workstations
            .read()
            .await
            .values()
            .find(|w| w.agent == agent && w.user_id == user_id)
            .cloned()
    }

    pub async fn save_workstation(&self, record: WorkstationRecord) -> Result<()> {
        let mut map = self.workstations.write().await;
        map.insert(record.id, record);
        self.persist_vec(
            "workstations.json",
            &map.values().cloned().collect::<Vec<_>>(),
        )
        .await
    }

    pub async fn delete_workstation(&self, id: Uuid) -> Result<bool> {
        let mut map = self.workstations.write().await;
        if map.remove(&id).is_none() {
            return Ok(false);
        }
        self.persist_vec(
            "workstations.json",
            &map.values().cloned().collect::<Vec<_>>(),
        )
        .await?;
        Ok(true)
    }

    pub async fn list_webhooks(&self) -> Vec<WebhookRecord> {
        let mut out: Vec<_> = self.webhooks.read().await.values().cloned().collect();
        out.sort_by_key(|record| record.created_at);
        out
    }

    pub async fn get_webhook(&self, id: Uuid) -> Option<WebhookRecord> {
        self.webhooks.read().await.get(&id).cloned()
    }

    pub async fn save_webhook(&self, record: WebhookRecord) -> Result<()> {
        let mut map = self.webhooks.write().await;
        map.insert(record.id, record);
        self.persist_vec("webhooks.json", &map.values().cloned().collect::<Vec<_>>())
            .await
    }

    pub async fn delete_webhook(&self, id: Uuid) -> Result<bool> {
        let mut map = self.webhooks.write().await;
        if map.remove(&id).is_none() {
            return Ok(false);
        }
        self.persist_vec("webhooks.json", &map.values().cloned().collect::<Vec<_>>())
            .await?;
        Ok(true)
    }

    pub async fn list_loops(&self) -> Vec<LoopRecord> {
        let mut out: Vec<_> = self.loops.read().await.values().cloned().collect();
        out.sort_by_key(|record| record.created_at);
        out
    }

    pub async fn save_loop(&self, record: LoopRecord) -> Result<()> {
        let mut map = self.loops.write().await;
        map.insert(record.id, record);
        self.persist_vec("loops.json", &map.values().cloned().collect::<Vec<_>>())
            .await
    }

    pub async fn delete_loop(&self, id: Uuid) -> Result<bool> {
        let mut map = self.loops.write().await;
        if map.remove(&id).is_none() {
            return Ok(false);
        }
        self.persist_vec("loops.json", &map.values().cloned().collect::<Vec<_>>())
            .await?;
        Ok(true)
    }

    pub async fn list_approvals(&self) -> Vec<ApprovalRecord> {
        let mut out: Vec<_> = self.approvals.read().await.values().cloned().collect();
        out.sort_by_key(|record| record.created_at);
        out
    }

    /// The approvals of one session, oldest first (a read of the map, not a copy of every approval).
    pub async fn approvals_of_session(&self, session_id: Uuid) -> Vec<ApprovalRecord> {
        let mut out: Vec<_> = self
            .approvals
            .read()
            .await
            .values()
            .filter(|a| a.session_id == session_id)
            .cloned()
            .collect();
        out.sort_by_key(|record| record.created_at);
        out
    }

    pub async fn get_approval(&self, id: Uuid) -> Option<ApprovalRecord> {
        self.approvals.read().await.get(&id).cloned()
    }

    pub async fn save_approval(&self, record: ApprovalRecord) -> Result<()> {
        let mut map = self.approvals.write().await;
        map.insert(record.id, record);
        self.persist_vec("approvals.json", &map.values().cloned().collect::<Vec<_>>())
            .await
    }

    pub async fn list_goals(&self) -> Vec<GoalRecord> {
        let mut out: Vec<_> = self.goals.read().await.values().cloned().collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
        out
    }

    pub async fn get_goal(&self, id: Uuid) -> Option<GoalRecord> {
        self.goals.read().await.get(&id).cloned()
    }

    pub async fn save_goal(&self, record: GoalRecord) -> Result<()> {
        let mut map = self.goals.write().await;
        map.insert(record.id, record);
        self.persist_vec("goals.json", &map.values().cloned().collect::<Vec<_>>())
            .await
    }

    pub async fn list_demo_specs(&self) -> Vec<CustomDemoRecord> {
        let mut out: Vec<_> = self.demo_specs.read().await.values().cloned().collect();
        out.sort_by(|a, b| a.spec.id.cmp(&b.spec.id));
        out
    }

    pub async fn get_demo_spec(&self, id: &str) -> Option<CustomDemoRecord> {
        self.demo_specs.read().await.get(id).cloned()
    }

    pub async fn save_demo_spec(&self, record: CustomDemoRecord) -> Result<()> {
        let mut map = self.demo_specs.write().await;
        map.insert(record.spec.id.clone(), record);
        self.persist_vec(
            "demo_specs.json",
            &map.values().cloned().collect::<Vec<_>>(),
        )
        .await
    }

    /// Returns whether a spec with that id existed.
    pub async fn delete_demo_spec(&self, id: &str) -> Result<bool> {
        let mut map = self.demo_specs.write().await;
        let existed = map.remove(id).is_some();
        if existed {
            self.persist_vec(
                "demo_specs.json",
                &map.values().cloned().collect::<Vec<_>>(),
            )
            .await?;
        }
        Ok(existed)
    }

    pub async fn list_devices(&self, user: &str) -> Vec<DeviceRecord> {
        let mut out: Vec<_> = self
            .devices
            .read()
            .await
            .values()
            .filter(|d| d.user_id == user)
            .cloned()
            .collect();
        out.sort_by_key(|d| d.created_at);
        out
    }

    pub async fn get_device(&self, user: &str, device: &str) -> Option<DeviceRecord> {
        self.devices
            .read()
            .await
            .get(&format!("{user}/{device}"))
            .cloned()
    }

    /// Enrol or replace a device. A user may hold at most 10.
    pub async fn save_device(&self, record: DeviceRecord) -> Result<()> {
        let mut map = self.devices.write().await;
        let key = format!("{}/{}", record.user_id, record.device_id);
        if !map.contains_key(&key)
            && map.values().filter(|d| d.user_id == record.user_id).count() >= 10
        {
            anyhow::bail!("a user can have at most 10 enrolled devices");
        }
        map.insert(key, record);
        self.persist_vec("devices.json", &map.values().cloned().collect::<Vec<_>>())
            .await
    }

    pub async fn delete_device(&self, user: &str, device: &str) -> Result<bool> {
        let mut map = self.devices.write().await;
        let existed = map.remove(&format!("{user}/{device}")).is_some();
        if existed {
            self.persist_vec("devices.json", &map.values().cloned().collect::<Vec<_>>())
                .await?;
        }
        Ok(existed)
    }

    pub async fn list_changeset_policies(
        &self,
        user: &str,
    ) -> Vec<crate::changeset_policy::StoredPolicy> {
        let mut out: Vec<_> = self
            .changeset_policies
            .read()
            .await
            .values()
            .filter(|p| p.user_id == user)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.agent.cmp(&b.agent));
        out
    }

    pub async fn get_changeset_policy(
        &self,
        user: &str,
        agent: &str,
    ) -> Option<crate::changeset_policy::StoredPolicy> {
        self.changeset_policies
            .read()
            .await
            .get(&format!("{user}/{agent}"))
            .cloned()
    }

    /// Set or replace a policy; the stored version is the previous one plus one.
    pub async fn save_changeset_policy(
        &self,
        user: &str,
        agent: &str,
        mut policy: crate::changeset_policy::ChangesetPolicy,
    ) -> Result<crate::changeset_policy::StoredPolicy> {
        let mut map = self.changeset_policies.write().await;
        let key = format!("{user}/{agent}");
        policy.version = map
            .get(&key)
            .map_or(1, |p| p.policy.version.saturating_add(1));
        let record = crate::changeset_policy::StoredPolicy {
            user_id: user.to_string(),
            agent: agent.to_string(),
            policy,
            updated_at: Utc::now(),
        };
        map.insert(key, record.clone());
        self.persist_vec(
            "changeset_policies.json",
            &map.values().cloned().collect::<Vec<_>>(),
        )
        .await?;
        Ok(record)
    }

    pub async fn delete_changeset_policy(&self, user: &str, agent: &str) -> Result<bool> {
        let mut map = self.changeset_policies.write().await;
        let existed = map.remove(&format!("{user}/{agent}")).is_some();
        if existed {
            self.persist_vec(
                "changeset_policies.json",
                &map.values().cloned().collect::<Vec<_>>(),
            )
            .await?;
        }
        Ok(existed)
    }

    pub async fn token_floor(&self, user: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        self.token_floors.read().await.get(user).copied()
    }

    /// Refuse every token issued to `user` before `floor`.
    pub async fn set_token_floor(
        &self,
        user: &str,
        floor: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let mut map = self.token_floors.write().await;
        map.insert(user.to_string(), floor);
        atomic_write(
            &self.root.join("user_token_floors.json"),
            &serde_json::to_vec_pretty(&*map)?,
        )
        .await
    }

    pub async fn list_model_grants(&self) -> Vec<ModelGrant> {
        let mut out: Vec<_> = self.model_grants.read().await.values().cloned().collect();
        out.sort_by_key(|g| g.approved_at);
        out
    }

    pub async fn has_model_grant(&self, key: &str) -> bool {
        self.model_grants.read().await.contains_key(key)
    }

    pub async fn save_model_grant(&self, grant: ModelGrant) -> Result<()> {
        let mut map = self.model_grants.write().await;
        map.insert(grant.key.clone(), grant);
        self.persist_vec(
            "model_grants.json",
            &map.values().cloned().collect::<Vec<_>>(),
        )
        .await
    }

    pub async fn revoke_model_grant(&self, key: &str) -> Result<bool> {
        let mut map = self.model_grants.write().await;
        let existed = map.remove(key).is_some();
        if existed {
            self.persist_vec(
                "model_grants.json",
                &map.values().cloned().collect::<Vec<_>>(),
            )
            .await?;
        }
        Ok(existed)
    }

    pub async fn list_triggers(&self) -> Vec<TriggerRecord> {
        let mut out: Vec<_> = self.triggers.read().await.values().cloned().collect();
        out.sort_by_key(|t| t.created_at);
        out
    }

    pub async fn get_trigger(&self, id: Uuid) -> Option<TriggerRecord> {
        self.triggers.read().await.get(&id).cloned()
    }

    pub async fn save_trigger(&self, record: TriggerRecord) -> Result<()> {
        let mut map = self.triggers.write().await;
        map.insert(record.id, record);
        self.persist_vec("triggers.json", &map.values().cloned().collect::<Vec<_>>())
            .await
    }

    /// Save only if the trigger still exists, so a scan that outlived a delete
    /// does not bring the trigger back.
    pub async fn save_trigger_if_present(&self, record: TriggerRecord) -> Result<()> {
        let mut map = self.triggers.write().await;
        if !map.contains_key(&record.id) {
            return Ok(());
        }
        map.insert(record.id, record);
        self.persist_vec("triggers.json", &map.values().cloned().collect::<Vec<_>>())
            .await
    }

    pub async fn delete_trigger(&self, id: Uuid) -> Result<bool> {
        let mut map = self.triggers.write().await;
        let existed = map.remove(&id).is_some();
        if existed {
            self.persist_vec("triggers.json", &map.values().cloned().collect::<Vec<_>>())
                .await?;
        }
        Ok(existed)
    }

    pub async fn list_artifacts(&self) -> Vec<ArtifactRecord> {
        let now = chrono::Utc::now();
        let mut out: Vec<_> = self
            .artifacts
            .read()
            .await
            .values()
            .filter(|a| !artifact_expired(a, now))
            .cloned()
            .collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        out
    }

    pub async fn get_artifact(&self, id: Uuid) -> Option<ArtifactRecord> {
        let now = chrono::Utc::now();
        self.artifacts
            .read()
            .await
            .get(&id)
            .filter(|a| !artifact_expired(a, now))
            .cloned()
    }

    pub async fn save_artifact(&self, record: ArtifactRecord) -> Result<()> {
        let mut map = self.artifacts.write().await;
        // Expired artifacts are already invisible; drop them on the next write.
        let now = chrono::Utc::now();
        map.retain(|_, a| !artifact_expired(a, now));
        map.insert(record.id, record);
        self.persist_vec("artifacts.json", &map.values().cloned().collect::<Vec<_>>())
            .await
    }

    pub async fn approval_for_event(
        &self,
        session_id: Uuid,
        source_seq: u64,
    ) -> Option<ApprovalRecord> {
        self.approvals
            .read()
            .await
            .values()
            .find(|record| record.session_id == session_id && record.source_seq == Some(source_seq))
            .cloned()
    }

    /// Atomically decide a pending approval. Returns `None` if it was not
    /// pending (already decided or expired), so a decision and a timeout can
    /// never both win.
    pub async fn transition_approval(
        &self,
        id: Uuid,
        status: ApprovalStatus,
        comment: Option<String>,
        scope: Option<GrantScope>,
    ) -> Result<Option<ApprovalRecord>> {
        let mut map = self.approvals.write().await;
        let Some(record) = map.get_mut(&id) else {
            return Ok(None);
        };
        if record.status != ApprovalStatus::Pending {
            return Ok(None);
        }
        record.status = status;
        record.comment = comment;
        record.decided_at = Some(Utc::now());
        // What the person was shown lives only while the question is open.
        record.preview = None;
        record.grant_scope = if status == ApprovalStatus::Approved {
            scope
        } else {
            None
        };
        let updated = record.clone();
        self.persist_vec("approvals.json", &map.values().cloned().collect::<Vec<_>>())
            .await?;
        Ok(Some(updated))
    }

    /// Return the pending egress approval for (session, host), or create one
    /// from `new`. The bool is true when a new record was created, so
    /// concurrent requests to the same host share a single question.
    pub async fn open_egress_approval(
        &self,
        session_id: Uuid,
        host: &str,
        new: ApprovalRecord,
    ) -> Result<(ApprovalRecord, bool)> {
        let mut map = self.approvals.write().await;
        if let Some(existing) = map.values().find(|r| {
            r.session_id == session_id
                && r.kind == ApprovalKind::Egress
                && r.status == ApprovalStatus::Pending
                && r.subject.as_deref() == Some(host)
        }) {
            return Ok((existing.clone(), false));
        }
        map.insert(new.id, new.clone());
        self.persist_vec("approvals.json", &map.values().cloned().collect::<Vec<_>>())
            .await?;
        Ok((new, true))
    }

    /// True when an operator approved this host for the rest of the session.
    pub async fn has_session_egress_grant(&self, session_id: Uuid, host: &str) -> bool {
        self.approvals.read().await.values().any(|r| {
            r.session_id == session_id
                && r.kind == ApprovalKind::Egress
                && r.status == ApprovalStatus::Approved
                && r.grant_scope == Some(GrantScope::Session)
                && r.subject.as_deref() == Some(host)
        })
    }

    async fn persist_vec<T: serde::Serialize>(&self, file: &str, records: &[T]) -> Result<()> {
        atomic_write(&self.root.join(file), &serde_json::to_vec_pretty(records)?).await
    }

    pub async fn active_sessions(&self) -> Vec<SessionRecord> {
        self.sessions
            .read()
            .await
            .values()
            .filter(|s| matches!(s.status, SessionStatus::Creating | SessionStatus::Running))
            .cloned()
            .collect()
    }
}

async fn read_id_map<T>(path: &Path) -> Result<HashMap<Uuid, T>>
where
    T: DeserializeOwned + Identified,
{
    let items = match fs::read(path).await {
        Ok(raw) => serde_json::from_slice::<Vec<T>>(&raw)
            .with_context(|| format!("decoding {}", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    Ok(items
        .into_iter()
        .map(|item| (item.identified_id(), item))
        .collect())
}

trait Identified {
    fn identified_id(&self) -> Uuid;
}

impl Identified for ScheduleRecord {
    fn identified_id(&self) -> Uuid {
        self.id
    }
}
impl Identified for WorkstationRecord {
    fn identified_id(&self) -> Uuid {
        self.id
    }
}
impl Identified for WebhookRecord {
    fn identified_id(&self) -> Uuid {
        self.id
    }
}
impl Identified for LoopRecord {
    fn identified_id(&self) -> Uuid {
        self.id
    }
}
impl Identified for ApprovalRecord {
    fn identified_id(&self) -> Uuid {
        self.id
    }
}
impl Identified for GoalRecord {
    fn identified_id(&self) -> Uuid {
        self.id
    }
}
impl Identified for TriggerRecord {
    fn identified_id(&self) -> Uuid {
        self.id
    }
}
impl Identified for ArtifactRecord {
    fn identified_id(&self) -> Uuid {
        self.id
    }
}

fn artifact_expired(a: &ArtifactRecord, now: chrono::DateTime<chrono::Utc>) -> bool {
    a.expires_at.is_some_and(|t| t <= now)
}

pub(crate) async fn atomic_write(path: impl AsRef<Path>, bytes: &[u8]) -> Result<()> {
    let path_s = path.as_ref().to_string_lossy().into_owned();
    if path_s.contains("..") {
        bail!("refusing path traversal");
    } else {
        let path = PathBuf::from(&path_s);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let tmp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
        fs::write(&tmp, bytes).await?;
        fs::rename(&tmp, &path).await?;
        Ok(())
    }
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 80 {
        bail!("name must be 1..=80 characters");
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        bail!("name may contain only ASCII letters, digits, '.', '_' and '-'");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        AgentManifest, DeployAgentRequest, SessionRecord, SessionStartMode, WarmSandboxRecord,
        WarmSandboxState,
    };
    use serde_json::json;

    fn test_root() -> PathBuf {
        std::env::temp_dir().join(format!("zyvor-agent-test-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn deploy_and_reload_agent() {
        let root = test_root();
        let store = Store::open(&root).await.unwrap();
        let request = DeployAgentRequest {
            name: "research".into(),
            bundle_base64: base64::engine::general_purpose::STANDARD
                .encode("export default () => 1"),
            manifest: AgentManifest {
                egress_rules: vec![],
                dlp: false,
                taint: None,
                sequence: None,
                confinement: Default::default(),
                resources: None,
                confidential: Default::default(),
                inner_container: Default::default(),
                persistent: false,
                browser_port: None,
                browser: None,
                template: "node22".into(),
                credentials: vec!["openai".into()],
                egress_allow_hosts: vec!["api.openai.com".into()],
                allow_private_networks: false,
                runtime_port: 8080,
                memory: false,
                suggestions: false,
                card: false,
                ttl_seconds: None,
                max_concurrent_sessions: None,
                idle_hibernate_seconds: None,
                warm_pool_size: 0,
                runtime: Default::default(),
                egress_mode: Default::default(),
                home_volume: None,
                skills: vec![],
                skill_scope: None,
                egress_approval_timeout_seconds: None,
                model_socket: None,
                cell_backend: None,
                gpus: None,
                verify_command: None,
                snapshot_before_apply: false,
            },
        };
        let record = store.deploy_agent(request).await.unwrap();
        assert_eq!(record.name, "research");
        drop(store);

        let reloaded = Store::open(&root).await.unwrap();
        assert_eq!(
            reloaded.get_agent("research").await.unwrap().version,
            record.version
        );
        let _ = fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn manifest_change_creates_new_immutable_version() {
        let root = test_root();
        let store = Store::open(&root).await.unwrap();
        let bundle = base64::engine::general_purpose::STANDARD.encode("export default () => 1");
        let one = store
            .deploy_agent(DeployAgentRequest {
                name: "research".into(),
                bundle_base64: bundle.clone(),
                manifest: AgentManifest {
                    egress_rules: vec![],
                    dlp: false,
                    taint: None,
                    sequence: None,
                    confinement: Default::default(),
                    resources: None,
                    confidential: Default::default(),
                    inner_container: Default::default(),
                    persistent: false,
                    browser_port: None,
                    browser: None,
                    template: "node22".into(),
                    credentials: vec![],
                    egress_allow_hosts: vec!["api.openai.com".into()],
                    allow_private_networks: false,
                    runtime_port: 8080,
                    memory: false,
                    suggestions: false,
                    card: false,
                    ttl_seconds: None,
                    max_concurrent_sessions: None,
                    idle_hibernate_seconds: None,
                    warm_pool_size: 0,
                    runtime: Default::default(),
                    egress_mode: Default::default(),
                    home_volume: None,
                    skills: vec![],
                    skill_scope: None,
                    egress_approval_timeout_seconds: None,
                    model_socket: None,
                    cell_backend: None,
                    gpus: None,
                    verify_command: None,
                    snapshot_before_apply: false,
                },
            })
            .await
            .unwrap();
        let two = store
            .deploy_agent(DeployAgentRequest {
                name: "research".into(),
                bundle_base64: bundle,
                manifest: AgentManifest {
                    egress_rules: vec![],
                    dlp: false,
                    taint: None,
                    sequence: None,
                    confinement: Default::default(),
                    resources: None,
                    confidential: Default::default(),
                    inner_container: Default::default(),
                    persistent: false,
                    browser_port: None,
                    browser: None,
                    template: "node22".into(),
                    credentials: vec![],
                    egress_allow_hosts: vec!["api.anthropic.com".into()],
                    allow_private_networks: false,
                    runtime_port: 8080,
                    memory: false,
                    suggestions: false,
                    card: false,
                    ttl_seconds: None,
                    max_concurrent_sessions: None,
                    idle_hibernate_seconds: None,
                    warm_pool_size: 0,
                    runtime: Default::default(),
                    egress_mode: Default::default(),
                    home_volume: None,
                    skills: vec![],
                    skill_scope: None,
                    egress_approval_timeout_seconds: None,
                    model_socket: None,
                    cell_backend: None,
                    gpus: None,
                    verify_command: None,
                    snapshot_before_apply: false,
                },
            })
            .await
            .unwrap();
        assert_ne!(one.version, two.version);
        assert_eq!(
            store
                .get_agent_version("research", &one.version)
                .await
                .unwrap()
                .manifest
                .egress_allow_hosts[0],
            "api.openai.com"
        );
        let _ = fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn events_are_monotonic_and_durable() {
        let root = test_root();
        let store = Store::open(&root).await.unwrap();
        let id = Uuid::new_v4();
        let now = Utc::now();
        store
            .save_session(SessionRecord {
                id,
                agent: "a".into(),
                agent_version: "v".into(),
                sandbox_id: Uuid::new_v4(),
                status: SessionStatus::Running,
                input: json!({}),
                created_at: now,
                updated_at: now,
                last_event_seq: 0,
                guest_event_cursor: 0,
                request_id: Some("job:1".into()),
                start_policy: crate::model::SessionStartPolicy::PreferWarm,
                start_mode: SessionStartMode::Cold,
                startup_ms: None,
                expires_at: None,
                sandbox_released: false,
                capability_token: "cap".into(),
                error: None,
                parent_session_id: None,
                user_id: None,
                tainted_by: vec![],
                writes: 0,
                confidential: None,
                agent_paused_reason: None,
                browse: Default::default(),
            })
            .await
            .unwrap();
        assert_eq!(
            store
                .find_session_by_request_id("a", "job:1")
                .await
                .unwrap()
                .id,
            id
        );
        assert_eq!(store.count_non_terminal_for_agent("a").await, 1);
        assert_eq!(store.count_non_terminal_for_user("a", "alice").await, 0);
        assert_eq!(
            store.append_event(id, "one", json!(1)).await.unwrap().seq,
            1
        );
        assert_eq!(
            store.append_event(id, "two", json!(2)).await.unwrap().seq,
            2
        );
        let events = store.events_after(id, 1).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "two");
        let _ = fs::remove_dir_all(root).await;
    }
    #[tokio::test]
    async fn reconciling_warm_sandbox_cannot_be_claimed() {
        let root = test_root();
        let store = Store::open(&root).await.unwrap();
        let sandbox_id = Uuid::new_v4();
        let now = Utc::now();
        store
            .save_warm_sandbox(WarmSandboxRecord {
                sandbox_id,
                agent: "research".into(),
                agent_version: "abc123".into(),
                runtime_port: 8080,
                worker_digest_sha256: "worker-digest".into(),
                state: WarmSandboxState::Ready,
                claimed_by: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();

        let reserved = store
            .begin_warm_reconcile(sandbox_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reserved.state, WarmSandboxState::Reconciling);
        assert!(store
            .claim_warm_sandbox("research", "abc123", "worker-digest", 8080, Uuid::new_v4(),)
            .await
            .unwrap()
            .is_none());

        store.finish_warm_reconcile(sandbox_id).await.unwrap();
        assert!(store
            .claim_warm_sandbox("research", "abc123", "worker-digest", 8080, Uuid::new_v4(),)
            .await
            .unwrap()
            .is_some());
        let _ = fs::remove_dir_all(root).await;
    }

    #[tokio::test]
    async fn warm_sandbox_claim_is_durable_and_single_use() {
        let root = test_root();
        let store = Store::open(&root).await.unwrap();
        let sandbox_id = Uuid::new_v4();
        let now = Utc::now();
        store
            .save_warm_sandbox(WarmSandboxRecord {
                sandbox_id,
                agent: "research".into(),
                agent_version: "abc123".into(),
                runtime_port: 8080,
                worker_digest_sha256: "worker-digest".into(),
                state: WarmSandboxState::Ready,
                claimed_by: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();

        let session_id = Uuid::new_v4();
        let claimed = store
            .claim_warm_sandbox("research", "abc123", "worker-digest", 8080, session_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.sandbox_id, sandbox_id);
        assert_eq!(claimed.state, WarmSandboxState::Claiming);
        assert_eq!(claimed.claimed_by, Some(session_id));
        assert!(store
            .claim_warm_sandbox("research", "abc123", "worker-digest", 8080, Uuid::new_v4(),)
            .await
            .unwrap()
            .is_none());
        drop(store);

        let reloaded = Store::open(&root).await.unwrap();
        let records = reloaded.list_warm_sandboxes_for("research", "abc123").await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].state, WarmSandboxState::Claiming);
        assert_eq!(records[0].claimed_by, Some(session_id));
        let _ = fs::remove_dir_all(root).await;
    }
}
