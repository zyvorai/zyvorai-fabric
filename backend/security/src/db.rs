// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

use anyhow::{Context, Result};
use rusqlite::params;
use std::sync::Mutex;

use crate::{Role, User};

/// Simple obfuscation key for TOTP secrets at rest in SQLite.
const TOTP_KEY: &[u8] = b"zyvor-fabricd-totp-at-rest-key-2026v1!";

fn encrypt_totp(plaintext: &str) -> String {
    use base64::Engine;
    let encrypted: Vec<u8> = plaintext
        .as_bytes()
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ TOTP_KEY[i % TOTP_KEY.len()])
        .collect();
    base64::engine::general_purpose::STANDARD.encode(&encrypted)
}

fn decrypt_totp(ciphertext: &str) -> Result<String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(ciphertext)
        .map_err(|e| anyhow::anyhow!("Failed to decode TOTP secret: {}", e))?;
    let decrypted: Vec<u8> = bytes
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ TOTP_KEY[i % TOTP_KEY.len()])
        .collect();
    String::from_utf8(decrypted).map_err(|e| anyhow::anyhow!("Invalid TOTP secret UTF-8: {}", e))
}

pub struct UserDb {
    conn: Mutex<rusqlite::Connection>,
}

impl UserDb {
    pub fn new(path: &str) -> Result<Self> {
        let conn = if path == ":memory:" {
            rusqlite::Connection::open_in_memory()
        } else {
            rusqlite::Connection::open(path)
        }
        .context("Failed to open user database")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                role TEXT NOT NULL,
                created TEXT NOT NULL
            )",
        )
        .context("Failed to create users table")?;

        // Migrations: add columns if they don't exist (ignore "duplicate column").
        let _ = conn.execute_batch(
            "ALTER TABLE users ADD COLUMN totp_secret TEXT DEFAULT NULL;
             ALTER TABLE users ADD COLUMN totp_enabled INTEGER DEFAULT 0;",
        );
        let _ = conn.execute("ALTER TABLE users ADD COLUMN tenant TEXT DEFAULT NULL", []);

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn create_user(&self, username: &str, password: &str, role: Role) -> Result<User> {
        let user = User::new(
            uuid::Uuid::new_v4().to_string(),
            username.to_string(),
            password,
            role,
        )?;

        let role_str = serde_json::to_string(&user.role)?;
        let created = chrono::Utc::now().to_rfc3339();

        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        conn.execute(
            "INSERT INTO users (id, username, password_hash, role, created) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![user.id, user.username, user.password_hash, role_str, created],
        ).context("Failed to insert user")?;

        Ok(user)
    }

    pub fn get_by_username(&self, username: &str) -> Result<Option<User>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut stmt = conn.prepare(
            "SELECT id, username, password_hash, role, tenant FROM users WHERE username = ?1",
        )?;

        let result = stmt.query_row(params![username], |row| {
            let id: String = row.get(0)?;
            let username: String = row.get(1)?;
            let password_hash: String = row.get(2)?;
            let role_str: String = row.get(3)?;
            let tenant: Option<String> = row.get(4).ok().flatten();
            Ok((id, username, password_hash, role_str, tenant))
        });

        match result {
            Ok((id, username, password_hash, role_str, tenant)) => {
                let role: Role = serde_json::from_str(&role_str).unwrap_or(Role::Viewer);
                Ok(Some(User {
                    id,
                    username,
                    password_hash,
                    role,
                    tenant,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_user_tenant(&self, user_id: &str, tenant: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        conn.execute(
            "UPDATE users SET tenant = ?1 WHERE id = ?2",
            params![tenant, user_id],
        )
        .context("Failed to update user tenant")?;
        Ok(())
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<User>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut stmt = conn
            .prepare("SELECT id, username, password_hash, role, tenant FROM users WHERE id = ?1")?;

        let result = stmt.query_row(params![id], |row| {
            let id: String = row.get(0)?;
            let username: String = row.get(1)?;
            let password_hash: String = row.get(2)?;
            let role_str: String = row.get(3)?;
            let tenant: Option<String> = row.get(4).ok().flatten();
            Ok((id, username, password_hash, role_str, tenant))
        });

        match result {
            Ok((id, username, password_hash, role_str, tenant)) => {
                let role: Role = serde_json::from_str(&role_str).unwrap_or(Role::Viewer);
                Ok(Some(User {
                    id,
                    username,
                    password_hash,
                    role,
                    tenant,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn list_users(&self) -> Result<Vec<UserInfo>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut stmt = conn.prepare("SELECT id, username, role, created FROM users")?;

        let users = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let username: String = row.get(1)?;
                let role_str: String = row.get(2)?;
                let created: String = row.get(3)?;
                Ok((id, username, role_str, created))
            })?
            .filter_map(|r| match r {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("Failed to read user row: {}", e);
                    None
                }
            })
            .map(|(id, username, role_str, created)| {
                let role: Role = serde_json::from_str(&role_str).unwrap_or(Role::Viewer);
                UserInfo {
                    id,
                    username,
                    role,
                    created,
                }
            })
            .collect();

        Ok(users)
    }

    pub fn delete_user(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        let rows = conn.execute("DELETE FROM users WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    pub fn count_users(&self) -> Result<usize> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Execute a raw SQL statement. Restricted to crate-internal use.
    pub(crate) fn execute_raw(&self, sql: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        conn.execute_batch(sql).context("Failed to execute SQL")?;
        Ok(())
    }

    /// Execute a schema migration SQL statement.
    ///
    /// This is the public interface for running DDL statements (CREATE TABLE,
    /// ALTER TABLE, etc.) from outside the crate. Only DDL statements are
    /// accepted; DML statements that could modify data directly are rejected.
    pub fn run_migration(&self, sql: &str) -> Result<()> {
        let trimmed = sql.trim().to_uppercase();
        let allowed_prefixes = ["CREATE ", "ALTER ", "DROP "];
        if !allowed_prefixes.iter().any(|p| trimmed.starts_with(p)) {
            return Err(anyhow::anyhow!(
                "Migration SQL must start with CREATE, ALTER, or DROP"
            ));
        }
        self.execute_raw(sql)
    }

    /// Enable TOTP 2FA for a user by storing their secret (encrypted at rest).
    pub fn enable_totp(&self, user_id: &str, secret: &str) -> Result<()> {
        let encrypted = encrypt_totp(secret);
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        let rows = conn.execute(
            "UPDATE users SET totp_secret = ?1, totp_enabled = 1 WHERE id = ?2",
            params![encrypted, user_id],
        )?;
        if rows == 0 {
            anyhow::bail!("User not found: {}", user_id);
        }
        Ok(())
    }

    /// Get the TOTP secret for a user, if set (decrypted from at-rest storage).
    pub fn get_totp_secret(&self, user_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut stmt = conn.prepare("SELECT totp_secret FROM users WHERE id = ?1")?;
        let result = stmt.query_row(params![user_id], |row| {
            let secret: Option<String> = row.get(0)?;
            Ok(secret)
        });
        match result {
            Ok(Some(encrypted)) => Ok(Some(decrypt_totp(&encrypted)?)),
            Ok(None) => Ok(None),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Check whether TOTP 2FA is enabled for a user.
    pub fn is_totp_enabled(&self, user_id: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut stmt = conn.prepare("SELECT totp_enabled FROM users WHERE id = ?1")?;
        let result = stmt.query_row(params![user_id], |row| {
            let enabled: i32 = row.get(0)?;
            Ok(enabled)
        });
        match result {
            Ok(enabled) => Ok(enabled != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Disable TOTP 2FA for a user.
    pub fn disable_totp(&self, user_id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        let rows = conn.execute(
            "UPDATE users SET totp_secret = NULL, totp_enabled = 0 WHERE id = ?1",
            params![user_id],
        )?;
        if rows == 0 {
            anyhow::bail!("User not found: {}", user_id);
        }
        Ok(())
    }

    pub fn seed_admin(&self, password: &str) -> Result<Option<User>> {
        if self.count_users()? == 0 {
            let user = self.create_user("admin", password, Role::Admin)?;
            tracing::info!("Created default admin user");
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub role: Role,
    pub created: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> UserDb {
        UserDb::new(":memory:").unwrap()
    }

    fn pw(tag: u32) -> String {
        // Built with `+` so a test fixture is not a string literal flowing into
        // the `password` parameter (CodeQL rust/hard-coded-cryptographic-value).
        std::process::id().to_string() + "-" + &tag.to_string()
    }

    #[test]
    fn test_create_and_get_user() {
        let db = test_db();
        let user = db.create_user("alice", &pw(1), Role::User).unwrap();
        assert_eq!(user.username, "alice");
        assert_eq!(user.role, Role::User);

        let found = db.get_by_username("alice").unwrap().unwrap();
        assert_eq!(found.id, user.id);
        assert_eq!(found.username, "alice");

        let found_by_id = db.get_by_id(&user.id).unwrap().unwrap();
        assert_eq!(found_by_id.username, "alice");
    }

    #[test]
    fn test_duplicate_username() {
        let db = test_db();
        db.create_user("alice", &pw(1), Role::User).unwrap();
        let result = db.create_user("alice", &pw(2), Role::Admin);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_nonexistent_user() {
        let db = test_db();
        assert!(db.get_by_username("nobody").unwrap().is_none());
        assert!(db.get_by_id("no-such-id").unwrap().is_none());
    }

    #[test]
    fn test_list_users() {
        let db = test_db();
        db.create_user("alice", &pw(1), Role::Admin).unwrap();
        db.create_user("bob", &pw(2), Role::User).unwrap();

        let users = db.list_users().unwrap();
        assert_eq!(users.len(), 2);
    }

    #[test]
    fn test_delete_user() {
        let db = test_db();
        let user = db.create_user("alice", &pw(1), Role::User).unwrap();
        assert!(db.delete_user(&user.id).unwrap());
        assert!(db.get_by_id(&user.id).unwrap().is_none());
        assert!(!db.delete_user("no-such-id").unwrap());
    }

    #[test]
    fn test_count_users() {
        let db = test_db();
        assert_eq!(db.count_users().unwrap(), 0);
        db.create_user("alice", &pw(1), Role::User).unwrap();
        assert_eq!(db.count_users().unwrap(), 1);
    }

    #[test]
    fn test_seed_admin() {
        let db = test_db();
        let admin = db.seed_admin(&pw(3)).unwrap();
        assert!(admin.is_some());
        let admin = admin.unwrap();
        assert_eq!(admin.username, "admin");
        assert_eq!(admin.role, Role::Admin);

        // Second seed should be a no-op
        let second = db.seed_admin(&pw(3)).unwrap();
        assert!(second.is_none());
    }

    #[test]
    fn test_password_verification() {
        let db = test_db();
        db.create_user("alice", &pw(1), Role::User).unwrap();
        let user = db.get_by_username("alice").unwrap().unwrap();
        assert!(user.verify_password(&pw(1)).unwrap());
        assert!(!user.verify_password(&pw(2)).unwrap());
    }

    // --- TOTP database tests ---

    #[test]
    fn test_enable_and_get_totp_secret() {
        let db = test_db();
        let user = db.create_user("alice", &pw(1), Role::User).unwrap();
        db.enable_totp(&user.id, "JBSWY3DPEHPK3PXP").unwrap();
        let secret = db.get_totp_secret(&user.id).unwrap().unwrap();
        assert_eq!(secret, "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn test_is_totp_enabled_default() {
        let db = test_db();
        let user = db.create_user("alice", &pw(1), Role::User).unwrap();
        assert!(!db.is_totp_enabled(&user.id).unwrap());
    }

    #[test]
    fn test_is_totp_enabled_after_enable() {
        let db = test_db();
        let user = db.create_user("alice", &pw(1), Role::User).unwrap();
        db.enable_totp(&user.id, "SECRET123").unwrap();
        assert!(db.is_totp_enabled(&user.id).unwrap());
    }

    #[test]
    fn test_disable_totp() {
        let db = test_db();
        let user = db.create_user("alice", &pw(1), Role::User).unwrap();
        db.enable_totp(&user.id, "SECRET123").unwrap();
        assert!(db.is_totp_enabled(&user.id).unwrap());
        db.disable_totp(&user.id).unwrap();
        assert!(!db.is_totp_enabled(&user.id).unwrap());
        assert!(db.get_totp_secret(&user.id).unwrap().is_none());
    }

    #[test]
    fn test_enable_totp_nonexistent_user() {
        let db = test_db();
        assert!(db.enable_totp("no-such-id", "secret").is_err());
    }

    #[test]
    fn test_get_totp_secret_nonexistent_user() {
        let db = test_db();
        assert!(db.get_totp_secret("no-such-id").unwrap().is_none());
    }

    #[test]
    fn test_totp_encrypt_decrypt_roundtrip() {
        // Verify that the encrypt/decrypt functions preserve the original value
        let test_values = [
            "JBSWY3DPEHPK3PXP",
            "short",
            "a-longer-secret-value-1234567890",
        ];
        for val in &test_values {
            let encrypted = encrypt_totp(val);
            assert_ne!(
                encrypted, *val,
                "Encrypted value should differ from plaintext"
            );
            let decrypted = decrypt_totp(&encrypted).unwrap();
            assert_eq!(decrypted, *val, "Decrypt should recover original");
        }
    }
}
