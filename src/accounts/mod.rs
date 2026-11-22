//! Accounts: every Auriga login is recorded in `users`, and admins come from the
//! `ADMIN_USERS` setting (never from the database, so nobody can grant it).

use regex::Regex;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// `Prenom.Nom@epita.fr` and `prenom.nom` are the same account.
pub fn normalize_username(raw: &str) -> String {
    raw.trim().split('@').next().unwrap_or("").trim().to_lowercase()
}

/// Comma-separated `ADMIN_USERS`, defaulting to the project owner.
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
    RE.get_or_init(|| Regex::new(r"^[a-z0-9][a-z0-9._-]{0,63}$").unwrap())
        .is_match(username)
}

pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS users (
            username TEXT PRIMARY KEY,
            first_seen REAL,
            last_seen REAL
        );
        -- Club calendars and moderators were removed
        DROP TABLE IF EXISTS community_events;
        DROP TABLE IF EXISTS community_calendars;
        DROP INDEX IF EXISTS idx_users_role;",
    )?;
    let has_role = conn
        .prepare("PRAGMA table_info(users)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|c| c == "role");
    if has_role {
        conn.execute("ALTER TABLE users DROP COLUMN role", [])?;
    }
    Ok(())
}

pub fn record_login(conn: &Connection, username: &str) -> rusqlite::Result<()> {
    let user = normalize_username(username);
    if !is_valid_username(&user) {
        return Ok(());
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    conn.execute(
        "INSERT INTO users (username, first_seen, last_seen) VALUES (?1, ?2, ?2)
         ON CONFLICT(username) DO UPDATE SET
            last_seen = excluded.last_seen,
            first_seen = COALESCE(users.first_seen, excluded.first_seen)",
        params![user, now],
    )?;
    Ok(())
}

/// Forgets the account: its login and dates. Returns true if it existed.
pub fn delete_user(conn: &Connection, username: &str) -> rusqlite::Result<bool> {
    let user = normalize_username(username);
    Ok(conn.execute("DELETE FROM users WHERE username = ?1", params![user])? > 0)
}

#[derive(Debug, Clone, Serialize)]
pub struct UserRow {
    pub username: String,
    pub admin: bool,
    pub first_seen: Option<f64>,
    pub last_seen: Option<f64>,
}

/// Users whose login contains `query`, most recently seen first.
pub fn list_users(conn: &Connection, query: Option<&str>, limit: i64) -> rusqlite::Result<Vec<UserRow>> {
    let pattern = format!("%{}%", normalize_username(query.unwrap_or("")));
    let mut stmt = conn.prepare(
        "SELECT username, first_seen, last_seen FROM users
         WHERE username LIKE ?1
         ORDER BY last_seen IS NULL, last_seen DESC, username
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![pattern, limit], |r| {
        let username: String = r.get(0)?;
        Ok(UserRow { admin: is_admin(&username), username, first_seen: r.get(1)?, last_seen: r.get(2)? })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_are_normalized() {
        assert_eq!(normalize_username(" Fernand.Huttois@epita.fr "), "fernand.huttois");
        assert!(is_valid_username("prenom.nom"));
        assert!(!is_valid_username("<script>"));
    }

    #[test]
    fn logins_are_recorded_and_old_schema_is_migrated() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (username TEXT PRIMARY KEY, role TEXT NOT NULL DEFAULT 'user'
                 CHECK (role IN ('user', 'moderator')), first_seen REAL, last_seen REAL);
             CREATE INDEX idx_users_role ON users(role);
             CREATE TABLE community_calendars (id INTEGER PRIMARY KEY);
             INSERT INTO users (username, role) VALUES ('alice.martin', 'moderator');",
        )
        .unwrap();
        init_db(&conn).unwrap();
        init_db(&conn).unwrap();
        record_login(&conn, "Bob.Durand@epita.fr").unwrap();
        record_login(&conn, "fernand.huttois").unwrap();
        let users = list_users(&conn, None, 10).unwrap();
        assert_eq!(users.len(), 3, "existing users kept");
        assert!(users.iter().any(|u| u.username == "fernand.huttois" && u.admin));
        assert!(users.iter().any(|u| u.username == "bob.durand" && !u.admin));
        let tables: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'community_%'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tables, 0);
    }
}
