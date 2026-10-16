// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

//! Which program in the cell made a request?
//!
//! An egress rule can list `binaries`. To enforce one, the runtime asks the guest agent to run
//! `/opt/zyvor/attribute.sh` with the connection's source port. The script walks `/proc` as root,
//! from the socket to the process that holds it, and returns that process's executable path and
//! SHA-256 (see `guest/attribute.sh`).
//!
//! What this is: a check against an agent *process* that has been talked into calling out through
//! the wrong program, or that dropped in its own binary. What it is not: protection from a
//! compromised guest kernel, which can lie to the script. The check needs the host channel, so it
//! does not work for a confidential cell, and it **fails closed**: when the caller cannot be
//! identified, a rule that names binaries refuses the request.
//!
//! A rule can pin a hash (`sha256`). Without one the first hash seen for that agent and path is
//! remembered (trust on first use) and a different hash is refused until an operator clears the pin.

use crate::model::SessionRecord;
use crate::AppState;
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Guest path of the attribution script.
pub const GUEST_PATH: &str = "/opt/zyvor/attribute.sh";
pub const SCRIPT: &str = include_str!("../guest/attribute.sh");

const CACHE_TTL: Duration = Duration::from_secs(3);
const CACHE_MAX: usize = 1024;
const LOOKUP_TIMEOUT_SECS: u64 = 5;
const PINS_FILE: &str = "binary-pins.json";

/// The program behind a connection, as the guest reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryIdentity {
    pub pid: u32,
    pub path: String,
    pub sha256: String,
}

/// Read the script's one line: `<pid> <exe> <sha256>`. The path may contain spaces. A path that
/// is not absolute, or that the kernel marks ` (deleted)`, is not an identity anyone should trust.
pub fn parse_attribution(stdout: &str) -> Option<BinaryIdentity> {
    let line = stdout.trim();
    if line.contains('\n') {
        return None;
    }
    let (pid, rest) = line.split_once(' ')?;
    let (path, sha256) = rest.rsplit_once(' ')?;
    let pid: u32 = pid.parse().ok()?;
    let valid_sha = sha256.len() == 64
        && sha256
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if !valid_sha
        || !path.starts_with('/')
        || path.ends_with(" (deleted)")
        || path.chars().any(char::is_control)
    {
        return None;
    }
    Some(BinaryIdentity {
        pid,
        path: path.to_string(),
        sha256: sha256.to_string(),
    })
}

type Cache = Mutex<HashMap<(Uuid, u16, u16), (Instant, BinaryIdentity)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Ask the guest which program owns the connection `peer` opened to the runtime's `server_port`.
/// The error says why in words fit for a log; do not show it to the agent.
pub async fn attribute(
    state: &AppState,
    session: &SessionRecord,
    peer: Option<SocketAddr>,
    server_port: u16,
) -> Result<BinaryIdentity, String> {
    let peer =
        peer.ok_or("the request did not arrive on a connection the runtime can attribute")?;
    let key = (session.id, peer.port(), server_port);
    if let Some((at, id)) = cache().lock().unwrap().get(&key) {
        if at.elapsed() < CACHE_TTL {
            return Ok(id.clone());
        }
    }
    // Both arguments are integers, so nothing the agent controls reaches the shell.
    let command = format!("{GUEST_PATH} {} {server_port}", peer.port());
    let reply = state
        .fluxvm
        .process_for_session(
            session.confidential.as_ref(),
            session.sandbox_id,
            &command,
            Some(LOOKUP_TIMEOUT_SECS),
        )
        .await
        .map_err(|e| format!("the guest lookup failed: {e}"))?;
    if reply.get("result").and_then(|r| r.as_str()) != Some("exec")
        || reply.get("exit_code").and_then(|c| c.as_i64()) != Some(0)
    {
        return Err("the guest could not find the program that owns the connection".into());
    }
    let id = parse_attribution(reply.get("stdout").and_then(|s| s.as_str()).unwrap_or(""))
        .ok_or("the guest's answer was not a usable program identity")?;
    let mut cache = cache().lock().unwrap();
    if cache.len() >= CACHE_MAX {
        cache.clear();
    }
    cache.insert(key, (Instant::now(), id.clone()));
    Ok(id)
}

// ---- trust on first use -------------------------------------------------------------------

fn pins_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Default::default)
}

fn pins_path(dir: &Path) -> PathBuf {
    dir.join(PINS_FILE)
}

fn pin_key(agent: &str, path: &str) -> String {
    format!("{agent}\u{1f}{path}")
}

async fn read_pins(dir: &Path) -> Result<HashMap<String, String>, String> {
    match tokio::fs::read(pins_path(dir)).await {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("binary pins are unreadable: {e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(format!("binary pins could not be read: {e}")),
    }
}

async fn write_pins(dir: &Path, pins: &HashMap<String, String>) -> Result<(), String> {
    let tmp = dir.join(format!("{PINS_FILE}.tmp"));
    let bytes = serde_json::to_vec_pretty(pins).map_err(|e| e.to_string())?;
    tokio::fs::write(&tmp, bytes)
        .await
        .map_err(|e| format!("binary pins could not be written: {e}"))?;
    tokio::fs::rename(&tmp, pins_path(dir))
        .await
        .map_err(|e| format!("binary pins could not be written: {e}"))
}

/// Trust on first use: remember the hash of `id` for `agent`, or refuse a hash that differs from
/// the one remembered. An unreadable pin file refuses too, so it cannot be used to forget a pin.
pub async fn check_or_pin(dir: &Path, agent: &str, id: &BinaryIdentity) -> Result<(), String> {
    let _guard = pins_lock().lock().await;
    let mut pins = read_pins(dir).await?;
    let key = pin_key(agent, &id.path);
    match pins.get(&key) {
        Some(pinned) if pinned == &id.sha256 => Ok(()),
        Some(_) => Err(format!(
            "{} is not the program first seen for this agent (its hash changed); an operator must clear the pin",
            id.path
        )),
        None => {
            pins.insert(key, id.sha256.clone());
            write_pins(dir, &pins).await
        }
    }
}

/// The pinned hash for each program path of `agent`.
pub async fn list_pins(dir: &Path, agent: &str) -> Result<Vec<(String, String)>, String> {
    let _guard = pins_lock().lock().await;
    let prefix = format!("{agent}\u{1f}");
    let mut out: Vec<(String, String)> = read_pins(dir)
        .await?
        .into_iter()
        .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|p| (p.to_string(), v)))
        .collect();
    out.sort();
    Ok(out)
}

/// Forget every pin of `agent`, or just the one for `path`. Returns how many were removed.
pub async fn clear_pins(dir: &Path, agent: &str, path: Option<&str>) -> Result<usize, String> {
    let _guard = pins_lock().lock().await;
    let mut pins = read_pins(dir).await?;
    let before = pins.len();
    match path {
        Some(p) => {
            pins.remove(&pin_key(agent, p));
        }
        None => {
            let prefix = format!("{agent}\u{1f}");
            pins.retain(|k, _| !k.starts_with(&prefix));
        }
    }
    let removed = before - pins.len();
    if removed > 0 {
        write_pins(dir, &pins).await?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "52e0a13e60a981d8c4b6478be2ba5176f69da07948a056bf49cf6f077e30cb41";

    fn id(path: &str, sha: &str) -> BinaryIdentity {
        BinaryIdentity {
            pid: 1,
            path: path.into(),
            sha256: sha.into(),
        }
    }

    #[test]
    fn parses_the_scripts_line_including_paths_with_spaces() {
        let got = parse_attribution(&format!("904763 /usr/bin/python3.14 {SHA}\n")).unwrap();
        assert_eq!(
            got,
            BinaryIdentity {
                pid: 904763,
                path: "/usr/bin/python3.14".into(),
                sha256: SHA.into()
            }
        );
        let spaced = parse_attribution(&format!("7 /opt/my app/bin/tool {SHA}")).unwrap();
        assert_eq!(spaced.path, "/opt/my app/bin/tool");
    }

    #[test]
    fn refuses_anything_that_is_not_a_clean_identity() {
        for bad in [
            "",
            "garbage",
            &format!("x /usr/bin/a {SHA}"),
            &format!("1 usr/bin/a {SHA}"),            // not absolute
            &format!("1 /usr/bin/a (deleted) {SHA}"), // replaced or removed on disk
            "1 /usr/bin/a deadbeef",                  // short hash
            &format!("1 /usr/bin/a {}", SHA.to_uppercase()), // hashes are lower-case hex
            &format!("1 /usr/bin/a {SHA}\n2 /usr/bin/b {SHA}"), // more than one line
            &format!("1 /usr/bin/a\u{7}b {SHA}"),
        ] {
            assert!(parse_attribution(bad).is_none(), "{bad:?} should not parse");
        }
    }

    fn scratch() -> PathBuf {
        let d = std::env::temp_dir().join(format!("zyvor-pins-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn first_use_is_remembered_and_a_changed_hash_is_refused() {
        let dir = scratch();
        let node = id("/usr/bin/node", SHA);
        check_or_pin(&dir, "desk", &node).await.unwrap();
        check_or_pin(&dir, "desk", &node).await.unwrap();
        let other = id("/usr/bin/node", &"b".repeat(64));
        let err = check_or_pin(&dir, "desk", &other).await.unwrap_err();
        assert!(err.contains("clear the pin"), "{err}");
        // Another path, and another agent, are independent.
        check_or_pin(&dir, "desk", &id("/usr/bin/curl", &"b".repeat(64)))
            .await
            .unwrap();
        check_or_pin(&dir, "other", &other).await.unwrap();
        // Pins survive a restart: they are read from disk each time.
        assert!(check_or_pin(&dir, "desk", &other).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn pins_can_be_listed_and_cleared() {
        let dir = scratch();
        check_or_pin(&dir, "desk", &id("/usr/bin/node", SHA))
            .await
            .unwrap();
        check_or_pin(&dir, "desk", &id("/usr/bin/curl", &"c".repeat(64)))
            .await
            .unwrap();
        check_or_pin(&dir, "other", &id("/usr/bin/node", SHA))
            .await
            .unwrap();
        let listed = list_pins(&dir, "desk").await.unwrap();
        assert_eq!(
            listed.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
            ["/usr/bin/curl", "/usr/bin/node"]
        );
        assert_eq!(
            clear_pins(&dir, "desk", Some("/usr/bin/node"))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            clear_pins(&dir, "desk", Some("/usr/bin/node"))
                .await
                .unwrap(),
            0
        );
        // After clearing, a new hash is accepted and pinned afresh.
        check_or_pin(&dir, "desk", &id("/usr/bin/node", &"d".repeat(64)))
            .await
            .unwrap();
        assert_eq!(clear_pins(&dir, "desk", None).await.unwrap(), 2);
        assert!(list_pins(&dir, "desk").await.unwrap().is_empty());
        // The other agent's pin is untouched.
        assert_eq!(list_pins(&dir, "other").await.unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_corrupt_pin_file_refuses_instead_of_forgetting() {
        let dir = scratch();
        std::fs::write(pins_path(&dir), b"{not json").unwrap();
        let err = check_or_pin(&dir, "desk", &id("/usr/bin/node", SHA))
            .await
            .unwrap_err();
        assert!(err.contains("unreadable"), "{err}");
        assert_eq!(std::fs::read(pins_path(&dir)).unwrap(), b"{not json");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_pin_endpoints_are_operator_only_and_clear_what_they_list() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let token = crate::fixture::text("operator-token");
        let (state, _) =
            crate::egress::ask_tests::state_and_session_cfg(|c| c.api_token = Some(token.clone()))
                .await;
        let app = crate::app::public_router(state.clone());
        let call = |method: &'static str, uri: String, tok: String, body: Option<String>| {
            let app = app.clone();
            async move {
                let b = Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("authorization", format!("Bearer {tok}"));
                let req = match body {
                    Some(v) => b
                        .header("content-type", "application/json")
                        .body(Body::from(v)),
                    None => b.body(Body::empty()),
                }
                .unwrap();
                let resp = app.oneshot(req).await.unwrap();
                let st = resp.status();
                let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
                    .await
                    .unwrap();
                (
                    st,
                    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or_default(),
                )
            }
        };
        let deploy = serde_json::json!({
            "name": "pin-desk", "bundle_base64": "ZXhwb3J0IGRlZmF1bHQgMQ==",
            "manifest": {"template": "agent-node", "egress_mode": "deny"}
        });
        let (st, _) = call(
            "POST",
            "/v1/agents".into(),
            token.clone(),
            Some(deploy.to_string()),
        )
        .await;
        assert_eq!(st, 201);
        let dir = state.config.state_dir.clone();
        check_or_pin(&dir, "pin-desk", &id("/usr/bin/node", SHA))
            .await
            .unwrap();
        check_or_pin(&dir, "pin-desk", &id("/usr/bin/curl", &"c".repeat(64)))
            .await
            .unwrap();

        let (st, v) = call(
            "GET",
            "/v1/agents/pin-desk/binary-pins".into(),
            token.clone(),
            None,
        )
        .await;
        assert_eq!(st, 200, "{v}");
        assert_eq!(v["pins"].as_array().unwrap().len(), 2);
        assert_eq!(v["pins"][1]["path"], "/usr/bin/node");
        assert_eq!(v["pins"][1]["sha256"], SHA);

        let (st, _) = call(
            "GET",
            "/v1/agents/nobody/binary-pins".into(),
            token.clone(),
            None,
        )
        .await;
        assert_eq!(st, 404);

        // A user token cannot read or clear pins.
        let (st, v) = call(
            "POST",
            "/v1/user-tokens".into(),
            token.clone(),
            Some(serde_json::json!({"user_id": "ana", "ttl_seconds": 600}).to_string()),
        )
        .await;
        assert_eq!(st, 201, "{v}");
        let user_tok = v["token"].as_str().unwrap().to_string();
        assert_eq!(
            call(
                "GET",
                "/v1/agents/pin-desk/binary-pins".into(),
                user_tok.clone(),
                None
            )
            .await
            .0,
            403
        );
        assert_eq!(
            call(
                "DELETE",
                "/v1/agents/pin-desk/binary-pins".into(),
                user_tok,
                None
            )
            .await
            .0,
            403
        );
        assert_eq!(list_pins(&dir, "pin-desk").await.unwrap().len(), 2);

        let (st, v) = call(
            "DELETE",
            "/v1/agents/pin-desk/binary-pins?path=/usr/bin/node".into(),
            token.clone(),
            None,
        )
        .await;
        assert_eq!(
            (st.as_u16(), v["removed"].clone()),
            (200, serde_json::json!(1)),
            "{v}"
        );
        let (_, v) = call(
            "DELETE",
            "/v1/agents/pin-desk/binary-pins".into(),
            token.clone(),
            None,
        )
        .await;
        assert_eq!(v["removed"], 1);
        assert!(list_pins(&dir, "pin-desk").await.unwrap().is_empty());
        // Clearing is journaled.
        let rows = state.store.audit.list(None, 50).await.unwrap();
        assert!(rows.iter().any(|r| r.action == "merion.binary_pins.cleared"));
    }

    /// The guest script against real sockets: the test process owns both ends of a connection,
    /// so the script must report this very executable and its hash. Linux only.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_guest_script_names_the_process_that_holds_a_connection() {
        use sha2::{Digest, Sha256};
        use std::net::{TcpListener, TcpStream};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_port = listener.local_addr().unwrap().port();
        let client = TcpStream::connect(("127.0.0.1", server_port)).unwrap();
        let (_accepted, _) = listener.accept().unwrap();
        let client_port = client.local_addr().unwrap().port();

        let dir = scratch();
        let script = dir.join("attribute.sh");
        std::fs::write(&script, SCRIPT).unwrap();
        let run = |a: &str, b: &str| {
            std::process::Command::new("sh")
                .arg(&script)
                .args([a, b])
                .output()
                .unwrap()
        };
        let out = run(&client_port.to_string(), &server_port.to_string());
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let got =
            parse_attribution(&String::from_utf8_lossy(&out.stdout)).expect("a usable identity");
        let exe = std::fs::read_link("/proc/self/exe").unwrap();
        assert_eq!(got.pid, std::process::id());
        assert_eq!(got.path, exe.to_string_lossy());
        assert_eq!(
            got.sha256,
            hex::encode(Sha256::digest(std::fs::read(&exe).unwrap()))
        );

        // A connection that does not exist, and arguments that are not ports.
        assert_eq!(run(&client_port.to_string(), "1").status.code(), Some(1));
        for bad in [("1;id", "2"), ("70000", "1"), ("", "1"), ("-1", "1")] {
            let r = run(bad.0, bad.1);
            assert_eq!(r.status.code(), Some(2), "{bad:?}");
            assert!(r.stdout.is_empty());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
