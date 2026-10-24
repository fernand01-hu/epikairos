use minijinja::Environment;
use rand::RngCore;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::db::DbPool;
use crate::feed::FeedDbPool;

// File name kept from the Flask backend so existing installs keep their sessions
const SESSION_SECRET_FILE: &str = ".flask_secret";

/// Resolves a secret file inside `SECRET_DIR` (defaults to the working directory).
/// Docker points this at the persistent data volume so keys survive rebuilds.
pub fn secret_path(file_name: &str) -> PathBuf {
    let dir = std::env::var("SECRET_DIR").unwrap_or_else(|_| ".".to_string());
    Path::new(&dir).join(file_name)
}

#[derive(Clone)]
pub struct AppState {
    pub db_pool: DbPool,
    pub feed_pool: FeedDbPool,
    pub jinja: Arc<Environment<'static>>,
    pub login_attempts: Arc<Mutex<HashMap<String, Vec<f64>>>>,
    pub secret_key: String,
}

impl AppState {
    pub fn new(db_pool: DbPool, feed_pool: FeedDbPool, jinja: Environment<'static>) -> Self {
        let secret_key = get_or_create_session_secret();
        Self {
            db_pool,
            feed_pool,
            jinja: Arc::new(jinja),
            login_attempts: Arc::new(Mutex::new(HashMap::new())),
            secret_key,
        }
    }

    pub async fn check_login_rate_limit(&self, client_ip: &str) -> bool {
        const RATE_LIMIT_WINDOW: f64 = 60.0;
        const MAX_LOGIN_ATTEMPTS: usize = 10;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        let mut lock = self.login_attempts.lock().await;

        // Prune expired timestamps across all entries and remove entries that become empty
        lock.retain(|_ip, attempts| {
            attempts.retain(|&t| now - t < RATE_LIMIT_WINDOW);
            !attempts.is_empty()
        });

        let attempts = lock.entry(client_ip.to_string()).or_default();

        if attempts.len() >= MAX_LOGIN_ATTEMPTS {
            return false;
        }

        attempts.push(now);
        true
    }
}

pub fn get_or_create_session_secret() -> String {
    let env_key = std::env::var("SESSION_SECRET_KEY").or_else(|_| std::env::var("FLASK_SECRET_KEY"));
    if let Ok(env_key) = env_key {
        if !env_key.trim().is_empty() && env_key != "auriga-epita-secret-key-change-in-prod" {
            return env_key;
        }
    }

    let secret_file = secret_path(SESSION_SECRET_FILE);
    if secret_file.exists() {
        if let Ok(mut f) = File::open(&secret_file) {
            let mut s = String::new();
            if f.read_to_string(&mut s).is_ok() && s.trim().len() >= 32 {
                return s.trim().to_string();
            }
        }
    }

    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let hex_key = hex::encode(bytes);

    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);

    if let Ok(mut f) = opts.open(&secret_file) {
        let _ = f.write_all(hex_key.as_bytes());
    }

    hex_key
}

mod hex {
    pub fn encode(data: [u8; 32]) -> String {
        let mut s = String::with_capacity(64);
        for b in data {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use r2d2_sqlite::SqliteConnectionManager;

    fn create_test_state() -> AppState {
        let db_manager = SqliteConnectionManager::memory();
        let db_pool = r2d2::Pool::builder().max_size(1).build(db_manager).unwrap();
        let feed_manager = SqliteConnectionManager::memory();
        let feed_pool = r2d2::Pool::builder().max_size(1).build(feed_manager).unwrap();
        let jinja = minijinja::Environment::new();
        AppState::new(db_pool, feed_pool, jinja)
    }

    #[tokio::test]
    async fn test_rate_limiter_allows_up_to_max() {
        let state = create_test_state();
        let ip = "192.168.1.100";

        for _ in 0..10 {
            assert!(state.check_login_rate_limit(ip).await);
        }

        // 11th attempt should fail
        assert!(!state.check_login_rate_limit(ip).await);

        // Different IP should succeed
        assert!(state.check_login_rate_limit("192.168.1.101").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_prunes_expired_and_empty_entries() {
        let state = create_test_state();

        // Inject old timestamps for an IP
        {
            let mut lock = state.login_attempts.lock().await;
            lock.insert("10.0.0.1".to_string(), vec![1.0, 2.0, 3.0]); // Very old timestamps
            lock.insert("10.0.0.2".to_string(), vec![]); // Already empty
        }

        // Check for another IP; this triggers pruning across the entire map
        assert!(state.check_login_rate_limit("10.0.0.3").await);

        let lock = state.login_attempts.lock().await;
        // Expired entry and empty entry should have been pruned and removed completely
        assert!(!lock.contains_key("10.0.0.1"));
        assert!(!lock.contains_key("10.0.0.2"));
        assert!(lock.contains_key("10.0.0.3"));
    }
}
