// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! One Raft voter per fabricd process.
//!
//! Peers come from `FLUXVM_AI_RAFT_PEERS`. This process never starts the
//! other voters. A unit test may run three in-memory voters to prove an
//! election; that network is not used at startup.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::Cursor;
use std::ops::RangeBounds;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use axum::extract::Json;
use axum::http::{HeaderMap, StatusCode};
use openraft::error::{
    InstallSnapshotError, NetworkError, RPCError, RaftError, RemoteError, Unreachable,
};
use openraft::network::{RPCOption, RaftNetwork, RaftNetworkFactory};
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use openraft::storage::{LogFlushed, RaftLogStorage, RaftStateMachine};
use openraft::{
    AnyError, BasicNode, Config, Entry, EntryPayload, ErrorSubject, ErrorVerb, LogId, LogState,
    OptionalSend, RaftLogId, RaftLogReader, RaftSnapshotBuilder, ServerState, Snapshot,
    SnapshotMeta, StorageError, StorageIOError, StoredMembership, Vote,
};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, RwLock};

pub type NodeId = u64;

openraft::declare_raft_types!(
    pub TypeConfig:
        D = LeaseCommand,
        R = LeaseResponse,
);

pub type RaftNode = openraft::Raft<TypeConfig>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op")]
pub enum LeaseCommand {
    Leader {
        leader: NodeId,
    },
    Admit {
        id: String,
        now: i64,
        tokens: u64,
        rpm: u64,
        tpm: u64,
        streams: u32,
    },
    Release {
        id: String,
    },
    AdmitDay {
        id: String,
        now: i64,
        tokens: u64,
        daily: u64,
    },
    /// Append one audit chain tip on the leader (replicated).
    AuditLink {
        line: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op")]
pub enum LeaseResponse {
    Leader { leader: NodeId },
    Admitted { stream_id: Option<String> },
    Rejected { message: String },
    Released,
    AuditLinked { tip: String },
}

#[derive(Debug, Clone)]
struct PeerSpec {
    id: NodeId,
    peers: BTreeMap<NodeId, BasicNode>,
}

fn peer_spec() -> Option<PeerSpec> {
    let id = std::env::var("FLUXVM_AI_RAFT_ID").ok()?.parse().ok()?;
    let raw = std::env::var("FLUXVM_AI_RAFT_PEERS").ok()?;
    let token = std::env::var("FLUXVM_AI_RAFT_TOKEN").ok()?;
    if token.is_empty() {
        return None;
    }
    let mut peers = BTreeMap::new();
    for item in raw.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let (id_text, addr) = item.split_once('@')?;
        let Ok(peer_id) = id_text.parse::<NodeId>() else {
            return None;
        };
        if addr.is_empty() {
            return None;
        }
        peers.insert(
            peer_id,
            BasicNode {
                addr: addr.to_string(),
            },
        );
    }
    if peers.len() < 3 || !peers.contains_key(&id) {
        return None;
    }
    Some(PeerSpec { id, peers })
}

static MEMBER: OnceLock<Arc<RaftNode>> = OnceLock::new();
static START_GATE: OnceLock<Mutex<()>> = OnceLock::new();
static RECORDED_LEADER: AtomicU64 = AtomicU64::new(u64::MAX);
static AUDIT_TIP: OnceLock<std::sync::Mutex<String>> = OnceLock::new();

pub fn may_place() -> bool {
    if peer_spec().is_none() {
        return true;
    }
    is_leader()
}

pub fn is_leader() -> bool {
    let Some(raft) = MEMBER.get() else {
        return false;
    };
    let metrics = raft.metrics().borrow().clone();
    metrics.state == ServerState::Leader && metrics.current_leader == Some(metrics.id)
}

pub fn peers_configured() -> bool {
    peer_spec().is_some()
}

/// Where a rate counter is stored. Unset peers stay on the local file.
/// A follower forwards to the elected leader. No elected leader fails closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CounterHome {
    Local,
    Leader(String),
    Unavailable,
}

pub fn counter_home() -> CounterHome {
    let Some(spec) = peer_spec() else {
        return CounterHome::Local;
    };
    if is_leader() {
        return CounterHome::Local;
    }
    let Some(raft) = MEMBER.get() else {
        return CounterHome::Unavailable;
    };
    let metrics = raft.metrics().borrow().clone();
    let Some(leader) = metrics.current_leader else {
        return CounterHome::Unavailable;
    };
    spec.peers
        .get(&leader)
        .map(|node| CounterHome::Leader(node.addr.clone()))
        .unwrap_or(CounterHome::Unavailable)
}

pub async fn write_audit_link(line: String) -> Result<String, String> {
    let response = write_counter(LeaseCommand::AuditLink { line }).await?;
    match response {
        LeaseResponse::AuditLinked { tip } => Ok(tip),
        other => Err(format!("unexpected audit reply: {other:?}")),
    }
}

pub fn replicated_audit_tip() -> Option<String> {
    AUDIT_TIP.get().and_then(|tip| {
        tip.lock()
            .ok()
            .map(|guard| tip_from_guard(&guard))
            .filter(|value| !value.is_empty())
    })
}

fn tip_from_guard(guard: &std::sync::MutexGuard<'_, String>) -> String {
    (**guard).clone()
}

pub async fn write_counter(mut command: LeaseCommand) -> Result<LeaseResponse, String> {
    let Some(raft) = MEMBER.get() else {
        return Err("AI raft member is not running".into());
    };
    if !is_leader() {
        return Err("this process is not the AI raft leader".into());
    }
    match &mut command {
        LeaseCommand::Admit { now, .. } | LeaseCommand::AdmitDay { now, .. } => {
            *now = chrono::Utc::now().timestamp();
        }
        LeaseCommand::Leader { .. } => {
            return Err("leader records are not written through the limit path".into());
        }
        LeaseCommand::Release { .. } | LeaseCommand::AuditLink { .. } => {}
    }
    raft.client_write(command)
        .await
        .map(|written| written.data)
        .map_err(|err| err.to_string())
}

pub fn quorum_committed() -> bool {
    let Some(raft) = MEMBER.get() else {
        return false;
    };
    let metrics = raft.metrics().borrow().clone();
    metrics.current_leader.is_some()
        && metrics.membership_config.membership().voter_ids().count() >= 3
}

pub async fn record_leader() -> Result<(), String> {
    let Some(raft) = MEMBER.get() else {
        return Ok(());
    };
    if !is_leader() {
        RECORDED_LEADER.store(u64::MAX, Ordering::Relaxed);
        return Ok(());
    }
    let id = raft.metrics().borrow().id;
    if RECORDED_LEADER.load(Ordering::Relaxed) == id {
        return Ok(());
    }
    raft.client_write(LeaseCommand::Leader { leader: id })
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())?;
    RECORDED_LEADER.store(id, Ordering::Relaxed);
    Ok(())
}

pub async fn start(state_dir: &Path) -> Result<(), String> {
    if peer_spec().is_none() {
        return Ok(());
    }
    let gate = START_GATE.get_or_init(|| Mutex::new(()));
    let _guard = gate.lock().await;
    if MEMBER.get().is_some() {
        return Ok(());
    }
    let Some(spec) = peer_spec() else {
        return Ok(());
    };
    let dir = state_dir.join("ai-raft");
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let log_store = LogStore::open(dir.join("log.json"))?;
    let state_machine = StateMachineStore::open(dir.join("state-machine.json"))?;
    let config = Config {
        heartbeat_interval: 500,
        election_timeout_min: 1500,
        election_timeout_max: 3000,
        ..Default::default()
    };
    let config = Arc::new(config.validate().map_err(|err| err.to_string())?);
    let raft = RaftNode::new(spec.id, config, HttpNetwork, log_store, state_machine)
        .await
        .map_err(|err| err.to_string())?;
    let raft = Arc::new(raft);
    if spec.id == *spec.peers.keys().next().expect("peers are non-empty") {
        let initialized = raft.is_initialized().await.map_err(|err| err.to_string())?;
        if !initialized {
            raft.initialize(spec.peers)
                .await
                .map_err(|err| err.to_string())?;
        }
    }
    let _ = MEMBER.set(raft);
    Ok(())
}

fn token_ok(headers: &HeaderMap) -> bool {
    let Ok(expected) = std::env::var("FLUXVM_AI_RAFT_TOKEN") else {
        return false;
    };
    let Some(got) = headers
        .get("x-raft-token")
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    expected.len() == got.len() && bool::from(expected.as_bytes().ct_eq(got.as_bytes()))
}

fn member(headers: &HeaderMap) -> Result<Arc<RaftNode>, StatusCode> {
    if !token_ok(headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    MEMBER.get().cloned().ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

pub async fn vote(
    headers: HeaderMap,
    Json(req): Json<VoteRequest<NodeId>>,
) -> Result<Json<Result<VoteResponse<NodeId>, RaftError<NodeId>>>, StatusCode> {
    let raft = member(&headers)?;
    Ok(Json(raft.vote(req).await))
}

pub async fn append(
    headers: HeaderMap,
    Json(req): Json<AppendEntriesRequest<TypeConfig>>,
) -> Result<Json<Result<AppendEntriesResponse<NodeId>, RaftError<NodeId>>>, StatusCode> {
    let raft = member(&headers)?;
    Ok(Json(raft.append_entries(req).await))
}

pub async fn snapshot(
    headers: HeaderMap,
    Json(req): Json<InstallSnapshotRequest<TypeConfig>>,
) -> Result<
    Json<Result<InstallSnapshotResponse<NodeId>, RaftError<NodeId, InstallSnapshotError>>>,
    StatusCode,
> {
    let raft = member(&headers)?;
    Ok(Json(raft.install_snapshot(req).await))
}

/// POST /api/ai/raft/limits
///
/// The leader appends one counter command. A follower returns 503 so the
/// caller can retry the elected leader instead of keeping a second count.
pub async fn limit_write(
    headers: HeaderMap,
    Json(command): Json<LeaseCommand>,
) -> Result<Json<LeaseResponse>, (StatusCode, String)> {
    if !token_ok(&headers) {
        return Err((StatusCode::UNAUTHORIZED, "raft token is required".into()));
    }
    if matches!(command, LeaseCommand::Leader { .. }) {
        return Err((
            StatusCode::FORBIDDEN,
            "leader records are not written through the limit path".into(),
        ));
    }
    write_counter(command)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::SERVICE_UNAVAILABLE, err))
}

#[derive(Clone, Debug)]
struct HttpNetwork;

impl HttpNetwork {
    // `RPCError` is openraft's own error type; it is returned as the network trait requires.
    #[allow(clippy::result_large_err)]
    async fn send<Req, Resp, Err>(
        &self,
        target: NodeId,
        node: &BasicNode,
        path: &str,
        req: Req,
    ) -> Result<Resp, RPCError<NodeId, BasicNode, Err>>
    where
        Req: Serialize,
        Resp: for<'de> Deserialize<'de>,
        Err: std::error::Error + for<'de> Deserialize<'de>,
    {
        let token = std::env::var("FLUXVM_AI_RAFT_TOKEN").unwrap_or_default();
        let url = format!("http://{}/api/ai/raft/{path}", node.addr);
        let client = reqwest::Client::new();
        let resp = client
            .post(url)
            .header("x-raft-token", token)
            .json(&req)
            .send()
            .await
            .map_err(|err| {
                if err.is_connect() {
                    RPCError::Unreachable(Unreachable::new(&err))
                } else {
                    RPCError::Network(NetworkError::new(&err))
                }
            })?;
        let body: Result<Resp, Err> = resp
            .json()
            .await
            .map_err(|err| RPCError::Network(NetworkError::new(&err)))?;
        body.map_err(|err| RPCError::RemoteError(RemoteError::new(target, err)))
    }
}

impl RaftNetworkFactory<TypeConfig> for HttpNetwork {
    type Network = HttpConn;

    async fn new_client(&mut self, target: NodeId, node: &BasicNode) -> Self::Network {
        HttpConn {
            owner: HttpNetwork,
            target,
            target_node: node.clone(),
        }
    }
}

struct HttpConn {
    owner: HttpNetwork,
    target: NodeId,
    target_node: BasicNode,
}

impl RaftNetwork<TypeConfig> for HttpConn {
    async fn append_entries(
        &mut self,
        req: AppendEntriesRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<NodeId>, RPCError<NodeId, BasicNode, RaftError<NodeId>>> {
        self.owner
            .send(self.target, &self.target_node, "append", req)
            .await
    }

    async fn install_snapshot(
        &mut self,
        req: InstallSnapshotRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<
        InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, BasicNode, RaftError<NodeId, InstallSnapshotError>>,
    > {
        self.owner
            .send(self.target, &self.target_node, "snapshot", req)
            .await
    }

    async fn vote(
        &mut self,
        req: VoteRequest<NodeId>,
        _option: RPCOption,
    ) -> Result<VoteResponse<NodeId>, RPCError<NodeId, BasicNode, RaftError<NodeId>>> {
        self.owner
            .send(self.target, &self.target_node, "vote", req)
            .await
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct DiskLog {
    last_purged_log_id: Option<LogId<NodeId>>,
    log: BTreeMap<u64, Entry<TypeConfig>>,
    committed: Option<LogId<NodeId>>,
    vote: Option<Vote<NodeId>>,
}

#[derive(Clone, Debug)]
struct LogStore {
    inner: Arc<Mutex<DiskLog>>,
    path: Option<PathBuf>,
}

impl LogStore {
    fn open(path: PathBuf) -> Result<Self, String> {
        let disk = if path.exists() {
            let bytes = std::fs::read(&path).map_err(|err| err.to_string())?;
            serde_json::from_slice(&bytes).map_err(|err| err.to_string())?
        } else {
            DiskLog::default()
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(disk)),
            path: Some(path),
        })
    }

    #[cfg(test)]
    fn memory() -> Self {
        Self {
            inner: Arc::new(Mutex::new(DiskLog::default())),
            path: None,
        }
    }

    // `StorageError` is openraft's own error type; boxing it would change the trait the store implements.
    #[allow(clippy::result_large_err)]
    fn flush(path: &Option<PathBuf>, disk: &DiskLog) -> Result<(), StorageError<NodeId>> {
        let Some(path) = path else {
            return Ok(());
        };
        let bytes = serde_json::to_vec(disk).map_err(|err| StorageError::IO {
            source: StorageIOError::new(ErrorSubject::Logs, ErrorVerb::Write, AnyError::error(err)),
        })?;
        std::fs::write(path, bytes).map_err(|err| StorageError::IO {
            source: StorageIOError::new(ErrorSubject::Logs, ErrorVerb::Write, AnyError::error(err)),
        })
    }
}

impl RaftLogReader<TypeConfig> for LogStore {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<TypeConfig>>, StorageError<NodeId>> {
        let inner = self.inner.lock().await;
        Ok(inner
            .log
            .range(range)
            .map(|(_, entry)| entry.clone())
            .collect())
    }
}

impl RaftLogStorage<TypeConfig> for LogStore {
    type LogReader = Self;

    async fn get_log_state(&mut self) -> Result<LogState<TypeConfig>, StorageError<NodeId>> {
        let inner = self.inner.lock().await;
        let last = inner
            .log
            .iter()
            .next_back()
            .map(|(_, entry)| *entry.get_log_id());
        let last_purged = inner.last_purged_log_id;
        Ok(LogState {
            last_purged_log_id: last_purged,
            last_log_id: last.or(last_purged),
        })
    }

    async fn save_committed(
        &mut self,
        committed: Option<LogId<NodeId>>,
    ) -> Result<(), StorageError<NodeId>> {
        let mut inner = self.inner.lock().await;
        inner.committed = committed;
        Self::flush(&self.path, &inner)
    }

    async fn read_committed(&mut self) -> Result<Option<LogId<NodeId>>, StorageError<NodeId>> {
        Ok(self.inner.lock().await.committed)
    }

    async fn save_vote(&mut self, vote: &Vote<NodeId>) -> Result<(), StorageError<NodeId>> {
        let mut inner = self.inner.lock().await;
        inner.vote = Some(*vote);
        Self::flush(&self.path, &inner)
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<NodeId>>, StorageError<NodeId>> {
        Ok(self.inner.lock().await.vote)
    }

    async fn append<I>(
        &mut self,
        entries: I,
        callback: LogFlushed<TypeConfig>,
    ) -> Result<(), StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + OptionalSend,
        I::IntoIter: OptionalSend,
    {
        let mut inner = self.inner.lock().await;
        for entry in entries {
            inner.log.insert(entry.get_log_id().index, entry);
        }
        if let Err(err) = Self::flush(&self.path, &inner) {
            callback.log_io_completed(Err(std::io::Error::other(err.to_string())));
            return Err(err);
        }
        callback.log_io_completed(Ok(()));
        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        let mut inner = self.inner.lock().await;
        let keys: Vec<u64> = inner
            .log
            .range(log_id.index..)
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            inner.log.remove(&key);
        }
        Self::flush(&self.path, &inner)
    }

    async fn purge(&mut self, log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        let mut inner = self.inner.lock().await;
        inner.last_purged_log_id = Some(log_id);
        let keys: Vec<u64> = inner
            .log
            .range(..=log_id.index)
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            inner.log.remove(&key);
        }
        Self::flush(&self.path, &inner)
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }
}

fn empty_counter(id: String) -> super::limits::RateCounter {
    super::limits::RateCounter {
        id,
        window_started_unix: 0,
        requests: 0,
        tokens: 0,
        day_started_unix: 0,
        day_tokens: 0,
        inflight: 0,
    }
}

fn apply_command(data: &mut MachineData, command: LeaseCommand) -> LeaseResponse {
    match command {
        LeaseCommand::Leader { leader } => {
            data.leader = Some(leader);
            LeaseResponse::Leader { leader }
        }
        LeaseCommand::Admit {
            id,
            now,
            tokens,
            rpm,
            tpm,
            streams,
        } => {
            let current = data
                .counters
                .get(&id)
                .cloned()
                .unwrap_or_else(|| empty_counter(id.clone()));
            match super::limits::admit(current, now, tokens, rpm, tpm, streams) {
                Ok(next) => {
                    let stream_id = if streams > 0 { Some(id.clone()) } else { None };
                    data.counters.insert(id, next);
                    LeaseResponse::Admitted { stream_id }
                }
                Err(message) => LeaseResponse::Rejected { message },
            }
        }
        LeaseCommand::Release { id } => {
            if let Some(counter) = data.counters.get(&id).cloned() {
                data.counters
                    .insert(id, super::limits::release_inflight(counter));
            }
            LeaseResponse::Released
        }
        LeaseCommand::AdmitDay {
            id,
            now,
            tokens,
            daily,
        } => {
            let current = data
                .counters
                .get(&id)
                .cloned()
                .unwrap_or_else(|| empty_counter(id.clone()));
            match super::limits::admit_day(current, now, tokens, daily) {
                Ok(next) => {
                    data.counters.insert(id, next);
                    LeaseResponse::Admitted { stream_id: None }
                }
                Err(message) => LeaseResponse::Rejected { message },
            }
        }
        LeaseCommand::AuditLink { line } => {
            let tip = super::chain_hash(&data.audit_tip, &line);
            data.audit_tip = tip.clone();
            let cell = AUDIT_TIP.get_or_init(|| std::sync::Mutex::new(String::new()));
            if let Ok(mut guard) = cell.lock() {
                *guard = tip.clone();
            }
            LeaseResponse::AuditLinked { tip }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MachineData {
    last_applied_log: Option<LogId<NodeId>>,
    last_membership: StoredMembership<NodeId, BasicNode>,
    leader: Option<NodeId>,
    #[serde(default)]
    counters: BTreeMap<String, super::limits::RateCounter>,
    #[serde(default)]
    audit_tip: String,
}

/// The snapshot the state machine last built: its metadata and bytes.
type CurrentSnapshot = Option<(SnapshotMeta<NodeId, BasicNode>, Vec<u8>)>;

#[derive(Debug, Default)]
struct StateMachineStore {
    state_machine: RwLock<MachineData>,
    snapshot_idx: AtomicU64,
    current_snapshot: RwLock<CurrentSnapshot>,
    path: Option<PathBuf>,
}

impl StateMachineStore {
    fn open(path: PathBuf) -> Result<Arc<Self>, String> {
        let data = if path.exists() {
            let bytes = std::fs::read(&path).map_err(|err| err.to_string())?;
            serde_json::from_slice(&bytes).map_err(|err| err.to_string())?
        } else {
            MachineData::default()
        };
        Ok(Arc::new(Self {
            state_machine: RwLock::new(data),
            path: Some(path),
            ..Self::default()
        }))
    }

    #[cfg(test)]
    fn memory() -> Arc<Self> {
        Arc::new(Self::default())
    }

    // `StorageError` is openraft's own error type; boxing it would change the trait the store implements.
    #[allow(clippy::result_large_err)]
    fn flush(&self, data: &MachineData) -> Result<(), StorageError<NodeId>> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let bytes = serde_json::to_vec(data).map_err(|err| StorageError::IO {
            source: StorageIOError::new(
                ErrorSubject::StateMachine,
                ErrorVerb::Write,
                AnyError::error(err),
            ),
        })?;
        std::fs::write(path, bytes).map_err(|err| StorageError::IO {
            source: StorageIOError::new(
                ErrorSubject::StateMachine,
                ErrorVerb::Write,
                AnyError::error(err),
            ),
        })
    }
}

impl RaftSnapshotBuilder<TypeConfig> for Arc<StateMachineStore> {
    async fn build_snapshot(&mut self) -> Result<Snapshot<TypeConfig>, StorageError<NodeId>> {
        let state_machine = self.state_machine.read().await;
        let data = serde_json::to_vec(&*state_machine).map_err(|err| StorageError::IO {
            source: StorageIOError::new(
                ErrorSubject::StateMachine,
                ErrorVerb::Read,
                AnyError::error(err),
            ),
        })?;
        let last_applied_log = state_machine.last_applied_log;
        let last_membership = state_machine.last_membership.clone();
        drop(state_machine);
        let snapshot_idx = self.snapshot_idx.fetch_add(1, Ordering::Relaxed) + 1;
        let snapshot_id = match last_applied_log {
            Some(last) => format!("{}-{}-{snapshot_idx}", last.leader_id, last.index),
            None => format!("--{snapshot_idx}"),
        };
        let meta = SnapshotMeta {
            last_log_id: last_applied_log,
            last_membership,
            snapshot_id,
        };
        *self.current_snapshot.write().await = Some((meta.clone(), data.clone()));
        Ok(Snapshot {
            meta,
            snapshot: Box::new(Cursor::new(data)),
        })
    }
}

impl RaftStateMachine<TypeConfig> for Arc<StateMachineStore> {
    type SnapshotBuilder = Self;

    async fn applied_state(
        &mut self,
    ) -> Result<(Option<LogId<NodeId>>, StoredMembership<NodeId, BasicNode>), StorageError<NodeId>>
    {
        let state_machine = self.state_machine.read().await;
        Ok((
            state_machine.last_applied_log,
            state_machine.last_membership.clone(),
        ))
    }

    async fn apply<I>(&mut self, entries: I) -> Result<Vec<LeaseResponse>, StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + OptionalSend,
        I::IntoIter: OptionalSend,
    {
        let mut responses = Vec::new();
        let mut state_machine = self.state_machine.write().await;
        for entry in entries {
            state_machine.last_applied_log = Some(entry.log_id);
            let response = match entry.payload {
                EntryPayload::Blank => LeaseResponse::Leader {
                    leader: state_machine.leader.unwrap_or(0),
                },
                EntryPayload::Normal(command) => apply_command(&mut state_machine, command),
                EntryPayload::Membership(membership) => {
                    state_machine.last_membership =
                        StoredMembership::new(Some(entry.log_id), membership);
                    LeaseResponse::Leader {
                        leader: state_machine.leader.unwrap_or(0),
                    }
                }
            };
            responses.push(response);
        }
        self.flush(&state_machine)?;
        Ok(responses)
    }

    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<Cursor<Vec<u8>>>, StorageError<NodeId>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<NodeId, BasicNode>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<NodeId>> {
        let bytes = snapshot.into_inner();
        let mut data: MachineData =
            serde_json::from_slice(&bytes).map_err(|err| StorageError::IO {
                source: StorageIOError::new(
                    ErrorSubject::Snapshot(None),
                    ErrorVerb::Read,
                    AnyError::error(err),
                ),
            })?;
        data.last_applied_log = meta.last_log_id;
        data.last_membership = meta.last_membership.clone();
        self.flush(&data)?;
        *self.state_machine.write().await = data;
        *self.current_snapshot.write().await = Some((meta.clone(), bytes));
        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<TypeConfig>>, StorageError<NodeId>> {
        Ok(self
            .current_snapshot
            .read()
            .await
            .as_ref()
            .map(|(meta, data)| Snapshot {
                meta: meta.clone(),
                snapshot: Box::new(Cursor::new(data.clone())),
            }))
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        self.clone()
    }
}

#[cfg(test)]
#[derive(Clone)]
struct MemHub {
    nodes: Arc<Mutex<BTreeMap<NodeId, RaftNode>>>,
}

#[cfg(test)]
struct MemFactory {
    hub: MemHub,
}

#[cfg(test)]
struct MemConn {
    hub: MemHub,
    target: NodeId,
}

#[cfg(test)]
impl RaftNetworkFactory<TypeConfig> for MemFactory {
    type Network = MemConn;

    async fn new_client(&mut self, target: NodeId, _node: &BasicNode) -> Self::Network {
        MemConn {
            hub: self.hub.clone(),
            target,
        }
    }
}

#[cfg(test)]
impl RaftNetwork<TypeConfig> for MemConn {
    async fn append_entries(
        &mut self,
        req: AppendEntriesRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<NodeId>, RPCError<NodeId, BasicNode, RaftError<NodeId>>> {
        let raft = self
            .hub
            .nodes
            .lock()
            .await
            .get(&self.target)
            .cloned()
            .ok_or_else(|| {
                RPCError::Network(NetworkError::new(&AnyError::error("missing voter")))
            })?;
        raft.append_entries(req)
            .await
            .map_err(|err| RPCError::RemoteError(RemoteError::new(self.target, err)))
    }

    async fn install_snapshot(
        &mut self,
        req: InstallSnapshotRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<
        InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, BasicNode, RaftError<NodeId, InstallSnapshotError>>,
    > {
        let raft = self
            .hub
            .nodes
            .lock()
            .await
            .get(&self.target)
            .cloned()
            .ok_or_else(|| {
                RPCError::Network(NetworkError::new(&AnyError::error("missing voter")))
            })?;
        raft.install_snapshot(req)
            .await
            .map_err(|err| RPCError::RemoteError(RemoteError::new(self.target, err)))
    }

    async fn vote(
        &mut self,
        req: VoteRequest<NodeId>,
        _option: RPCOption,
    ) -> Result<VoteResponse<NodeId>, RPCError<NodeId, BasicNode, RaftError<NodeId>>> {
        let raft = self
            .hub
            .nodes
            .lock()
            .await
            .get(&self.target)
            .cloned()
            .ok_or_else(|| {
                RPCError::Network(NetworkError::new(&AnyError::error("missing voter")))
            })?;
        raft.vote(req)
            .await
            .map_err(|err| RPCError::RemoteError(RemoteError::new(self.target, err)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn three_memory_voters_elect_one_leader() {
        let hub = MemHub {
            nodes: Arc::new(Mutex::new(BTreeMap::new())),
        };
        let mut peers = BTreeMap::new();
        let mut created = Vec::new();
        for id in 1..=3 {
            peers.insert(
                id,
                BasicNode {
                    addr: format!("mem-{id}"),
                },
            );
            let config = Config {
                heartbeat_interval: 50,
                election_timeout_min: 150,
                election_timeout_max: 300,
                ..Default::default()
            };
            let config = Arc::new(config.validate().unwrap());
            let raft = RaftNode::new(
                id,
                config,
                MemFactory { hub: hub.clone() },
                LogStore::memory(),
                StateMachineStore::memory(),
            )
            .await
            .unwrap();
            created.push(raft);
        }
        {
            let mut guard = hub.nodes.lock().await;
            for raft in &created {
                guard.insert(raft.metrics().borrow().id, raft.clone());
            }
        }
        created[0].initialize(peers).await.unwrap();
        let mut leader = None;
        for _ in 0..40 {
            for raft in &created {
                let metrics = raft.metrics().borrow().clone();
                if metrics.state == ServerState::Leader {
                    leader = metrics.current_leader;
                }
            }
            if leader.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let leader = leader.expect("three voters elect a leader");
        assert!((1..=3).contains(&leader));
        assert!(created.iter().all(|raft| {
            raft.metrics()
                .borrow()
                .membership_config
                .membership()
                .voter_ids()
                .count()
                >= 3
        }));
        let leader_raft = created
            .iter()
            .find(|raft| raft.metrics().borrow().current_leader == Some(leader))
            .expect("leader handle")
            .clone();
        let admitted = leader_raft
            .client_write(LeaseCommand::Admit {
                id: "endpoint:shared".into(),
                now: 1_700_000_000,
                tokens: 1,
                rpm: 10,
                tpm: 100,
                streams: 0,
            })
            .await
            .unwrap()
            .data;
        assert_eq!(admitted, LeaseResponse::Admitted { stream_id: None });
        let linked = leader_raft
            .client_write(LeaseCommand::AuditLink {
                line: "admin|create|model|ok".into(),
            })
            .await
            .unwrap()
            .data;
        match linked {
            LeaseResponse::AuditLinked { tip } => assert!(!tip.is_empty()),
            other => panic!("expected AuditLinked, got {other:?}"),
        }
    }

    #[test]
    fn a_second_admit_is_rejected_and_release_drops_the_inflight_slot() {
        let mut data = MachineData::default();
        let first = apply_command(
            &mut data,
            LeaseCommand::Admit {
                id: "endpoint:qwen".into(),
                now: 1_000,
                tokens: 4,
                rpm: 1,
                tpm: 10,
                streams: 1,
            },
        );
        assert_eq!(
            first,
            LeaseResponse::Admitted {
                stream_id: Some("endpoint:qwen".into())
            }
        );
        let second = apply_command(
            &mut data,
            LeaseCommand::Admit {
                id: "endpoint:qwen".into(),
                now: 1_001,
                tokens: 4,
                rpm: 1,
                tpm: 10,
                streams: 1,
            },
        );
        assert!(matches!(second, LeaseResponse::Rejected { .. }));
        assert_eq!(
            data.counters
                .get("endpoint:qwen")
                .map(|counter| counter.inflight),
            Some(1)
        );
        assert_eq!(
            apply_command(
                &mut data,
                LeaseCommand::Release {
                    id: "endpoint:qwen".into()
                }
            ),
            LeaseResponse::Released
        );
        assert_eq!(
            data.counters
                .get("endpoint:qwen")
                .map(|counter| counter.inflight),
            Some(0)
        );
    }
}
