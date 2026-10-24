//! Users, roles and community calendars (clubs, associations, BDE...).
//!
//! Every Auriga login is recorded in `users` so the admin can pick people from the
//! database and promote them to moderator. Moderators create calendars and events
//! that every logged-in student can display in Old Zeus.

use chrono::{NaiveDateTime, TimeZone};
use chrono_tz::Europe::Paris;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::DbPool;

pub const MAX_CALENDARS_PER_OWNER: i64 = 20;
pub const MAX_EVENTS_PER_CALENDAR: i64 = 500;
pub const MAX_EVENT_RANGE_DAYS: i64 = 62;
const LOCAL_DATETIME_FORMAT: &str = "%Y-%m-%dT%H:%M";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Moderator,
    Admin,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Moderator => "moderator",
            Role::Admin => "admin",
        }
    }

    pub fn can_moderate(&self) -> bool {
        matches!(self, Role::Moderator | Role::Admin)
    }

    /// Admin edits everything; a moderator only edits calendars they own.
    pub fn can_edit_calendar(&self, username: &str, owner: &str) -> bool {
        match self {
            Role::Admin => true,
            Role::Moderator => normalize_username(username) == owner,
            Role::User => false,
        }
    }
}

/// `Prenom.Nom@epita.fr` and `prenom.nom` are the same account.
pub fn normalize_username(raw: &str) -> String {
    raw.trim()
        .split('@')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase()
}

/// Admins come from configuration, never from the database, so they cannot be demoted.
/// `ADMIN_USERS` is a comma-separated list; it defaults to the project owner.
pub fn admin_usernames() -> Vec<String> {
    std::env::var("ADMIN_USERS")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "fernand.huttois".to_string())
        .split(',')
        .map(normalize_username)
        .filter(|u| !u.is_empty())
        .collect()
}

pub fn is_admin(username: &str) -> bool {
    let user = normalize_username(username);
    !user.is_empty() && admin_usernames().contains(&user)
}

pub fn is_valid_username(username: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^[a-z0-9][a-z0-9._-]{0,63}$").unwrap());
    re.is_match(username)
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS users (
            username TEXT PRIMARY KEY,
            role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'moderator')),
            first_seen REAL,
            last_seen REAL
        );
        CREATE TABLE IF NOT EXISTS community_calendars (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            color TEXT NOT NULL,
            owner TEXT NOT NULL,
            created_at REAL NOT NULL
        );
        CREATE TABLE IF NOT EXISTS community_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            calendar_id INTEGER NOT NULL REFERENCES community_calendars(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            start TEXT NOT NULL,
            end TEXT NOT NULL,
            location TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            created_by TEXT NOT NULL,
            created_at REAL NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);
        CREATE INDEX IF NOT EXISTS idx_calendars_owner ON community_calendars(owner);
        CREATE INDEX IF NOT EXISTS idx_events_calendar_start ON community_events(calendar_id, start);",
    )
}

/// Looks up a role without blocking the async runtime. Any database error falls back to `User`.
pub async fn lookup_role(pool: &DbPool, username: &str) -> Role {
    if is_admin(username) {
        return Role::Admin;
    }
    let pool = pool.clone();
    let user = normalize_username(username);
    tokio::task::spawn_blocking(move || {
        let conn = pool.get().ok()?;
        get_role(&conn, &user).ok()
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(Role::User)
}

// -----------------------------------------------------------------------------
// Users & roles
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct UserRow {
    pub username: String,
    pub role: Role,
    pub first_seen: Option<f64>,
    pub last_seen: Option<f64>,
    pub calendars: i64,
}

pub fn record_login(conn: &Connection, username: &str) -> rusqlite::Result<()> {
    let user = normalize_username(username);
    if !is_valid_username(&user) {
        return Ok(());
    }
    let now = now_secs();
    conn.execute(
        "INSERT INTO users (username, role, first_seen, last_seen) VALUES (?1, 'user', ?2, ?2)
         ON CONFLICT(username) DO UPDATE SET
            last_seen = excluded.last_seen,
            first_seen = COALESCE(users.first_seen, excluded.first_seen)",
        params![user, now],
    )?;
    Ok(())
}

pub fn get_role(conn: &Connection, username: &str) -> rusqlite::Result<Role> {
    if is_admin(username) {
        return Ok(Role::Admin);
    }
    let role: Option<String> = conn
        .query_row(
            "SELECT role FROM users WHERE username = ?1",
            params![normalize_username(username)],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match role.as_deref() {
        Some("moderator") => Role::Moderator,
        _ => Role::User,
    })
}

pub fn list_users(conn: &Connection, query: Option<&str>, limit: i64) -> rusqlite::Result<Vec<UserRow>> {
    let pattern = format!("%{}%", normalize_username(query.unwrap_or("")));
    let mut stmt = conn.prepare(
        "SELECT u.username, u.role, u.first_seen, u.last_seen,
                (SELECT COUNT(*) FROM community_calendars c WHERE c.owner = u.username)
         FROM users u
         WHERE u.username LIKE ?1
         ORDER BY (u.role = 'moderator') DESC, u.last_seen IS NULL, u.last_seen DESC, u.username
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![pattern, limit], |r| {
        let username: String = r.get(0)?;
        let role: String = r.get(1)?;
        Ok(UserRow {
            role: if is_admin(&username) {
                Role::Admin
            } else if role == "moderator" {
                Role::Moderator
            } else {
                Role::User
            },
            username,
            first_seen: r.get(2)?,
            last_seen: r.get(3)?,
            calendars: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// Grants or removes the moderator role. Accepts usernames that never logged in yet,
/// so the admin can prepare a role before the person's first visit.
pub fn set_role(conn: &Connection, username: &str, role: Role) -> Result<(), String> {
    let user = normalize_username(username);
    if !is_valid_username(&user) {
        return Err("Identifiant invalide (format attendu : prenom.nom).".to_string());
    }
    if is_admin(&user) {
        return Err("Ce compte est administrateur, son rôle ne peut pas être modifié.".to_string());
    }
    let role_str = match role {
        Role::Moderator => "moderator",
        Role::User => "user",
        Role::Admin => return Err("Le rôle administrateur se configure côté serveur (ADMIN_USERS).".to_string()),
    };
    conn.execute(
        "INSERT INTO users (username, role) VALUES (?1, ?2)
         ON CONFLICT(username) DO UPDATE SET role = excluded.role",
        params![user, role_str],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// -----------------------------------------------------------------------------
// Calendars
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct CalendarRow {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub color: String,
    pub owner: String,
    pub created_at: f64,
    pub event_count: i64,
}

#[derive(Debug, Clone)]
pub struct CalendarInput {
    pub name: String,
    pub description: String,
    pub color: String,
}

pub fn validate_calendar_input(name: &str, description: &str, color: &str) -> Result<CalendarInput, String> {
    static COLOR_RE: OnceLock<Regex> = OnceLock::new();
    let color_re = COLOR_RE.get_or_init(|| Regex::new(r"^#[0-9a-fA-F]{6}$").unwrap());

    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("Le nom du calendrier doit contenir entre 1 et 80 caractères.".to_string());
    }
    let description = description.trim();
    if description.chars().count() > 500 {
        return Err("La description est limitée à 500 caractères.".to_string());
    }
    let color = color.trim();
    if !color_re.is_match(color) {
        return Err("Couleur invalide.".to_string());
    }
    Ok(CalendarInput {
        name: name.to_string(),
        description: description.to_string(),
        color: color.to_lowercase(),
    })
}

fn calendar_from_row(r: &rusqlite::Row) -> rusqlite::Result<CalendarRow> {
    Ok(CalendarRow {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        color: r.get(3)?,
        owner: r.get(4)?,
        created_at: r.get(5)?,
        event_count: r.get(6)?,
    })
}

const CALENDAR_COLUMNS: &str = "c.id, c.name, c.description, c.color, c.owner, c.created_at,
    (SELECT COUNT(*) FROM community_events e WHERE e.calendar_id = c.id)";

pub fn list_calendars(conn: &Connection) -> rusqlite::Result<Vec<CalendarRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM community_calendars c ORDER BY c.name COLLATE NOCASE",
        CALENDAR_COLUMNS
    ))?;
    let rows = stmt.query_map([], calendar_from_row)?;
    rows.collect()
}

pub fn get_calendar(conn: &Connection, id: i64) -> rusqlite::Result<Option<CalendarRow>> {
    conn.query_row(
        &format!("SELECT {} FROM community_calendars c WHERE c.id = ?1", CALENDAR_COLUMNS),
        params![id],
        calendar_from_row,
    )
    .optional()
}

pub fn create_calendar(conn: &Connection, owner: &str, input: &CalendarInput) -> Result<i64, String> {
    let owner = normalize_username(owner);
    let owned: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM community_calendars WHERE owner = ?1",
            params![owner],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if owned >= MAX_CALENDARS_PER_OWNER {
        return Err(format!("Limite atteinte : {} calendriers par modérateur.", MAX_CALENDARS_PER_OWNER));
    }
    conn.execute(
        "INSERT INTO community_calendars (name, description, color, owner, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![input.name, input.description, input.color, owner, now_secs()],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub fn update_calendar(conn: &Connection, id: i64, input: &CalendarInput) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE community_calendars SET name = ?1, description = ?2, color = ?3 WHERE id = ?4",
        params![input.name, input.description, input.color, id],
    )?;
    Ok(())
}

pub fn delete_calendar(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    // Explicit delete keeps this correct even if a connection runs without foreign_keys
    conn.execute("DELETE FROM community_events WHERE calendar_id = ?1", params![id])?;
    conn.execute("DELETE FROM community_calendars WHERE id = ?1", params![id])?;
    Ok(())
}

// -----------------------------------------------------------------------------
// Events
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct EventRow {
    pub id: i64,
    pub calendar_id: i64,
    pub calendar_name: String,
    pub calendar_owner: String,
    pub color: String,
    pub title: String,
    /// RFC 3339 with the Europe/Paris offset, ready for `new Date()`.
    pub start: String,
    pub end: String,
    /// Wall-clock Paris time (`YYYY-MM-DDTHH:MM`), used to prefill edit forms.
    pub start_local: String,
    pub end_local: String,
    pub location: String,
    pub description: String,
    pub created_by: String,
}

#[derive(Debug, Clone)]
pub struct EventInput {
    pub title: String,
    pub start: String,
    pub end: String,
    pub location: String,
    pub description: String,
}

fn parse_local(value: &str) -> Option<NaiveDateTime> {
    let trimmed = value.trim();
    // Accept an optional ":SS" suffix from datetime-local inputs
    let without_seconds = if trimmed.len() == 19 { &trimmed[..16] } else { trimmed };
    NaiveDateTime::parse_from_str(without_seconds, LOCAL_DATETIME_FORMAT).ok()
}

fn to_paris_rfc3339(local: &str) -> String {
    parse_local(local)
        .and_then(|naive| Paris.from_local_datetime(&naive).earliest())
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
        .unwrap_or_else(|| local.to_string())
}

pub fn validate_event_input(
    title: &str,
    start: &str,
    end: &str,
    location: &str,
    description: &str,
) -> Result<EventInput, String> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 120 {
        return Err("Le titre doit contenir entre 1 et 120 caractères.".to_string());
    }
    let start_dt = parse_local(start).ok_or("Date de début invalide.")?;
    let end_dt = parse_local(end).ok_or("Date de fin invalide.")?;
    if end_dt <= start_dt {
        return Err("La fin doit être après le début.".to_string());
    }
    // The Old Zeus grid lays events out per day
    if end_dt.date() != start_dt.date() {
        return Err("Un événement doit commencer et finir le même jour.".to_string());
    }
    let location = location.trim();
    if location.chars().count() > 120 {
        return Err("Le lieu est limité à 120 caractères.".to_string());
    }
    let description = description.trim();
    if description.chars().count() > 1000 {
        return Err("La description est limitée à 1000 caractères.".to_string());
    }
    Ok(EventInput {
        title: title.to_string(),
        start: start_dt.format(LOCAL_DATETIME_FORMAT).to_string(),
        end: end_dt.format(LOCAL_DATETIME_FORMAT).to_string(),
        location: location.to_string(),
        description: description.to_string(),
    })
}

fn event_from_row(r: &rusqlite::Row) -> rusqlite::Result<EventRow> {
    let start_local: String = r.get(6)?;
    let end_local: String = r.get(7)?;
    Ok(EventRow {
        id: r.get(0)?,
        calendar_id: r.get(1)?,
        calendar_name: r.get(2)?,
        calendar_owner: r.get(3)?,
        color: r.get(4)?,
        title: r.get(5)?,
        start: to_paris_rfc3339(&start_local),
        end: to_paris_rfc3339(&end_local),
        start_local,
        end_local,
        location: r.get(8)?,
        description: r.get(9)?,
        created_by: r.get(10)?,
    })
}

const EVENT_SELECT: &str = "SELECT e.id, e.calendar_id, c.name, c.owner, c.color, e.title, e.start, e.end,
        e.location, e.description, e.created_by
    FROM community_events e
    JOIN community_calendars c ON c.id = e.calendar_id";

/// Events of the given calendars between two `YYYY-MM-DD` dates (inclusive).
pub fn list_events(
    conn: &Connection,
    calendar_ids: &[i64],
    start_date: &str,
    end_date: &str,
) -> rusqlite::Result<Vec<EventRow>> {
    if calendar_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (0..calendar_ids.len())
        .map(|i| format!("?{}", i + 3))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "{} WHERE e.start >= ?1 AND e.start <= ?2 AND e.calendar_id IN ({}) ORDER BY e.start",
        EVENT_SELECT, placeholders
    );
    let mut values: Vec<rusqlite::types::Value> = vec![
        format!("{}T00:00", start_date).into(),
        format!("{}T23:59", end_date).into(),
    ];
    values.extend(calendar_ids.iter().map(|id| rusqlite::types::Value::from(*id)));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), event_from_row)?;
    rows.collect()
}

pub fn get_event(conn: &Connection, id: i64) -> rusqlite::Result<Option<EventRow>> {
    conn.query_row(&format!("{} WHERE e.id = ?1", EVENT_SELECT), params![id], event_from_row)
        .optional()
}

pub fn create_event(conn: &Connection, calendar_id: i64, created_by: &str, input: &EventInput) -> Result<i64, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM community_events WHERE calendar_id = ?1",
            params![calendar_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if count >= MAX_EVENTS_PER_CALENDAR {
        return Err(format!("Limite atteinte : {} événements par calendrier.", MAX_EVENTS_PER_CALENDAR));
    }
    conn.execute(
        "INSERT INTO community_events (calendar_id, title, start, end, location, description, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            calendar_id,
            input.title,
            input.start,
            input.end,
            input.location,
            input.description,
            normalize_username(created_by),
            now_secs()
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub fn update_event(conn: &Connection, id: i64, calendar_id: i64, input: &EventInput) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE community_events SET calendar_id = ?1, title = ?2, start = ?3, end = ?4, location = ?5, description = ?6
         WHERE id = ?7",
        params![calendar_id, input.title, input.start, input.end, input.location, input.description, id],
    )?;
    Ok(())
}

pub fn delete_event(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM community_events WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        init_db(&conn).unwrap();
        conn
    }

    #[test]
    fn test_normalize_username() {
        assert_eq!(normalize_username("  Fernand.Huttois@epita.fr "), "fernand.huttois");
        assert_eq!(normalize_username("prenom.nom"), "prenom.nom");
        assert!(is_valid_username("prenom.nom-2"));
        assert!(!is_valid_username("bad name"));
        assert!(!is_valid_username(""));
    }

    #[test]
    fn test_default_admin_is_project_owner() {
        if std::env::var("ADMIN_USERS").is_err() {
            assert!(is_admin("Fernand.Huttois@epita.fr"));
            assert!(!is_admin("someone.else"));
        }
    }

    #[test]
    fn test_roles_lifecycle() {
        let conn = test_conn();
        record_login(&conn, "Alice.Martin@epita.fr").unwrap();
        assert_eq!(get_role(&conn, "alice.martin").unwrap(), Role::User);

        set_role(&conn, "alice.martin", Role::Moderator).unwrap();
        assert_eq!(get_role(&conn, "Alice.Martin@epita.fr").unwrap(), Role::Moderator);

        // A later login must not reset the role
        record_login(&conn, "alice.martin").unwrap();
        assert_eq!(get_role(&conn, "alice.martin").unwrap(), Role::Moderator);

        set_role(&conn, "alice.martin", Role::User).unwrap();
        assert_eq!(get_role(&conn, "alice.martin").unwrap(), Role::User);

        // Never-seen users can be promoted ahead of their first login
        set_role(&conn, "bob.durand", Role::Moderator).unwrap();
        let users = list_users(&conn, None, 50).unwrap();
        assert_eq!(users[0].username, "bob.durand");
        assert!(users[0].last_seen.is_none());

        assert!(set_role(&conn, "not valid!", Role::Moderator).is_err());
        assert!(set_role(&conn, "x", Role::Admin).is_err());
    }

    #[test]
    fn test_calendar_permissions() {
        assert!(Role::Admin.can_edit_calendar("anyone", "alice.martin"));
        assert!(Role::Moderator.can_edit_calendar("Alice.Martin@epita.fr", "alice.martin"));
        assert!(!Role::Moderator.can_edit_calendar("bob.durand", "alice.martin"));
        assert!(!Role::User.can_edit_calendar("alice.martin", "alice.martin"));
    }

    #[test]
    fn test_event_validation() {
        assert!(validate_event_input("Soirée BDE", "2026-11-05T19:00", "2026-11-05T23:00", "", "").is_ok());
        assert!(validate_event_input("", "2026-11-05T19:00", "2026-11-05T23:00", "", "").is_err());
        assert!(validate_event_input("X", "2026-11-05T19:00", "2026-11-05T18:00", "", "").is_err());
        assert!(validate_event_input("X", "2026-11-05T19:00", "2026-11-06T01:00", "", "").is_err());
        assert!(validate_event_input("X", "not a date", "2026-11-05T18:00", "", "").is_err());
        assert!(validate_calendar_input("BDE", "", "#3B82F6").is_ok());
        assert!(validate_calendar_input("BDE", "", "red; background:url(x)").is_err());
    }

    #[test]
    fn test_calendar_and_event_crud() {
        let conn = test_conn();
        let input = validate_calendar_input("Club Robotique", "Ateliers du jeudi", "#10b981").unwrap();
        let cal_id = create_calendar(&conn, "Alice.Martin@epita.fr", &input).unwrap();

        let cal = get_calendar(&conn, cal_id).unwrap().unwrap();
        assert_eq!(cal.owner, "alice.martin");

        let ev = validate_event_input("Atelier Arduino", "2026-11-05T18:00", "2026-11-05T20:00", "Salle 301", "").unwrap();
        let ev_id = create_event(&conn, cal_id, "alice.martin", &ev).unwrap();

        let events = list_events(&conn, &[cal_id], "2026-11-02", "2026-11-08").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].start, "2026-11-05T18:00:00+01:00");
        assert_eq!(events[0].start_local, "2026-11-05T18:00");
        assert!(list_events(&conn, &[cal_id], "2026-11-06", "2026-11-08").unwrap().is_empty());

        delete_calendar(&conn, cal_id).unwrap();
        assert!(get_event(&conn, ev_id).unwrap().is_none());
    }
}
