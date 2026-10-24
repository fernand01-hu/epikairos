use base64::engine::general_purpose::URL_SAFE;
use base64::Engine;
use rand::distributions::Alphanumeric;
use rand::Rng;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::models::Course;
use crate::state::secret_path;

pub type FeedDbPool = Pool<SqliteConnectionManager>;

const SECRET_FILE: &str = ".server_secret";

pub fn get_server_key() -> [u8; 32] {
    if let Ok(env_secret) = std::env::var("SERVER_SECRET_KEY") {
        let mut hasher = Sha256::new();
        hasher.update(env_secret.as_bytes());
        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);
        return key;
    }

    let secret_file = secret_path(SECRET_FILE);
    if secret_file.exists() {
        if let Ok(mut f) = File::open(&secret_file) {
            let mut key = [0u8; 32];
            if f.read_exact(&mut key).is_ok() {
                return key;
            }
        }
    }

    let mut key = [0u8; 32];
    rand::thread_rng().fill(&mut key);

    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);

    if let Ok(mut f) = opts.open(&secret_file) {
        let _ = f.write_all(&key);
    }

    key
}

pub fn encrypt_secret(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let key = get_server_key();
    let b64_key = URL_SAFE.encode(key);
    match fernet::Fernet::new(&b64_key) {
        Some(f) => f.encrypt(text.as_bytes()),
        None => String::new(),
    }
}

pub fn decrypt_secret(encrypted_text: &str) -> String {
    if encrypted_text.is_empty() {
        return String::new();
    }
    let key = get_server_key();
    let b64_key = URL_SAFE.encode(key);
    if let Some(f) = fernet::Fernet::new(&b64_key) {
        if let Ok(bytes) = f.decrypt(encrypted_text) {
            if let Ok(s) = String::from_utf8(bytes) {
                return s;
            }
        }
    }
    String::new()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedConfig {
    pub feed_id: String,
    pub username: String,
    pub password: Option<String>,
    pub include_regex: Option<String>,
    pub exclude_regex: Option<String>,
    pub allowed_types: Option<Vec<String>>,
    pub allowed_courses: Option<Vec<String>>,
    pub hide_cancelled: bool,
    pub calendar_name: String,
    pub last_cached_ics: String,
    pub last_cached_at: f64,
    pub created_at: f64,
    pub cached_courses: Vec<Course>,
}

pub fn init_db(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS feeds (
            feed_id TEXT PRIMARY KEY,
            username TEXT NOT NULL,
            encrypted_password TEXT NOT NULL,
            include_regex TEXT,
            exclude_regex TEXT,
            allowed_types TEXT,
            hide_cancelled INTEGER DEFAULT 0,
            calendar_name TEXT,
            last_cached_ics TEXT,
            last_cached_at REAL DEFAULT 0,
            created_at REAL NOT NULL,
            allowed_courses TEXT,
            cached_courses_json TEXT
        )",
        [],
    )?;

    // Check dynamic columns for backward compatibility
    let mut stmt = conn.prepare("PRAGMA table_info(feeds)")?;
    let cols: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();

    if !cols.contains(&"allowed_courses".to_string()) {
        conn.execute("ALTER TABLE feeds ADD COLUMN allowed_courses TEXT;", [])?;
    }
    if !cols.contains(&"cached_courses_json".to_string()) {
        conn.execute("ALTER TABLE feeds ADD COLUMN cached_courses_json TEXT;", [])?;
    }

    Ok(())
}

pub fn create_feed(
    conn: &rusqlite::Connection,
    username: &str,
    password: &str,
    include_regex: Option<&str>,
    exclude_regex: Option<&str>,
    allowed_types: Option<&[String]>,
    allowed_courses: Option<&[String]>,
    hide_cancelled: bool,
    calendar_name: &str,
    initial_ics: Option<&str>,
    cached_courses: Option<&[Course]>,
) -> rusqlite::Result<String> {
    init_db(conn)?;

    let feed_id: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(20)
        .map(char::from)
        .collect();

    let enc_pw = if !password.is_empty() {
        encrypt_secret(password)
    } else {
        String::new()
    };

    let types_str = allowed_types.map(|t| t.join(",")).unwrap_or_default();

    let courses_str = match allowed_courses {
        None => String::new(),
        Some(slice) if slice.is_empty() => "__EMPTY__".to_string(),
        Some(slice) => slice.join("\t"),
    };

    let courses_json = cached_courses
        .map(|c| serde_json::to_string(c).unwrap_or_default())
        .unwrap_or_default();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let last_cached_at = if initial_ics.is_some() { now } else { 0.0 };

    conn.execute(
        "INSERT INTO feeds (
            feed_id, username, encrypted_password,
            include_regex, exclude_regex, allowed_types,
            hide_cancelled, calendar_name, last_cached_ics,
            last_cached_at, created_at, allowed_courses,
            cached_courses_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            feed_id,
            username,
            enc_pw,
            include_regex.unwrap_or(""),
            exclude_regex.unwrap_or(""),
            types_str,
            if hide_cancelled { 1 } else { 0 },
            calendar_name,
            initial_ics.unwrap_or(""),
            last_cached_at,
            now,
            courses_str,
            courses_json,
        ],
    )?;

    Ok(feed_id)
}

pub fn get_feed(conn: &rusqlite::Connection, feed_id: &str) -> rusqlite::Result<Option<FeedConfig>> {
    init_db(conn)?;

    let mut stmt = conn.prepare(
        "SELECT feed_id, username, encrypted_password,
                include_regex, exclude_regex, allowed_types,
                hide_cancelled, calendar_name, last_cached_ics,
                last_cached_at, created_at, allowed_courses,
                cached_courses_json
         FROM feeds WHERE feed_id = ?1",
    )?;

    let mut rows = stmt.query(params![feed_id])?;
    if let Some(row) = rows.next()? {
        let feed_id: String = row.get(0)?;
        let username: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
        let enc_pw: String = row.get::<_, Option<String>>(2)?.unwrap_or_default();
        let inc_re: String = row.get::<_, Option<String>>(3)?.unwrap_or_default();
        let exc_re: String = row.get::<_, Option<String>>(4)?.unwrap_or_default();
        let types_s: String = row.get::<_, Option<String>>(5)?.unwrap_or_default();
        let hide_canc: i64 = row.get::<_, Option<i64>>(6)?.unwrap_or(0);
        let cal_name: String = row.get::<_, Option<String>>(7)?.unwrap_or_default();
        let last_ics: String = row.get::<_, Option<String>>(8)?.unwrap_or_default();
        let last_at: f64 = row.get::<_, Option<f64>>(9)?.unwrap_or(0.0);
        let created: f64 = row.get::<_, Option<f64>>(10)?.unwrap_or(0.0);
        let courses_col: Option<String> = row.get(11)?;
        let courses_json: Option<String> = row.get(12)?;

        let password = if !enc_pw.is_empty() {
            let dec = decrypt_secret(&enc_pw);
            if !dec.is_empty() {
                Some(dec)
            } else {
                None
            }
        } else {
            None
        };

        let allowed_types = if !types_s.trim().is_empty() {
            Some(
                types_s
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            )
        } else {
            None
        };

        let allowed_courses = match courses_col.as_deref() {
            Some("__EMPTY__") => Some(Vec::new()),
            Some(s) if !s.trim().is_empty() => Some(
                s.split('\t')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            ),
            _ => None,
        };

        let cached_courses: Vec<Course> = courses_json
            .as_deref()
            .and_then(|json_str| serde_json::from_str(json_str).ok())
            .unwrap_or_default();

        Ok(Some(FeedConfig {
            feed_id,
            username,
            password,
            include_regex: if !inc_re.is_empty() { Some(inc_re) } else { None },
            exclude_regex: if !exc_re.is_empty() { Some(exc_re) } else { None },
            allowed_types,
            allowed_courses,
            hide_cancelled: hide_canc != 0,
            calendar_name: if !cal_name.is_empty() {
                cal_name
            } else {
                "EpiKairos - EPITA Emploi du temps".to_string()
            },
            last_cached_ics: last_ics,
            last_cached_at: last_at,
            created_at: created,
            cached_courses,
        }))
    } else {
        Ok(None)
    }
}

pub fn update_feed_cache(
    conn: &rusqlite::Connection,
    feed_id: &str,
    ics_content: &str,
) -> rusqlite::Result<()> {
    init_db(conn)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    conn.execute(
        "UPDATE feeds SET last_cached_ics = ?1, last_cached_at = ?2 WHERE feed_id = ?3",
        params![ics_content, now, feed_id],
    )?;
    Ok(())
}

pub fn update_feed_courses(
    conn: &rusqlite::Connection,
    feed_id: &str,
    courses: &[Course],
) -> rusqlite::Result<()> {
    init_db(conn)?;
    let json_str = serde_json::to_string(courses).unwrap_or_default();
    conn.execute(
        "UPDATE feeds SET cached_courses_json = ?1 WHERE feed_id = ?2",
        params![json_str, feed_id],
    )?;
    Ok(())
}

pub fn delete_feed(conn: &rusqlite::Connection, feed_id: &str) -> rusqlite::Result<bool> {
    init_db(conn)?;
    let count = conn.execute("DELETE FROM feeds WHERE feed_id = ?1", params![feed_id])?;
    Ok(count > 0)
}
