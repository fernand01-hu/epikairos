//! Subscription links (webcal / .ics). A link stores its owner, filters and a copy
//! of the owner's courses; it is rebuilt on every request with the latest version
//! of the class's courses from the shared database. No password is ever stored.

use rand::distributions::Alphanumeric;
use rand::Rng;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::models::Course;

pub type FeedDbPool = Pool<SqliteConnectionManager>;

/// Subscription links one account may own.
pub const MAX_FEEDS_PER_USER: i64 = 3;

fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedConfig {
    pub feed_id: String,
    pub username: String,
    pub include_regex: Option<String>,
    pub exclude_regex: Option<String>,
    pub allowed_types: Option<Vec<String>>,
    pub allowed_courses: Option<Vec<String>>,
    pub hide_cancelled: bool,
    pub calendar_name: String,
    pub created_at: f64,
    pub cached_courses: Vec<Course>,
}

const FEEDS_TABLE: &str = "CREATE TABLE IF NOT EXISTS feeds (
    feed_id TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    include_regex TEXT,
    exclude_regex TEXT,
    allowed_types TEXT,
    allowed_courses TEXT,
    hide_cancelled INTEGER DEFAULT 0,
    calendar_name TEXT,
    cached_courses_json TEXT,
    created_at REAL NOT NULL,
    last_used_at REAL DEFAULT 0
)";

/// Creates the table. Older versions stored Auriga passwords and a calendar cache:
/// those links are deleted and the table is rebuilt without these columns.
/// Returns how many password links were deleted.
pub fn init_db(conn: &rusqlite::Connection) -> rusqlite::Result<usize> {
    conn.execute_batch(FEEDS_TABLE)?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(feeds)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();
    if !cols.iter().any(|c| c == "encrypted_password") {
        return Ok(0);
    }
    let has = |c: &str| cols.iter().any(|x| x == c);
    let col = |c: &str| if has(c) { c.to_string() } else { "NULL".to_string() };
    let tx = conn.unchecked_transaction()?;
    let deleted = tx.execute("DELETE FROM feeds WHERE COALESCE(encrypted_password, '') != ''", [])?;
    tx.execute_batch(&format!(
        "{create};
         INSERT INTO feeds_new (feed_id, username, include_regex, exclude_regex, allowed_types,
                                allowed_courses, hide_cancelled, calendar_name, cached_courses_json,
                                created_at, last_used_at)
             SELECT feed_id, username, include_regex, exclude_regex, allowed_types, {allowed},
                    hide_cancelled, calendar_name, {courses}, created_at, COALESCE({used}, 0)
             FROM feeds;
         DROP TABLE feeds;
         ALTER TABLE feeds_new RENAME TO feeds;",
        create = FEEDS_TABLE.replace("IF NOT EXISTS feeds", "feeds_new"),
        allowed = col("allowed_courses"),
        courses = col("cached_courses_json"),
        used = col("last_cached_at"),
    ))?;
    tx.commit()?;
    Ok(deleted)
}

fn encode_allowed_courses(allowed: Option<&[String]>) -> String {
    match allowed {
        None => String::new(),
        Some([]) => "__EMPTY__".to_string(),
        Some(list) => list.join("\t"),
    }
}

#[allow(clippy::too_many_arguments)] // one column per argument, called from one place
pub fn create_feed(
    conn: &rusqlite::Connection,
    username: &str,
    include_regex: Option<&str>,
    exclude_regex: Option<&str>,
    allowed_types: Option<&[String]>,
    allowed_courses: Option<&[String]>,
    hide_cancelled: bool,
    calendar_name: &str,
    courses: &[Course],
) -> rusqlite::Result<String> {
    let feed_id: String = rand::thread_rng().sample_iter(&Alphanumeric).take(20).map(char::from).collect();
    conn.execute(
        "INSERT INTO feeds (feed_id, username, include_regex, exclude_regex, allowed_types,
                            allowed_courses, hide_cancelled, calendar_name, cached_courses_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            feed_id,
            username.trim(),
            include_regex.unwrap_or(""),
            exclude_regex.unwrap_or(""),
            allowed_types.map(|t| t.join(",")).unwrap_or_default(),
            encode_allowed_courses(allowed_courses),
            hide_cancelled as i64,
            calendar_name,
            serde_json::to_string(courses).unwrap_or_default(),
            now_secs(),
        ],
    )?;
    Ok(feed_id)
}

pub fn get_feed(conn: &rusqlite::Connection, feed_id: &str) -> rusqlite::Result<Option<FeedConfig>> {
    let mut stmt = conn.prepare(
        "SELECT feed_id, username, include_regex, exclude_regex, allowed_types, allowed_courses,
                hide_cancelled, calendar_name, cached_courses_json, created_at
         FROM feeds WHERE feed_id = ?1",
    )?;
    let mut rows = stmt.query(params![feed_id])?;
    let Some(row) = rows.next()? else { return Ok(None) };
    let text = |i: usize| -> rusqlite::Result<String> { Ok(row.get::<_, Option<String>>(i)?.unwrap_or_default()) };
    let non_empty = |s: String| if s.trim().is_empty() { None } else { Some(s) };
    let types = text(4)?;
    let allowed = text(5)?;
    let calendar_name = text(7)?;
    Ok(Some(FeedConfig {
        feed_id: row.get(0)?,
        username: text(1)?,
        include_regex: non_empty(text(2)?),
        exclude_regex: non_empty(text(3)?),
        allowed_types: non_empty(types).map(|t| t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()),
        allowed_courses: match allowed.as_str() {
            "__EMPTY__" => Some(Vec::new()),
            s if !s.trim().is_empty() => Some(s.split('\t').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()),
            _ => None,
        },
        hide_cancelled: row.get::<_, Option<i64>>(6)?.unwrap_or(0) != 0,
        calendar_name: non_empty(calendar_name).unwrap_or_else(|| "EpiKairos - EPITA Emploi du temps".to_string()),
        cached_courses: serde_json::from_str(&text(8)?).unwrap_or_default(),
        created_at: row.get::<_, Option<f64>>(9)?.unwrap_or(0.0),
    }))
}

/// Remembers when a calendar app last fetched the link (shown in the panel).
pub fn mark_feed_used(conn: &rusqlite::Connection, feed_id: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE feeds SET last_used_at = ?1 WHERE feed_id = ?2", params![now_secs(), feed_id])?;
    Ok(())
}

/// Number of subscription links an account already owns.
pub fn count_feeds_for(conn: &rusqlite::Connection, username: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM feeds WHERE {OWNER_MATCH}"),
        params![login_key(username)],
        |r| r.get(0),
    )
}

/// Gives every link of `username` the courses just downloaded at login.
pub fn update_feeds_for_user(conn: &rusqlite::Connection, username: &str, courses: &[Course]) -> rusqlite::Result<usize> {
    let key = login_key(username);
    if key.is_empty() || courses.is_empty() {
        return Ok(0);
    }
    conn.execute(
        &format!("UPDATE feeds SET cached_courses_json = ?2 WHERE {OWNER_MATCH}"),
        params![key, serde_json::to_string(courses).unwrap_or_default()],
    )
}

pub fn student_groups(personal: &[Course], known_groups: &std::collections::HashMap<String, Vec<String>>) -> Vec<String> {
    let mut groups: Vec<String> = personal
        .iter()
        .flat_map(|c| c.groups.iter().cloned())
        .chain(personal.iter().filter_map(|c| known_groups.get(&c.id)).flatten().cloned())
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty())
        .collect();
    groups.sort();
    groups.dedup();
    groups
}

/// Overlays the shared database's version of the student's class courses on their
/// own list: classmates' logins bring new courses, time and room changes and
/// cancellations; courses only the student has stay as they were.
pub fn merge_shared_courses(personal: Vec<Course>, shared: Vec<Course>) -> Vec<Course> {
    let mut by_id: std::collections::HashMap<String, Course> =
        personal.into_iter().map(|c| (c.id.clone(), c)).collect();
    for c in shared {
        by_id.insert(c.id.clone(), c);
    }
    let mut merged: Vec<Course> = by_id.into_values().collect();
    merged.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.id.cmp(&b.id)));
    merged
}

/// SQL condition matching an account's links: same login without case, with or
/// without "@domain" (links made with a password kept the typed name).
const OWNER_MATCH: &str = "(lower(username) = ?1 OR lower(username) LIKE ?1 || '@%')";

fn login_key(username: &str) -> String {
    username.trim().split('@').next().unwrap_or("").to_lowercase()
}

/// What the "Lien d'abonnement" panel shows about one link. No secrets.
#[derive(Debug, Clone, Serialize)]
pub struct FeedSummary {
    pub feed_id: String,
    pub created_at: f64,
    pub last_used_at: f64,
    pub hide_cancelled: bool,
    pub include_regex: Option<String>,
    pub exclude_regex: Option<String>,
    /// Number of subjects kept, None when every subject is included
    pub subjects: Option<usize>,
}

/// The account's links, newest first.
pub fn list_feeds_for(conn: &rusqlite::Connection, username: &str) -> rusqlite::Result<Vec<FeedSummary>> {
    let key = login_key(username);
    if key.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT feed_id, created_at, COALESCE(last_used_at, 0), COALESCE(hide_cancelled, 0),
                include_regex, exclude_regex, allowed_courses
         FROM feeds WHERE {OWNER_MATCH} ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map(params![key], |r| {
            let allowed: Option<String> = r.get(6)?;
            let non_empty = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
            Ok(FeedSummary {
                feed_id: r.get(0)?,
                created_at: r.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                last_used_at: r.get(2)?,
                hide_cancelled: r.get::<_, i64>(3)? != 0,
                include_regex: non_empty(r.get(4)?),
                exclude_regex: non_empty(r.get(5)?),
                subjects: match allowed.as_deref() {
                    None | Some("") => None,
                    Some("__EMPTY__") => Some(0),
                    Some(list) => Some(list.split('\t').filter(|s| !s.trim().is_empty()).count()),
                },
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// Deletes every link of the account (both "login" and "login@epita.fr" forms).
pub fn delete_feeds_for(conn: &rusqlite::Connection, username: &str) -> rusqlite::Result<usize> {
    let key = login_key(username);
    if key.is_empty() {
        return Ok(0);
    }
    conn.execute(&format!("DELETE FROM feeds WHERE {OWNER_MATCH}"), params![key])
}

/// True when the link belongs to that account.
pub fn feed_owned_by(conn: &rusqlite::Connection, feed_id: &str, username: &str) -> rusqlite::Result<bool> {
    let key = login_key(username);
    if key.is_empty() {
        return Ok(false);
    }
    let n: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM feeds WHERE {OWNER_MATCH} AND feed_id = ?2"),
        params![key, feed_id],
        |r| r.get(0),
    )?;
    Ok(n == 1)
}

/// Replaces a link's courses and filters, keeping its address.
pub fn update_feed_settings(
    conn: &rusqlite::Connection,
    feed_id: &str,
    allowed_courses: Option<&[String]>,
    hide_cancelled: bool,
    courses: &[Course],
) -> rusqlite::Result<()> {
    let allowed = encode_allowed_courses(allowed_courses);
    conn.execute(
        "UPDATE feeds SET allowed_courses = ?1, hide_cancelled = ?2, cached_courses_json = ?3,
                include_regex = NULL, exclude_regex = NULL
         WHERE feed_id = ?4",
        params![allowed, hide_cancelled as i64, serde_json::to_string(courses).unwrap_or_default(), feed_id],
    )?;
    Ok(())
}

pub fn delete_feed(conn: &rusqlite::Connection, feed_id: &str) -> rusqlite::Result<bool> {
    Ok(conn.execute("DELETE FROM feeds WHERE feed_id = ?1", params![feed_id])? > 0)
}
