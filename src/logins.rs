//! What each login brings to the shared database, and cleanup of courses that
//! Auriga deleted.
//!
//! At every login, the courses Auriga returns are saved (new / updated /
//! unchanged), a line is added to `login_events`, and `login_snapshots` keeps the
//! list of that login's courses. At the next login of the same account, courses
//! that were in the previous list but are now missing were deleted on Auriga
//! (often re-created elsewhere with a new id), so they are removed from the shared
//! database, as long as they are still in the future, inside both logins' date
//! ranges, and the answer does not look partial. Only this account's own previous
//! courses can be removed, so one login never touches another student's courses.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::accounts::{is_valid_username, normalize_username};
use crate::db::{ingest_courses_report, paris_date_time, CourseChange};
use crate::db::models::Course;

/// Login history kept per account
pub const EVENTS_KEPT: i64 = 50;

pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS login_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL,
            at REAL NOT NULL,
            consent INTEGER NOT NULL,
            range_start TEXT,
            range_end TEXT,
            received INTEGER NOT NULL DEFAULT 0,
            added INTEGER NOT NULL DEFAULT 0,
            updated INTEGER NOT NULL DEFAULT 0,
            unchanged INTEGER NOT NULL DEFAULT 0,
            removed INTEGER NOT NULL DEFAULT 0,
            removal_skipped TEXT,
            groups TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_login_events_user ON login_events(username, at);
         CREATE TABLE IF NOT EXISTS login_snapshots (
            username TEXT PRIMARY KEY,
            taken_at REAL NOT NULL,
            range_start TEXT,
            range_end TEXT,
            courses_json TEXT NOT NULL
         );",
    )
}

/// One course of a login, as kept in the snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotCourse {
    pub id: String,
    pub name: String,
    pub start: String,
    pub end: String,
    /// Paris date, YYYY-MM-DD
    pub date: String,
    pub groups: Vec<String>,
    /// "added" | "updated" | "unchanged"
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LoginEvent {
    pub at: f64,
    pub consent: bool,
    pub range_start: Option<String>,
    pub range_end: Option<String>,
    pub received: i64,
    pub added: i64,
    pub updated: i64,
    pub unchanged: i64,
    pub removed: i64,
    /// Why removals were skipped ("partial": too many vanished at once)
    pub removal_skipped: Option<String>,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub taken_at: f64,
    pub range_start: Option<String>,
    pub range_end: Option<String>,
    pub courses: Vec<SnapshotCourse>,
}

/// What a login did.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LoginReport {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: Vec<String>,
    pub removal_skipped: Option<&'static str>,
}

pub struct LoginInput<'a> {
    /// None when Auriga did not tell who logged in: courses are still saved,
    /// but nothing is recorded and nothing is removed.
    pub username: Option<&'a str>,
    pub consent: bool,
    pub range_start: &'a str,
    pub range_end: &'a str,
    pub courses: &'a [Course],
    /// Unix seconds; courses that started before are never removed
    pub now: f64,
}

/// Unix seconds of a course timestamp (UTC "…Z", or Paris local without offset).
fn course_time(ts: &str) -> Option<f64> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        return Some(dt.timestamp() as f64);
    }
    let naive = chrono::NaiveDateTime::parse_from_str(ts.get(..19)?, "%Y-%m-%dT%H:%M:%S").ok()?;
    use chrono::TimeZone;
    chrono_tz::Europe::Paris
        .from_local_datetime(&naive)
        .earliest()
        .map(|d| d.timestamp() as f64)
}

fn load_snapshot(conn: &Connection, user: &str) -> rusqlite::Result<Option<Snapshot>> {
    let row = conn.query_row(
        "SELECT taken_at, range_start, range_end, courses_json FROM login_snapshots WHERE username = ?1",
        params![user],
        |r| Ok((r.get::<_, f64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?)),
    );
    match row {
        Ok((taken_at, range_start, range_end, json)) => Ok(Some(Snapshot {
            taken_at,
            range_start,
            range_end,
            courses: serde_json::from_str(&json).unwrap_or_default(),
        })),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Courses of the previous login that Auriga no longer returns and that may be removed.
/// Returns (ids, skipped reason).
fn vanished_courses(
    previous: &Snapshot,
    fresh_ids: &HashSet<&str>,
    range_start: &str,
    range_end: &str,
    now: f64,
) -> (Vec<String>, Option<&'static str>) {
    // Only the days both logins asked Auriga about
    let from = previous.range_start.as_deref().unwrap_or("").max(range_start);
    let to = previous.range_end.as_deref().unwrap_or("9999-12-31").min(range_end);
    let comparable: Vec<&SnapshotCourse> = previous
        .courses
        .iter()
        .filter(|c| c.date.as_str() >= from && c.date.as_str() <= to)
        .filter(|c| course_time(&c.start).is_some_and(|t| t > now))
        .collect();
    let gone: Vec<String> = comparable
        .iter()
        .filter(|c| !fresh_ids.contains(c.id.as_str()))
        .map(|c| c.id.clone())
        .collect();
    // More than half of the future courses vanishing at once looks like a partial answer
    if !gone.is_empty() && gone.len() * 2 > comparable.len() {
        return (Vec::new(), Some("partial"));
    }
    (gone, None)
}

/// Deletes courses (and their room / group / teacher links) from the shared database.
fn delete_courses(conn: &Connection, ids: &[String]) -> rusqlite::Result<usize> {
    let mut n = 0;
    for id in ids {
        for table in ["course_rooms", "course_groups", "course_teachers"] {
            conn.execute(&format!("DELETE FROM {table} WHERE course_id = ?1"), params![id])?;
        }
        n += conn.execute("DELETE FROM courses WHERE id = ?1", params![id])?;
    }
    Ok(n)
}

/// Saves a login's courses, removes the ones Auriga deleted since this account's
/// previous login, and records the login. `feeds` gets the removed courses
/// dropped from every subscription link's saved copy.
pub fn process_login(
    school: &mut Connection,
    feeds: Option<&Connection>,
    input: &LoginInput,
) -> rusqlite::Result<LoginReport> {
    let mut report = LoginReport::default();
    let user = input
        .username
        .map(normalize_username)
        .filter(|u| is_valid_username(u));

    // 1. Save into the shared database (only with the student's consent)
    let statuses: HashMap<String, CourseChange> = if input.consent {
        ingest_courses_report(school, input.courses)?.into_iter().collect()
    } else {
        HashMap::new()
    };
    for change in statuses.values() {
        match change {
            CourseChange::Added => report.added += 1,
            CourseChange::Updated => report.updated += 1,
            CourseChange::Unchanged => report.unchanged += 1,
        }
    }

    let Some(user) = user else { return Ok(report) };

    // 2. Courses this account had last time and Auriga no longer returns
    if input.consent && !input.courses.is_empty() {
        if let Some(previous) = load_snapshot(school, &user)? {
            let fresh_ids: HashSet<&str> = input.courses.iter().map(|c| c.id.trim()).collect();
            let (gone, skipped) = vanished_courses(&previous, &fresh_ids, input.range_start, input.range_end, input.now);
            report.removal_skipped = skipped;
            if !gone.is_empty() {
                delete_courses(school, &gone)?;
                if let Some(feeds) = feeds {
                    crate::feed::remove_courses_from_feeds(feeds, &gone)?;
                }
                report.removed = gone;
            }
        }
    } else if input.consent {
        // Auriga returned nothing: keep everything, it is not a list of deletions
        report.removal_skipped = Some("empty");
    }

    // 3. Record the login, and this login's courses for the next comparison
    let mut groups: Vec<String> = input.courses.iter().flat_map(|c| c.groups.iter().map(|g| g.trim().to_string())).filter(|g| !g.is_empty()).collect();
    groups.sort();
    groups.dedup();
    school.execute(
        "INSERT INTO login_events (username, at, consent, range_start, range_end, received, added, updated,
                                   unchanged, removed, removal_skipped, groups)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            user,
            input.now,
            input.consent as i64,
            input.range_start,
            input.range_end,
            if input.consent { input.courses.len() as i64 } else { 0 },
            report.added as i64,
            report.updated as i64,
            report.unchanged as i64,
            report.removed.len() as i64,
            report.removal_skipped,
            if input.consent { serde_json::to_string(&groups).unwrap_or_default() } else { "[]".to_string() },
        ],
    )?;
    school.execute(
        "DELETE FROM login_events WHERE username = ?1 AND id NOT IN (
            SELECT id FROM login_events WHERE username = ?1 ORDER BY at DESC, id DESC LIMIT ?2)",
        params![user, EVENTS_KEPT],
    )?;

    if input.consent && !input.courses.is_empty() {
        let snapshot: Vec<SnapshotCourse> = input
            .courses
            .iter()
            .filter(|c| !c.id.trim().is_empty())
            .map(|c| SnapshotCourse {
                id: c.id.trim().to_string(),
                name: c.course_name.clone(),
                start: c.start.clone(),
                end: c.end.clone(),
                date: paris_date_time(c.start.trim(), "00:00").0,
                groups: c.groups.clone(),
                status: statuses.get(c.id.trim()).copied().unwrap_or(CourseChange::Unchanged).as_str().to_string(),
            })
            .collect();
        school.execute(
            "INSERT INTO login_snapshots (username, taken_at, range_start, range_end, courses_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(username) DO UPDATE SET taken_at = excluded.taken_at, range_start = excluded.range_start,
                range_end = excluded.range_end, courses_json = excluded.courses_json",
            params![user, input.now, input.range_start, input.range_end, serde_json::to_string(&snapshot).unwrap_or_default()],
        )?;
    } else if !input.consent {
        // Contribution turned off: keep no course list for this account
        school.execute("DELETE FROM login_snapshots WHERE username = ?1", params![user])?;
    }

    Ok(report)
}

/// The account's logins, newest first.
pub fn events_for(conn: &Connection, username: &str) -> rusqlite::Result<Vec<LoginEvent>> {
    let user = normalize_username(username);
    let mut stmt = conn.prepare(
        "SELECT at, consent, range_start, range_end, received, added, updated, unchanged, removed,
                removal_skipped, groups
         FROM login_events WHERE username = ?1 ORDER BY at DESC, id DESC",
    )?;
    let rows = stmt.query_map(params![user], |r| {
        let groups: Option<String> = r.get(10)?;
        Ok(LoginEvent {
            at: r.get(0)?,
            consent: r.get::<_, i64>(1)? != 0,
            range_start: r.get(2)?,
            range_end: r.get(3)?,
            received: r.get(4)?,
            added: r.get(5)?,
            updated: r.get(6)?,
            unchanged: r.get(7)?,
            removed: r.get(8)?,
            removal_skipped: r.get(9)?,
            groups: groups.and_then(|g| serde_json::from_str(&g).ok()).unwrap_or_default(),
        })
    })?;
    rows.collect()
}

/// The courses of the account's last login (with consent), if any.
pub fn snapshot_for(conn: &Connection, username: &str) -> rusqlite::Result<Option<Snapshot>> {
    load_snapshot(conn, &normalize_username(username))
}

/// Forgets the account's history and course list ("delete my data").
pub fn delete_for(conn: &Connection, username: &str) -> rusqlite::Result<()> {
    let user = normalize_username(username);
    conn.execute("DELETE FROM login_events WHERE username = ?1", params![user])?;
    conn.execute("DELETE FROM login_snapshots WHERE username = ?1", params![user])?;
    Ok(())
}
