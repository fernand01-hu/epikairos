pub mod models;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use regex::Regex;
use rusqlite::{params, Connection};
use serde_json::json;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub use models::*;

pub type DbPool = Pool<SqliteConnectionManager>;

pub const EPITA_PROMOTIONS_DEF: &[(&str, &str, &str, &str, i32, &str)] = &[
    (
        "sup_lyon",
        "SUP",
        "SUP Lyon (1ère année)",
        "Classes Préparatoires",
        1,
        "S1/S2",
    ),
    (
        "spe_lyon",
        "SPE",
        "SPÉ Lyon (2ème année)",
        "Classes Préparatoires",
        2,
        "S3/S4",
    ),
    (
        "ing1_lyon",
        "ING1",
        "ING1 Lyon (Cycle Ingénieur)",
        "Cycle Ingénieur",
        3,
        "SI5/SI6",
    ),
    (
        "ing2_lyon",
        "ING2",
        "ING2 Lyon",
        "Cycle Ingénieur",
        4,
        "SI7/SI8",
    ),
    (
        "ing3_lyon",
        "ING3",
        "ING3 / Majeures Lyon",
        "Cycle Ingénieur",
        5,
        "SI9/SI10",
    ),
    (
        "autre",
        "AUTRE",
        "Autres Groupes",
        "Divers",
        99,
        "",
    ),
];

pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &s[..boundary]
}

pub fn safe_slice(s: &str, start_bytes: usize, end_bytes: usize) -> Option<&str> {
    if start_bytes <= end_bytes
        && end_bytes <= s.len()
        && s.is_char_boundary(start_bytes)
        && s.is_char_boundary(end_bytes)
    {
        Some(&s[start_bytes..end_bytes])
    } else {
        None
    }
}

pub fn sanitize_room_name(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    let re1 = Regex::new(r"(?i)[\s\-_]*\(\s*\d+e?\s*étage\s*\)").unwrap();
    let re2 = Regex::new(r"(?i)[\s\-_]*\d+e?\s*étage").unwrap();
    let re3 = Regex::new(r"(?i)\s*\(\s*RDC\s*\)").unwrap();

    let s1 = re1.replace_all(name, "");
    let s2 = re2.replace_all(&s1, "");
    let s3 = re3.replace_all(&s2, "");
    let cleaned = s3.trim();
    if cleaned.is_empty() {
        name.trim().to_string()
    } else {
        cleaned.to_string()
    }
}

pub fn sanitize_teacher_name(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    let first_part = name.replace("\\n", "\n");
    let line1 = first_part.lines().next().unwrap_or("").trim();
    let re = Regex::new(r"(?i)(Groupe|Salle|Statut)\(s\):.*").unwrap();
    let cleaned = re.replace(line1, "").trim().to_string();
    if cleaned.is_empty() {
        name.trim().to_string()
    } else {
        cleaned
    }
}

#[derive(Debug, Clone)]
pub struct GroupIdentity {
    pub raw_name: String,
    pub promo_id: String,
    pub promo_code: String,
    pub promo_name: String,
    pub cycle: String,
    pub year: i32,
    pub semesters: String,
    pub subgroup: String,
    pub display_name: String,
    pub clean_name: String,
    pub is_promo_wide: bool,
}

pub fn parse_group_identity(raw_name: &str) -> GroupIdentity {
    let n = raw_name.trim();
    let upper = n.to_uppercase();

    let p_idx = if upper.contains("S1") || upper.contains("S2") || upper.contains("SUP") {
        0
    } else if upper.contains("S3") || upper.contains("S4") || upper.contains("SPE") || upper.contains("SPÉ") {
        1
    } else if upper.contains("SI5") || upper.contains("SI6") || upper.contains("ING1") || upper.contains("S5") {
        2
    } else if upper.contains("SI7") || upper.contains("SI8") || upper.contains("ING2") || upper.contains("S7") {
        3
    } else if upper.contains("SI9")
        || upper.contains("SI10")
        || upper.contains("ING3")
        || upper.contains("SCIA")
        || upper.contains("RDI")
        || upper.contains("SANTE")
        || upper.contains("IF")
        || upper.contains("S9")
    {
        4
    } else {
        5
    };

    let promo = EPITA_PROMOTIONS_DEF[p_idx];

    let l_re = Regex::new(r"\b(L[1-9])\b").unwrap();
    let (subgroup, display) = if let Some(cap) = l_re.captures(&upper) {
        let sub = cap[1].to_string();
        let disp = format!("Classe {}", sub);
        (sub, disp)
    } else if upper.contains("SCIA") {
        ("SCIA-G".to_string(), "Majeure SCIA Graphes".to_string())
    } else if upper.contains("RDI") {
        ("RDI-L".to_string(), "Majeure RDI-L".to_string())
    } else if upper.contains("SANTE") {
        ("SANTE".to_string(), "Majeure Santé".to_string())
    } else if upper.contains("IF") {
        ("IF".to_string(), "Majeure Ingénierie Financière".to_string())
    } else {
        ("Promo".to_string(), "Promotion Complète (Tous)".to_string())
    };

    let clean_name = format!("{} - {}", promo.2, display);

    GroupIdentity {
        raw_name: n.to_string(),
        promo_id: promo.0.to_string(),
        promo_code: promo.1.to_string(),
        promo_name: promo.2.to_string(),
        cycle: promo.3.to_string(),
        year: promo.4,
        semesters: promo.5.to_string(),
        is_promo_wide: subgroup == "Promo",
        subgroup,
        display_name: display,
        clean_name,
    }
}

/// Date (YYYY-MM-DD) and time (HH:MM) of a course timestamp in Paris time.
/// Auriga sends UTC ("…Z"); timestamps without an offset are already local.
pub fn paris_date_time(ts: &str, default_time: &str) -> (String, String) {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        let local = dt.with_timezone(&chrono_tz::Europe::Paris);
        return (local.format("%Y-%m-%d").to_string(), local.format("%H:%M").to_string());
    }
    (
        safe_truncate(ts, 10).to_string(),
        safe_slice(ts, 11, 16).unwrap_or(default_time).to_string(),
    )
}

/// Recomputes the local date/time columns of courses stored with a UTC timestamp
/// by older versions, which kept the UTC hour as if it were local.
fn fix_utc_time_columns(conn: &Connection) -> rusqlite::Result<usize> {
    let rows: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, start, end FROM courses WHERE start LIKE '%Z' OR start LIKE '%+__:__'",
        )?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        mapped.filter_map(Result::ok).collect()
    };
    let mut fixed = 0;
    for (id, start, end) in rows {
        let (date, start_t) = paris_date_time(&start, "00:00");
        let (_, end_t) = paris_date_time(&end, "23:59");
        fixed += conn.execute(
            "UPDATE courses SET date_str = ?1, start_time_str = ?2, end_time_str = ?3
             WHERE id = ?4 AND (date_str != ?1 OR start_time_str != ?2 OR end_time_str != ?3)",
            params![date, start_t, end_t, id],
        )?;
    }
    Ok(fixed)
}

fn columns_of(conn: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let cols = stmt.query_map([], |row| row.get::<_, String>(1))?.filter_map(Result::ok).collect();
    Ok(cols)
}

/// Removes what older versions created and nothing uses anymore: the recurring
/// courses ("planning type") and contributions tables, `courses.recurring_id` /
/// `courses.pattern_key`, `rooms.building` / `rooms.floor`. Idempotent.
fn drop_retired_schema(conn: &Connection) -> rusqlite::Result<()> {
    let course_cols = columns_of(conn, "courses")?;
    if course_cols.iter().any(|c| c == "recurring_id" || c == "pattern_key") {
        // recurring_id is a foreign key, which SQLite cannot drop in place: rebuild
        // the table (sqlite.org/lang_altertable.html, "other kinds of changes").
        // Foreign keys are off meanwhile so the course links are kept as they are.
        conn.execute_batch(
            "PRAGMA foreign_keys = OFF;
             BEGIN;
             CREATE TABLE courses_new (
                 id TEXT PRIMARY KEY,
                 start TEXT NOT NULL,
                 end TEXT NOT NULL,
                 date_str TEXT NOT NULL,
                 start_time_str TEXT NOT NULL,
                 end_time_str TEXT NOT NULL,
                 duration INTEGER DEFAULT 90,
                 course_name TEXT NOT NULL,
                 course_code TEXT,
                 activity_type TEXT NOT NULL,
                 activity_name TEXT,
                 is_exam INTEGER DEFAULT 0,
                 is_cancelled INTEGER DEFAULT 0,
                 status TEXT,
                 description TEXT,
                 searchable_text TEXT,
                 updated_at REAL NOT NULL
             );
             INSERT INTO courses_new
                 SELECT id, start, end, date_str, start_time_str, end_time_str, duration, course_name,
                        course_code, activity_type, activity_name, is_exam, is_cancelled, status,
                        description, searchable_text, updated_at
                 FROM courses;
             DROP TABLE courses;
             ALTER TABLE courses_new RENAME TO courses;
             CREATE INDEX IF NOT EXISTS idx_courses_date ON courses(date_str);
             CREATE INDEX IF NOT EXISTS idx_courses_start ON courses(start);
             CREATE INDEX IF NOT EXISTS idx_courses_type ON courses(activity_type);
             CREATE INDEX IF NOT EXISTS idx_courses_slot ON courses(start, end, course_name);
             COMMIT;
             PRAGMA foreign_keys = ON;",
        )?;
    }
    conn.execute_batch("DROP TABLE IF EXISTS recurring_courses; DROP TABLE IF EXISTS contributions;")?;
    let room_cols = columns_of(conn, "rooms")?;
    for col in ["building", "floor"] {
        if room_cols.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE rooms DROP COLUMN {col}"), [])?;
        }
    }
    Ok(())
}

pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS courses (
            id TEXT PRIMARY KEY,
            start TEXT NOT NULL,
            end TEXT NOT NULL,
            date_str TEXT NOT NULL,
            start_time_str TEXT NOT NULL,
            end_time_str TEXT NOT NULL,
            duration INTEGER DEFAULT 90,
            course_name TEXT NOT NULL,
            course_code TEXT,
            activity_type TEXT NOT NULL,
            activity_name TEXT,
            is_exam INTEGER DEFAULT 0,
            is_cancelled INTEGER DEFAULT 0,
            status TEXT,
            description TEXT,
            searchable_text TEXT,
            updated_at REAL NOT NULL
        );

        CREATE TABLE IF NOT EXISTS teachers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT UNIQUE COLLATE NOCASE
        );

        CREATE TABLE IF NOT EXISTS course_teachers (
            course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
            teacher_id INTEGER NOT NULL REFERENCES teachers(id) ON DELETE CASCADE,
            PRIMARY KEY (course_id, teacher_id)
        );

        CREATE TABLE IF NOT EXISTS rooms (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT UNIQUE COLLATE NOCASE
        );

        CREATE TABLE IF NOT EXISTS course_rooms (
            course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
            room_id INTEGER NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
            PRIMARY KEY (course_id, room_id)
        );

        CREATE TABLE IF NOT EXISTS groups (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT UNIQUE COLLATE NOCASE
        );

        CREATE TABLE IF NOT EXISTS course_groups (
            course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
            group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
            PRIMARY KEY (course_id, group_id)
        );

        CREATE TABLE IF NOT EXISTS promotions (
            id TEXT PRIMARY KEY,
            code TEXT NOT NULL,
            name TEXT NOT NULL,
            cycle TEXT NOT NULL,
            year INTEGER NOT NULL,
            semesters TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_courses_date ON courses(date_str);
        CREATE INDEX IF NOT EXISTS idx_courses_start ON courses(start);
        CREATE INDEX IF NOT EXISTS idx_courses_type ON courses(activity_type);
        DROP INDEX IF EXISTS idx_courses_slot;
        CREATE INDEX IF NOT EXISTS idx_courses_slot ON courses(start, end, course_name);
        CREATE INDEX IF NOT EXISTS idx_teachers_name ON teachers(name);
        CREATE INDEX IF NOT EXISTS idx_rooms_name ON rooms(name);
        CREATE INDEX IF NOT EXISTS idx_groups_name ON groups(name);
        CREATE INDEX IF NOT EXISTS idx_course_teachers_tid ON course_teachers(teacher_id);
        CREATE INDEX IF NOT EXISTS idx_course_rooms_rid ON course_rooms(room_id);
        CREATE INDEX IF NOT EXISTS idx_course_groups_gid ON course_groups(group_id);",
    )?;

    drop_retired_schema(conn)?;

    let mut stmt_g = conn.prepare("PRAGMA table_info(groups)")?;
    let g_cols: Vec<String> = stmt_g
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();

    if !g_cols.contains(&"clean_name".to_string()) {
        conn.execute("ALTER TABLE groups ADD COLUMN clean_name TEXT;", [])?;
    }
    if !g_cols.contains(&"promo_id".to_string()) {
        conn.execute(
            "ALTER TABLE groups ADD COLUMN promo_id TEXT REFERENCES promotions(id);",
            [],
        )?;
    }
    if !g_cols.contains(&"subgroup".to_string()) {
        conn.execute("ALTER TABLE groups ADD COLUMN subgroup TEXT;", [])?;
    }
    if !g_cols.contains(&"display_name".to_string()) {
        conn.execute("ALTER TABLE groups ADD COLUMN display_name TEXT;", [])?;
    }

    for p in EPITA_PROMOTIONS_DEF {
        conn.execute(
            "INSERT INTO promotions (id, code, name, cycle, year, semesters)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                 code = excluded.code,
                 name = excluded.name,
                 cycle = excluded.cycle,
                 year = excluded.year,
                 semesters = excluded.semesters;",
            params![p.0, p.1, p.2, p.3, p.4, p.5],
        )?;
    }

    fix_utc_time_columns(conn)?;

    // Classrooms on the floor plan that no synced planning has used yet: list
    // them so the free-rooms search shows them as free instead of unknown.
    for name in KNOWN_CLASSROOMS {
        conn.execute(
            "INSERT INTO rooms (name) VALUES (?1) ON CONFLICT(name) DO NOTHING;",
            params![name],
        )?;
    }

    Ok(())
}

const KNOWN_CLASSROOMS: &[&str] = &["Salle 317", "Salle 327", "Salle 329", "Salle 332"];

pub fn ingest_courses(
    conn: &mut Connection,
    courses: &[Course],
    consent: bool,
) -> rusqlite::Result<serde_json::Value> {
    if !consent {
        return Ok(json!({
            "success": false,
            "reason": "Consent not given",
            "inserted": 0,
            "updated": 0
        }));
    }

    if courses.is_empty() {
        return Ok(json!({
            "success": true,
            "inserted": 0,
            "updated": 0
        }));
    }

    let table_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='courses'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !table_exists {
        init_db(conn)?;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let mut inserted_count = 0;
    let mut updated_count = 0;

    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

    for c in courses {
        let c_id = c.id.trim();
        let start = c.start.trim();
        let end = c.end.trim();

        if c_id.is_empty() || start.is_empty() || end.is_empty() {
            continue;
        }

        let (date_str, start_time_str) = paris_date_time(start, "00:00");
        let (_, end_time_str) = paris_date_time(end, "23:59");
        let (date_str, start_time_str, end_time_str) = (date_str.as_str(), start_time_str.as_str(), end_time_str.as_str());

        let course_name = safe_truncate(&c.course_name, 300);
        let course_code = safe_truncate(&c.course_code, 100);
        let activity_type = c.activity_type.trim().to_uppercase();
        let activity_name = safe_truncate(&c.activity_name, 100);
        let is_exam = if c.is_exam { 1 } else { 0 };
        let is_cancelled = if c.is_cancelled { 1 } else { 0 };
        let status = safe_truncate(&c.status, 50);
        let description = safe_truncate(&c.description, 2000);
        let duration = if c.duration > 0 { c.duration } else { 90 };
        let default_search = format!("{} {}", course_name, activity_type);
        let searchable_text = if !c.searchable_text.is_empty() {
            &c.searchable_text
        } else {
            &default_search
        };

        let actual_id = c_id;
        let exists: bool = tx
            .query_row(
                "SELECT 1 FROM courses WHERE id = ?1",
                params![actual_id],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if exists {
            tx.execute(
                "UPDATE courses SET
                    start = ?1, end = ?2, date_str = ?3, start_time_str = ?4, end_time_str = ?5,
                    duration = ?6, course_name = ?7, course_code = ?8, activity_type = ?9,
                    activity_name = ?10, is_exam = ?11, is_cancelled = ?12, status = ?13,
                    description = ?14, searchable_text = ?15, updated_at = ?16
                 WHERE id = ?17",
                params![
                    start, end, date_str, start_time_str, end_time_str,
                    duration, course_name, course_code, activity_type,
                    activity_name, is_exam, is_cancelled, status,
                    description, searchable_text, now, actual_id
                ],
            )?;
            updated_count += 1;
        } else {
            tx.execute(
                "INSERT INTO courses (
                    id, start, end, date_str, start_time_str, end_time_str,
                    duration, course_name, course_code, activity_type,
                    activity_name, is_exam, is_cancelled, status,
                    description, searchable_text, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    actual_id, start, end, date_str, start_time_str, end_time_str,
                    duration, course_name, course_code, activity_type,
                    activity_name, is_exam, is_cancelled, status,
                    description, searchable_text, now
                ],
            )?;
            inserted_count += 1;
        }

        // Teachers
        // An empty list from Auriga keeps what was stored before instead of erasing it.
        if !c.teachers.is_empty() {
            tx.execute("DELETE FROM course_teachers WHERE course_id = ?1", params![actual_id])?;
        }
        for teacher_raw in &c.teachers {
            let t_name = sanitize_teacher_name(teacher_raw);
            if t_name.is_empty() {
                continue;
            }
            tx.execute("INSERT INTO teachers (name) VALUES (?1) ON CONFLICT(name) DO NOTHING;", params![t_name])?;
            if let Ok(tid) = tx.query_row::<i64, _, _>(
                "SELECT id FROM teachers WHERE name = ?1",
                params![t_name],
                |r| r.get(0),
            ) {
                let _ = tx.execute(
                    "INSERT OR IGNORE INTO course_teachers (course_id, teacher_id) VALUES (?1, ?2);",
                    params![actual_id, tid],
                );
            }
        }

        // Rooms
        // An empty list from Auriga keeps what was stored before instead of erasing it.
        if !c.rooms.is_empty() {
            tx.execute("DELETE FROM course_rooms WHERE course_id = ?1", params![actual_id])?;
        }
        for room_raw in &c.rooms {
            let r_name = sanitize_room_name(room_raw);
            if r_name.is_empty() {
                continue;
            }
            tx.execute("INSERT INTO rooms (name) VALUES (?1) ON CONFLICT(name) DO NOTHING;", params![r_name])?;
            if let Ok(rid) = tx.query_row::<i64, _, _>(
                "SELECT id FROM rooms WHERE name = ?1",
                params![r_name],
                |r| r.get(0),
            ) {
                let _ = tx.execute(
                    "INSERT OR IGNORE INTO course_rooms (course_id, room_id) VALUES (?1, ?2);",
                    params![actual_id, rid],
                );
            }
        }

        // Groups
        // An empty list from Auriga keeps what was stored before instead of erasing it.
        if !c.groups.is_empty() {
            tx.execute("DELETE FROM course_groups WHERE course_id = ?1", params![actual_id])?;
        }
        for group_raw in &c.groups {
            let g_name = group_raw.trim();
            if g_name.is_empty() {
                continue;
            }
            let info = parse_group_identity(g_name);
            tx.execute(
                "INSERT INTO groups (name, clean_name, promo_id, subgroup, display_name)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(name) DO UPDATE SET
                     clean_name = excluded.clean_name,
                     promo_id = excluded.promo_id,
                     subgroup = excluded.subgroup,
                     display_name = excluded.display_name;",
                params![g_name, info.clean_name, info.promo_id, info.subgroup, info.display_name],
            )?;
            if let Ok(gid) = tx.query_row::<i64, _, _>(
                "SELECT id FROM groups WHERE name = ?1",
                params![g_name],
                |r| r.get(0),
            ) {
                let _ = tx.execute(
                    "INSERT OR IGNORE INTO course_groups (course_id, group_id) VALUES (?1, ?2);",
                    params![actual_id, gid],
                );
            }
        }
    }

    tx.commit()?;

    Ok(json!({
        "success": true,
        "inserted": inserted_count,
        "updated": updated_count,
        "total": inserted_count + updated_count,
    }))
}

pub fn fetch_course_teachers_batched(
    conn: &Connection,
    course_ids: &[String],
) -> rusqlite::Result<HashMap<String, Vec<String>>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for chunk in course_ids.chunks(400) {
        if chunk.is_empty() {
            continue;
        }
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let id_params: Vec<&dyn rusqlite::ToSql> = chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let mut stmt = conn.prepare(&format!(
            "SELECT ct.course_id, t.name FROM course_teachers ct
             JOIN teachers t ON ct.teacher_id = t.id
             WHERE ct.course_id IN ({})
             ORDER BY t.name ASC;",
            placeholders
        ))?;
        let rows = stmt.query_map(&id_params[..], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for item in rows.flatten() {
            map.entry(item.0).or_default().push(item.1);
        }
    }
    Ok(map)
}

pub fn fetch_course_rooms_batched(
    conn: &Connection,
    course_ids: &[String],
) -> rusqlite::Result<HashMap<String, Vec<String>>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for chunk in course_ids.chunks(400) {
        if chunk.is_empty() {
            continue;
        }
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let id_params: Vec<&dyn rusqlite::ToSql> = chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let mut stmt = conn.prepare(&format!(
            "SELECT cr.course_id, r.name FROM course_rooms cr
             JOIN rooms r ON cr.room_id = r.id
             WHERE cr.course_id IN ({})
             ORDER BY r.name ASC;",
            placeholders
        ))?;
        let rows = stmt.query_map(&id_params[..], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for item in rows.flatten() {
            map.entry(item.0).or_default().push(item.1);
        }
    }
    Ok(map)
}

pub fn fetch_course_groups_batched(
    conn: &Connection,
    course_ids: &[String],
) -> rusqlite::Result<HashMap<String, Vec<String>>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for chunk in course_ids.chunks(400) {
        if chunk.is_empty() {
            continue;
        }
        let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let id_params: Vec<&dyn rusqlite::ToSql> = chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let mut stmt = conn.prepare(&format!(
            "SELECT cg.course_id, g.name FROM course_groups cg
             JOIN groups g ON cg.group_id = g.id
             WHERE cg.course_id IN ({})
             ORDER BY g.name ASC;",
            placeholders
        ))?;
        let rows = stmt.query_map(&id_params[..], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for item in rows.flatten() {
            map.entry(item.0).or_default().push(item.1);
        }
    }
    Ok(map)
}

pub fn query_free_rooms(
    conn: &Connection,
    target_date: &str,
    start_time: &str,
    end_time: &str,
) -> rusqlite::Result<FreeRoomsResponse> {
    let target_d = safe_truncate(target_date, 10);
    let t_start = start_time.trim();
    let t_end = end_time.trim();

    let mut r_stmt = conn.prepare("SELECT id, name FROM rooms ORDER BY name COLLATE NOCASE ASC")?;
    let all_rooms: Vec<(i64, String)> = r_stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(Result::ok)
        .collect();

    if all_rooms.is_empty() {
        return Ok(FreeRoomsResponse {
            date: target_d.to_string(),
            start_time: t_start.to_string(),
            end_time: t_end.to_string(),
            total_rooms: 0,
            free_count: 0,
            occupied_count: 0,
            free_rooms: Vec::new(),
            occupied_rooms: Vec::new(),
        });
    }

    struct DayCourse {
        id: String,
        course_name: String,
        activity_type: String,
        start_time_str: String,
        end_time_str: String,
        room_id: i64,
    }

    let mut c_stmt = conn.prepare(
        "SELECT c.id, c.course_name, c.activity_type, c.start_time_str, c.end_time_str, cr.room_id
         FROM courses c
         JOIN course_rooms cr ON c.id = cr.course_id
         WHERE c.date_str = ?1 AND c.is_cancelled = 0
         ORDER BY c.start ASC;",
    )?;

    let day_courses: Vec<DayCourse> = c_stmt
        .query_map(params![target_d], |r| {
            Ok(DayCourse {
                id: r.get(0)?,
                course_name: r.get(1)?,
                activity_type: r.get(2)?,
                start_time_str: r.get(3)?,
                end_time_str: r.get(4)?,
                room_id: r.get(5)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    let mut room_courses_map: HashMap<i64, Vec<usize>> = HashMap::new();
    for (idx, dc) in day_courses.iter().enumerate() {
        room_courses_map.entry(dc.room_id).or_default().push(idx);
    }

    let mut course_ids: Vec<String> = day_courses.iter().map(|dc| dc.id.clone()).collect();
    course_ids.sort();
    course_ids.dedup();

    let (teacher_map, group_map) = if !course_ids.is_empty() {
        (
            fetch_course_teachers_batched(conn, &course_ids)?,
            fetch_course_groups_batched(conn, &course_ids)?,
        )
    } else {
        (HashMap::new(), HashMap::new())
    };

    let mut free_rooms = Vec::new();
    let mut occupied_rooms = Vec::new();

    for (r_id, r_name) in all_rooms {
        let indices = room_courses_map.get(&r_id).cloned().unwrap_or_default();
        let mut overlapping = Vec::new();
        let mut subsequent = Vec::new();

        for idx in &indices {
            let dc = &day_courses[*idx];
            let c_start = dc.start_time_str.as_str();
            let c_end = dc.end_time_str.as_str();

            if c_start < t_end && c_end > t_start {
                overlapping.push(dc);
            } else if c_start >= t_end {
                subsequent.push(dc);
            }
        }

        if let Some(active_c) = overlapping.first() {
            let cid = &active_c.id;
            let teachers = teacher_map.get(cid).cloned().unwrap_or_default();
            let groups = group_map.get(cid).cloned().unwrap_or_default();

            occupied_rooms.push(FreeRoomItem {
                room_name: r_name,
                is_free: false,
                free_until: None,
                next_course: None,
                courses_count_today: None,
                current_course: Some(json!({
                    "id": cid,
                    "title": active_c.course_name,
                    "type": active_c.activity_type,
                    "start": active_c.start_time_str,
                    "end": active_c.end_time_str,
                    "teachers": teachers,
                    "groups": groups,
                })),
                total_overlapping: Some(overlapping.len()),
            });
        } else {
            let mut free_until_text = "Toute la journée".to_string();
            let mut next_course_info = None;

            if !subsequent.is_empty() {
                subsequent.sort_by_key(|c| &c.start_time_str);
                let next_c = subsequent[0];
                free_until_text = format!("Jusqu'à {}", next_c.start_time_str);
                next_course_info = Some(json!({
                    "title": next_c.course_name,
                    "type": next_c.activity_type,
                    "start": next_c.start_time_str,
                    "end": next_c.end_time_str,
                }));
            }

            free_rooms.push(FreeRoomItem {
                room_name: r_name,
                is_free: true,
                free_until: Some(free_until_text),
                next_course: next_course_info,
                courses_count_today: Some(indices.len()),
                current_course: None,
                total_overlapping: None,
            });
        }
    }

    free_rooms.sort_by(|a, b| a.room_name.cmp(&b.room_name));
    occupied_rooms.sort_by(|a, b| a.room_name.cmp(&b.room_name));

    let free_count = free_rooms.len();
    let occupied_count = occupied_rooms.len();
    let total = free_count + occupied_count;

    Ok(FreeRoomsResponse {
        date: target_d.to_string(),
        start_time: t_start.to_string(),
        end_time: t_end.to_string(),
        total_rooms: total,
        free_count,
        occupied_count,
        free_rooms,
        occupied_rooms,
    })
}

pub fn get_epita_group_tree() -> GroupTreeNode {
    GroupTreeNode {
        id: "root".to_string(),
        label: "EPITA".to_string(),
        expanded: true,
        patterns: None,
        children: Some(vec![
            GroupTreeNode {
                id: "prepa".to_string(),
                label: "CLASSES PREPARATOIRES".to_string(),
                expanded: true,
                patterns: None,
                children: Some(vec![
                    GroupTreeNode {
                        id: "spe_lyon".to_string(),
                        label: "SPE Lyon".to_string(),
                        expanded: true,
                        patterns: None,
                        children: Some(vec![
                            GroupTreeNode {
                                id: "spe_lyon_l1".to_string(),
                                label: "S3S4 L1".to_string(),
                                expanded: false,
                                patterns: Some(vec!["S3 Lyon L1".to_string(), "S3S4 L1".to_string(), "S3 L1".to_string()]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "spe_lyon_l2".to_string(),
                                label: "S3S4 L2".to_string(),
                                expanded: false,
                                patterns: Some(vec!["S3 Lyon L2".to_string(), "S3S4 L2".to_string(), "S3 L2".to_string()]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "spe_lyon_l3".to_string(),
                                label: "S3S4 L3".to_string(),
                                expanded: false,
                                patterns: Some(vec![
                                    "S3 Lyon L3".to_string(),
                                    "S3S4 L3".to_string(),
                                    "S3 L3".to_string(),
                                    "2627 - S3 Lyon L3".to_string(),
                                ]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "spe_lyon_promo".to_string(),
                                label: "Promo SPE Lyon".to_string(),
                                expanded: false,
                                patterns: Some(vec!["2627 - S3 Lyon".to_string(), "S3 Lyon".to_string()]),
                                children: None,
                            },
                        ]),
                    },
                    GroupTreeNode {
                        id: "sup_lyon".to_string(),
                        label: "SUP Lyon".to_string(),
                        expanded: false,
                        patterns: None,
                        children: Some(vec![
                            GroupTreeNode {
                                id: "sup_lyon_l1".to_string(),
                                label: "S1S2 L1".to_string(),
                                expanded: false,
                                patterns: Some(vec!["S1 Lyon L1".to_string(), "S1S2 L1".to_string(), "S1 L1".to_string()]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "sup_lyon_l2".to_string(),
                                label: "S1S2 L2".to_string(),
                                expanded: false,
                                patterns: Some(vec!["S1 Lyon L2".to_string(), "S1S2 L2".to_string(), "S1 L2".to_string()]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "sup_lyon_l3".to_string(),
                                label: "S1S2 L3".to_string(),
                                expanded: false,
                                patterns: Some(vec!["S1 Lyon L3".to_string(), "S1S2 L3".to_string(), "S1 L3".to_string()]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "sup_lyon_promo".to_string(),
                                label: "Promo SUP Lyon".to_string(),
                                expanded: false,
                                patterns: Some(vec!["2627 - S1 Lyon".to_string(), "S1 Lyon".to_string()]),
                                children: None,
                            },
                        ]),
                    },
                ]),
            },
            GroupTreeNode {
                id: "cycle_ingenieur".to_string(),
                label: "CYCLE INGENIEUR".to_string(),
                expanded: false,
                patterns: None,
                children: Some(vec![
                    GroupTreeNode {
                        id: "ing1_lyon".to_string(),
                        label: "ING1 Lyon (S5/S6)".to_string(),
                        expanded: false,
                        patterns: None,
                        children: Some(vec![
                            GroupTreeNode {
                                id: "ing1_lyon_l1".to_string(),
                                label: "SI5 Lyon L1".to_string(),
                                expanded: false,
                                patterns: Some(vec!["SI5 Lyon L1".to_string(), "ING1 Lyon L1".to_string(), "SI5 L1".to_string()]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "ing1_lyon_l2".to_string(),
                                label: "SI5 Lyon L2".to_string(),
                                expanded: false,
                                patterns: Some(vec!["SI5 Lyon L2".to_string(), "ING1 Lyon L2".to_string(), "SI5 L2".to_string()]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "ing1_lyon_l3".to_string(),
                                label: "SI5 Lyon L3".to_string(),
                                expanded: false,
                                patterns: Some(vec!["SI5 Lyon L3".to_string(), "ING1 Lyon L3".to_string(), "SI5 L3".to_string()]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "ing1_lyon_promo".to_string(),
                                label: "Promo SI5 Lyon".to_string(),
                                expanded: false,
                                patterns: Some(vec!["2627 - SI5 Lyon".to_string(), "SI5 Lyon".to_string()]),
                                children: None,
                            },
                        ]),
                    },
                    GroupTreeNode {
                        id: "majeures_lyon".to_string(),
                        label: "MAJEURES LYON".to_string(),
                        expanded: false,
                        patterns: None,
                        children: Some(vec![
                            GroupTreeNode {
                                id: "maj_if".to_string(),
                                label: "IF".to_string(),
                                expanded: false,
                                patterns: Some(vec!["IF".to_string(), "Ingénierie Financière".to_string(), "SI7 IF".to_string(), "SI9 IF".to_string()]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "maj_rdil".to_string(),
                                label: "RDI-L".to_string(),
                                expanded: false,
                                patterns: Some(vec!["RDI-L".to_string(), "RDI".to_string(), "SI7 RDI".to_string(), "SI9 RDI".to_string()]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "maj_sante".to_string(),
                                label: "SANTE".to_string(),
                                expanded: false,
                                patterns: Some(vec!["SANTE".to_string(), "Santé".to_string(), "SI7 SANTE".to_string(), "SI9 SANTE".to_string()]),
                                children: None,
                            },
                            GroupTreeNode {
                                id: "maj_scia_graphes".to_string(),
                                label: "SCIA Graphes".to_string(),
                                expanded: false,
                                patterns: Some(vec![
                                    "SCIA Graphes".to_string(),
                                    "SCIA-G".to_string(),
                                    "SI9 Lyon SCIA-G".to_string(),
                                    "SI9 Lyon SCIA".to_string(),
                                    "SCIA".to_string(),
                                ]),
                                children: None,
                            },
                        ]),
                    },
                ]),
            },
        ]),
    }
}

/// Courses of the selected groups (name contains a pattern) or taught by one of
/// the selected teachers (exact name, any case). Nothing selected means no courses.
pub fn query_courses_by_groups(
    conn: &Connection,
    patterns: &[String],
    teachers: &[String],
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> rusqlite::Result<Vec<Course>> {
    let pat_slice = if patterns.len() > 100 { &patterns[..100] } else { patterns };
    let teacher_slice = if teachers.len() > 100 { &teachers[..100] } else { teachers };
    if pat_slice.is_empty() && teacher_slice.is_empty() {
        return Ok(Vec::new());
    }

    let mut conditions = vec!["c.is_cancelled = 0".to_string()];
    let mut params: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(sd) = start_date {
        let d = safe_truncate(sd, 10);
        conditions.push("c.date_str >= ?".to_string());
        params.push(d.to_string().into());
    }

    if let Some(ed) = end_date {
        let d = safe_truncate(ed, 10);
        conditions.push("c.date_str <= ?".to_string());
        params.push(d.to_string().into());
    }

    let mut selection = Vec::new();
    if !pat_slice.is_empty() {
        let like_clauses = vec!["g.name LIKE ?"; pat_slice.len()].join(" OR ");
        for p in pat_slice {
            params.push(format!("%{}%", p).into());
        }
        selection.push(format!(
            "c.id IN (
                SELECT cg.course_id FROM course_groups cg
                JOIN groups g ON cg.group_id = g.id
                WHERE {}
            )",
            like_clauses
        ));
    }
    if !teacher_slice.is_empty() {
        let placeholders = vec!["?"; teacher_slice.len()].join(", ");
        for t in teacher_slice {
            params.push(t.trim().to_string().into());
        }
        selection.push(format!(
            "c.id IN (
                SELECT ct.course_id FROM course_teachers ct
                JOIN teachers t ON ct.teacher_id = t.id
                WHERE t.name COLLATE NOCASE IN ({})
            )",
            placeholders
        ));
    }
    conditions.push(format!("({})", selection.join(" OR ")));

    load_courses(conn, &conditions, &params)
}

/// Loads full courses (with teachers, rooms and groups) matching SQL conditions on `courses c`.
fn load_courses(
    conn: &Connection,
    conditions: &[String],
    params: &[rusqlite::types::Value],
) -> rusqlite::Result<Vec<Course>> {
    let sql = format!(
        "SELECT c.id, c.start, c.end, c.duration, c.course_name, c.course_code,
                c.activity_type, c.activity_name, c.is_exam, c.is_cancelled,
                c.status, c.description, c.searchable_text
         FROM courses c
         WHERE {}
         ORDER BY c.start ASC;",
        conditions.join(" AND ")
    );

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    struct PartialCourse {
        id: String,
        start: String,
        end: String,
        duration: i64,
        course_name: String,
        course_code: String,
        activity_type: String,
        activity_name: String,
        is_exam: bool,
        is_cancelled: bool,
        status: String,
        description: String,
        searchable_text: String,
    }

    let course_rows: Vec<PartialCourse> = stmt
        .query_map(&param_refs[..], |r| {
            Ok(PartialCourse {
                id: r.get(0)?,
                start: r.get(1)?,
                end: r.get(2)?,
                duration: r.get(3)?,
                course_name: r.get(4)?,
                course_code: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                activity_type: r.get(6)?,
                activity_name: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                is_exam: r.get::<_, i64>(8)? != 0,
                is_cancelled: r.get::<_, i64>(9)? != 0,
                status: r.get::<_, Option<String>>(10)?.unwrap_or_else(|| "PLANIFIE".to_string()),
                description: r.get::<_, Option<String>>(11)?.unwrap_or_default(),
                searchable_text: r.get::<_, Option<String>>(12)?.unwrap_or_default(),
            })
        })?
        .filter_map(Result::ok)
        .collect();

    if course_rows.is_empty() {
        return Ok(Vec::new());
    }

    let course_ids: Vec<String> = course_rows.iter().map(|c| c.id.clone()).collect();
    let mut teacher_map = fetch_course_teachers_batched(conn, &course_ids)?;
    let mut room_map = fetch_course_rooms_batched(conn, &course_ids)?;
    let mut group_map = fetch_course_groups_batched(conn, &course_ids)?;

    let result: Vec<Course> = course_rows
        .into_iter()
        .map(|r| {
            let cid = r.id.clone();
            let summary = format!("[{}] {}", r.activity_type, r.course_name);
            Course {
                id: r.id,
                start: r.start,
                end: r.end,
                duration: r.duration,
                summary,
                course_name: r.course_name,
                course_code: r.course_code,
                activity_type: r.activity_type,
                activity_name: r.activity_name,
                is_exam: r.is_exam,
                is_cancelled: r.is_cancelled,
                status: r.status,
                teachers: teacher_map.remove(&cid).unwrap_or_default(),
                rooms: room_map.remove(&cid).unwrap_or_default(),
                groups: group_map.remove(&cid).unwrap_or_default(),
                description: r.description,
                searchable_text: r.searchable_text,
            }
        })
        .collect();

    Ok(result)
}

/// Every room with its non-cancelled courses on `target_date` (YYYY-MM-DD),
/// sorted by start time. Powers the floor plan's day view.
pub fn query_rooms_day(conn: &Connection, target_date: &str) -> rusqlite::Result<serde_json::Value> {
    let target_d = safe_truncate(target_date, 10).to_string();

    let mut c_stmt = conn.prepare(
        "SELECT c.id, c.course_name, c.activity_type, c.start_time_str, c.end_time_str, cr.room_id
         FROM courses c
         JOIN course_rooms cr ON c.id = cr.course_id
         WHERE c.date_str = ?1 AND c.is_cancelled = 0
         ORDER BY c.start_time_str ASC;",
    )?;
    let day_courses: Vec<(String, String, String, String, String, i64)> = c_stmt
        .query_map(params![target_d], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })?
        .filter_map(Result::ok)
        .collect();

    let mut course_ids: Vec<String> = day_courses.iter().map(|c| c.0.clone()).collect();
    course_ids.sort();
    course_ids.dedup();
    let (teacher_map, group_map) = if course_ids.is_empty() {
        (HashMap::new(), HashMap::new())
    } else {
        (
            fetch_course_teachers_batched(conn, &course_ids)?,
            fetch_course_groups_batched(conn, &course_ids)?,
        )
    };

    let mut by_room: HashMap<i64, Vec<serde_json::Value>> = HashMap::new();
    for (id, name, kind, start, end, room_id) in &day_courses {
        by_room.entry(*room_id).or_default().push(json!({
            "title": name,
            "type": kind,
            "start": start,
            "end": end,
            "teachers": teacher_map.get(id).cloned().unwrap_or_default(),
            "groups": group_map.get(id).cloned().unwrap_or_default(),
        }));
    }

    let mut r_stmt = conn.prepare("SELECT id, name FROM rooms ORDER BY name COLLATE NOCASE ASC")?;
    let rooms: Vec<serde_json::Value> = r_stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .filter_map(Result::ok)
        .map(|(id, name)| json!({ "name": name, "courses": by_room.remove(&id).unwrap_or_default() }))
        .collect();

    Ok(json!({ "date": target_d, "rooms": rooms }))
}

/// Every teacher seen in a non-cancelled course, with how many courses they teach.
pub fn list_teachers(conn: &Connection) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT t.name, COUNT(c.id) AS n
         FROM teachers t
         JOIN course_teachers ct ON ct.teacher_id = t.id
         JOIN courses c ON c.id = ct.course_id AND c.is_cancelled = 0
         WHERE TRIM(t.name) != ''
         GROUP BY t.id
         ORDER BY t.name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// Every course (cancelled ones included) of the given groups, matched by exact
/// name, from `from_date` on. Lets a subscription link pick up what classmates'
/// logins brought into the shared database.
pub fn courses_of_groups(conn: &Connection, groups: &[String], from_date: &str) -> rusqlite::Result<Vec<Course>> {
    let groups: Vec<&String> = groups.iter().filter(|g| !g.trim().is_empty()).take(50).collect();
    if groups.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; groups.len()].join(", ");
    let mut params: Vec<rusqlite::types::Value> = vec![safe_truncate(from_date, 10).to_string().into()];
    params.extend(groups.iter().map(|g| rusqlite::types::Value::from(g.trim().to_string())));
    let conditions = vec![
        "c.date_str >= ?".to_string(),
        format!(
            "c.id IN (SELECT cg.course_id FROM course_groups cg JOIN groups g ON cg.group_id = g.id
                      WHERE g.name COLLATE NOCASE IN ({}))",
            placeholders
        ),
    ];
    load_courses(conn, &conditions, &params)
}
