use minijinja::Environment;
use rand::RngCore;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::db::DbPool;
use crate::feed::FeedDbPool;

const FLASK_SECRET_FILE: &str = ".flask_secret";

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
        let secret_key = get_or_create_flask_secret();
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
        let attempts = lock.entry(client_ip.to_string()).or_default();
        attempts.retain(|&t| now - t < RATE_LIMIT_WINDOW);

        if attempts.len() >= MAX_LOGIN_ATTEMPTS {
            return false;
        }

        attempts.push(now);
        true
    }
}

pub fn get_or_create_flask_secret() -> String {
    if let Ok(env_key) = std::env::var("FLASK_SECRET_KEY") {
        if !env_key.trim().is_empty() && env_key != "auriga-epita-secret-key-change-in-prod" {
            return env_key;
        }
    }

    if Path::new(FLASK_SECRET_FILE).exists() {
        if let Ok(mut f) = File::open(FLASK_SECRET_FILE) {
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

    if let Ok(mut f) = opts.open(FLASK_SECRET_FILE) {
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
