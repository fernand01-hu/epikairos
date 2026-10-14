pub mod models;

use chrono::{NaiveDate, NaiveDateTime};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use regex::Regex;
use rusqlite::{params, Connection};
use serde_json::json;
use std::collections::{HashMap, HashSet};
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
            name TEXT UNIQUE COLLATE NOCASE,
            building TEXT,
            floor TEXT
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

        CREATE TABLE IF NOT EXISTS contributions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp REAL NOT NULL,
            course_count INTEGER NOT NULL,
            consent INTEGER DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS promotions (
            id TEXT PRIMARY KEY,
            code TEXT NOT NULL,
            name TEXT NOT NULL,
            cycle TEXT NOT NULL,
            year INTEGER NOT NULL,
            semesters TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS recurring_courses (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pattern_key TEXT UNIQUE NOT NULL,
            course_name TEXT NOT NULL,
            course_code TEXT,
            activity_type TEXT NOT NULL,
            activity_name TEXT,
            day_of_week INTEGER NOT NULL,
            day_name TEXT NOT NULL,
            start_time TEXT NOT NULL,
            end_time TEXT NOT NULL,
            duration INTEGER NOT NULL,
            promo_id TEXT,
            promo_name TEXT,
            subgroup TEXT,
            group_display TEXT,
            primary_teacher TEXT,
            all_teachers TEXT,
            primary_room TEXT,
            all_rooms TEXT,
            occurrences_count INTEGER DEFAULT 1,
            first_date TEXT,
            last_date TEXT,
            frequency TEXT,
            is_recurring INTEGER DEFAULT 1,
            confidence_score REAL DEFAULT 1.0,
            created_at REAL NOT NULL,
            updated_at REAL NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_courses_date ON courses(date_str);
        CREATE INDEX IF NOT EXISTS idx_courses_start ON courses(start);
        CREATE INDEX IF NOT EXISTS idx_courses_type ON courses(activity_type);
        DROP INDEX IF EXISTS idx_courses_slot;
        CREATE INDEX IF NOT EXISTS idx_courses_slot ON courses(start, end, course_name);
        CREATE INDEX IF NOT EXISTS idx_teachers_name ON teachers(name);
        CREATE INDEX IF NOT EXISTS idx_rooms_name ON rooms(name);
        CREATE INDEX IF NOT EXISTS idx_groups_name ON groups(name);
        CREATE INDEX IF NOT EXISTS idx_rec_day ON recurring_courses(day_of_week, start_time);
        CREATE INDEX IF NOT EXISTS idx_rec_promo ON recurring_courses(promo_id);
        CREATE INDEX IF NOT EXISTS idx_course_teachers_tid ON course_teachers(teacher_id);
        CREATE INDEX IF NOT EXISTS idx_course_rooms_rid ON course_rooms(room_id);
        CREATE INDEX IF NOT EXISTS idx_course_groups_gid ON course_groups(group_id);",
    )?;

    // Check dynamic columns
    let mut stmt = conn.prepare("PRAGMA table_info(courses)")?;
    let c_cols: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect();

    if !c_cols.contains(&"recurring_id".to_string()) {
        conn.execute(
            "ALTER TABLE courses ADD COLUMN recurring_id INTEGER REFERENCES recurring_courses(id);",
            [],
        )?;
    }
    if !c_cols.contains(&"pattern_key".to_string()) {
        conn.execute("ALTER TABLE courses ADD COLUMN pattern_key TEXT;", [])?;
    }

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

    Ok(())
}

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

        let date_str = safe_truncate(start, 10);
        let start_time_str = safe_slice(start, 11, 16).unwrap_or("00:00");
        let end_time_str = safe_slice(end, 11, 16).unwrap_or("23:59");

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
        tx.execute("DELETE FROM course_teachers WHERE course_id = ?1", params![actual_id])?;
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
        tx.execute("DELETE FROM course_rooms WHERE course_id = ?1", params![actual_id])?;
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
        tx.execute("DELETE FROM course_groups WHERE course_id = ?1", params![actual_id])?;
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

    tx.execute(
        "INSERT INTO contributions (timestamp, course_count, consent) VALUES (?1, ?2, ?3);",
        params![now, courses.len() as i64, 1],
    )?;

    tx.commit()?;

    // Sync recurring courses in background or inline
    let _ = detect_and_sync_recurring_courses(conn);

    Ok(json!({
        "success": true,
        "inserted": inserted_count,
        "updated": updated_count,
        "total": inserted_count + updated_count,
    }))
}

pub fn get_database_stats(conn: &Connection) -> rusqlite::Result<DatabaseStats> {
    let total_courses: i64 = conn.query_row("SELECT COUNT(*) FROM courses", [], |r| r.get(0))?;
    let total_teachers: i64 = conn.query_row("SELECT COUNT(*) FROM teachers", [], |r| r.get(0))?;
    let total_rooms: i64 = conn.query_row("SELECT COUNT(*) FROM rooms", [], |r| r.get(0))?;
    let total_groups: i64 = conn.query_row("SELECT COUNT(*) FROM groups", [], |r| r.get(0))?;
    let total_promos: i64 = conn.query_row("SELECT COUNT(*) FROM promotions", [], |r| r.get(0))?;
    let total_patterns: i64 = conn.query_row("SELECT COUNT(*) FROM recurring_courses", [], |r| r.get(0))?;
    let recurring_patterns: i64 = conn.query_row(
        "SELECT COUNT(*) FROM recurring_courses WHERE is_recurring = 1",
        [],
        |r| r.get(0),
    )?;

    let (min_date, max_date) = conn.query_row(
        "SELECT MIN(date_str), MAX(date_str) FROM courses",
        [],
        |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)),
    )?;

    let mut stmt = conn.prepare("SELECT activity_type, COUNT(*) FROM courses GROUP BY activity_type ORDER BY COUNT(*) DESC")?;
    let mut map = serde_json::Map::new();
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for item in rows.flatten() {
        map.insert(item.0, json!(item.1));
    }

    Ok(DatabaseStats {
        total_courses,
        total_teachers,
        total_rooms,
        total_groups,
        total_promotions: total_promos,
        total_patterns,
        recurring_patterns,
        min_date,
        max_date,
        types_breakdown: serde_json::Value::Object(map),
    })
}

pub fn get_all_teachers(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM teachers ORDER BY name COLLATE NOCASE ASC")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut set = HashSet::new();
    for name in rows.flatten() {
        let clean = sanitize_teacher_name(&name);
        if !clean.is_empty() {
            set.insert(clean);
        }
    }
    let mut list: Vec<String> = set.into_iter().collect();
    list.sort();
    Ok(list)
}

pub fn get_all_rooms(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM rooms ORDER BY name COLLATE NOCASE ASC")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut set = HashSet::new();
    for name in rows.flatten() {
        let clean = sanitize_room_name(&name);
        if !clean.is_empty() {
            set.insert(clean);
        }
    }
    let mut list: Vec<String> = set.into_iter().collect();
    list.sort();
    Ok(list)
}

pub fn get_all_groups(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM groups ORDER BY name COLLATE NOCASE ASC")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut list = Vec::new();
    for name in rows.flatten() {
        list.push(name);
    }
    Ok(list)
}

pub fn get_all_groups_structured(conn: &Connection) -> rusqlite::Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT g.id, g.name, g.clean_name, g.promo_id, g.subgroup, g.display_name,
                p.name AS promo_name, p.cycle, p.year
         FROM groups g
         LEFT JOIN promotions p ON g.promo_id = p.id
         ORDER BY p.year ASC, g.subgroup ASC;",
    )?;

    let rows = stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "name": r.get::<_, String>(1)?,
            "clean_name": r.get::<_, Option<String>>(2)?,
            "promo_id": r.get::<_, Option<String>>(3)?,
            "subgroup": r.get::<_, Option<String>>(4)?,
            "display_name": r.get::<_, Option<String>>(5)?,
            "promo_name": r.get::<_, Option<String>>(6)?,
            "cycle": r.get::<_, Option<String>>(7)?,
            "year": r.get::<_, Option<i32>>(8)?,
        }))
    })?;

    Ok(rows.filter_map(Result::ok).collect())
}

pub fn get_structured_promotions_and_groups(conn: &Connection) -> rusqlite::Result<Vec<serde_json::Value>> {
    let mut promo_stmt = conn.prepare("SELECT id, code, name, cycle, year, semesters FROM promotions ORDER BY year ASC")?;
    let promo_rows = promo_stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i32>(4)?,
            r.get::<_, String>(5)?,
        ))
    })?;

    let mut promo_map: HashMap<String, serde_json::Value> = HashMap::new();
    let mut promo_order: Vec<String> = Vec::new();

    for p in promo_rows.flatten() {
        promo_order.push(p.0.clone());
        promo_map.insert(
            p.0.clone(),
            json!({
                "id": p.0,
                "code": p.1,
                "name": p.2,
                "cycle": p.3,
                "year": p.4,
                "semesters": p.5,
                "classes": []
            }),
        );
    }

    let mut grp_stmt = conn.prepare(
        "SELECT g.id, g.name, g.clean_name, g.promo_id, g.subgroup, g.display_name,
                COUNT(cg.course_id) AS course_count
         FROM groups g
         LEFT JOIN course_groups cg ON g.id = cg.group_id
         GROUP BY g.id
         ORDER BY g.subgroup ASC;",
    )?;

    let mut other_classes = Vec::new();

    let grp_rows = grp_stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "name": r.get::<_, String>(1)?,
            "clean_name": r.get::<_, Option<String>>(2)?,
            "promo_id": r.get::<_, Option<String>>(3)?,
            "subgroup": r.get::<_, Option<String>>(4)?,
            "display_name": r.get::<_, Option<String>>(5)?,
            "course_count": r.get::<_, i64>(6)?,
        }))
    })?;

    for gr in grp_rows.flatten() {
        let pid = gr.get("promo_id").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(target_promo) = promo_map.get_mut(pid) {
            if let Some(classes) = target_promo.get_mut("classes").and_then(|v| v.as_array_mut()) {
                classes.push(gr);
            }
        } else {
            other_classes.push(gr);
        }
    }

    let mut result = Vec::new();
    for id in promo_order {
        if let Some(p) = promo_map.remove(&id) {
            result.push(p);
        }
    }

    if !other_classes.is_empty() {
        result.push(json!({
            "id": "autre",
            "code": "AUTRE",
            "name": "Autres Groupes",
            "cycle": "Divers",
            "year": 0,
            "semesters": "",
            "classes": other_classes
        }));
    }

    Ok(result)
}

pub fn get_all_activity_types(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT DISTINCT activity_type FROM courses ORDER BY activity_type ASC")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.filter_map(Result::ok).collect())
}

pub fn detect_and_sync_recurring_courses(conn: &mut Connection) -> rusqlite::Result<serde_json::Value> {

    struct CourseRow {
        id: String,
        course_name: String,
        course_code: String,
        activity_type: String,
        activity_name: String,
        start: String,
        start_time_str: String,
        end_time_str: String,
        duration: i64,
        teachers: Option<String>,
        rooms: Option<String>,
        groups: Option<String>,
    }

    let rows: Vec<CourseRow> = {
        let mut stmt = conn.prepare(
            "SELECT c.id, c.course_name, c.course_code, c.activity_type, c.activity_name,
                    c.start, c.end, c.start_time_str, c.end_time_str, c.duration,
                    GROUP_CONCAT(DISTINCT t.name) as teachers,
                    GROUP_CONCAT(DISTINCT r.name) as rooms,
                    GROUP_CONCAT(DISTINCT g.name) as groups
             FROM courses c
             LEFT JOIN course_teachers ct ON c.id = ct.course_id
             LEFT JOIN teachers t ON ct.teacher_id = t.id
             LEFT JOIN course_rooms cr ON c.id = cr.course_id
             LEFT JOIN rooms r ON cr.room_id = r.id
             LEFT JOIN course_groups cg ON c.id = cg.course_id
             LEFT JOIN groups g ON cg.group_id = g.id
             WHERE c.is_cancelled = 0
             GROUP BY c.id
             ORDER BY c.start ASC;",
        )?;

        let r = stmt
            .query_map([], |row| {
                Ok(CourseRow {
                    id: row.get(0)?,
                    course_name: row.get(1)?,
                    course_code: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    activity_type: row.get(3)?,
                    activity_name: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    start: row.get(5)?,
                    start_time_str: row.get(6)?,
                    end_time_str: row.get(7)?,
                    duration: row.get(8)?,
                    teachers: row.get(9)?,
                    rooms: row.get(10)?,
                    groups: row.get(11)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        r
    };

    let day_names = ["Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi", "Dimanche"];

    #[allow(dead_code)]
    struct PatternCollector {
        pattern_key: String,
        course_name: String,
        course_code: String,
        activity_type: String,
        activity_name: String,
        day_of_week: usize,
        day_name: String,
        start_time: String,
        end_time: String,
        duration: i64,
        promo_id: String,
        promo_name: String,
        subgroup: String,
        group_display: String,
        course_ids: Vec<String>,
        dates: Vec<String>,
        teachers: HashMap<String, usize>,
        rooms: HashMap<String, usize>,
    }

    let slug_re = Regex::new(r"[^a-zA-Z0-9]+").unwrap();
    let mut patterns: HashMap<String, PatternCollector> = HashMap::new();

    for r in rows {
        let clean_start = r.start.replace('Z', "").split('+').next().unwrap_or("").to_string();
        let dow = if let Ok(dt) = NaiveDateTime::parse_from_str(&clean_start, "%Y-%m-%dT%H:%M:%S") {
            dt.date().format("%u").to_string().parse::<usize>().unwrap_or(1) - 1
        } else if let Ok(d) = NaiveDate::parse_from_str(safe_truncate(&clean_start, 10), "%Y-%m-%d") {
            d.format("%u").to_string().parse::<usize>().unwrap_or(1) - 1
        } else {
            0
        };

        let dow_idx = dow.min(6);
        let dname = day_names[dow_idx].to_string();

        let cname = r.course_name.trim().to_string();
        let act_type = r.activity_type.trim().to_uppercase();
        let st = if !r.start_time_str.trim().is_empty() { r.start_time_str.trim().to_string() } else { "08:30".to_string() };
        let et = if !r.end_time_str.trim().is_empty() { r.end_time_str.trim().to_string() } else { "10:00".to_string() };
        let dur = if r.duration > 0 { r.duration } else { 90 };

        let raw_groups: Vec<&str> = r.groups.as_deref().unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        let first_grp = raw_groups.first().copied().unwrap_or("");
        let grp_info = parse_group_identity(first_grp);

        let clean_slug = slug_re.replace_all(&cname.to_lowercase(), "_").trim_matches('_').to_string();
        let pattern_key = format!(
            "{}_{}_d{}_{}_{}_{}_{}",
            clean_slug,
            act_type.to_lowercase(),
            dow_idx,
            st.replace(':', ""),
            et.replace(':', ""),
            grp_info.promo_id,
            grp_info.subgroup.to_lowercase()
        );

        let entry = patterns.entry(pattern_key.clone()).or_insert_with(|| PatternCollector {
            pattern_key,
            course_name: cname,
            course_code: r.course_code,
            activity_type: act_type,
            activity_name: r.activity_name,
            day_of_week: dow_idx,
            day_name: dname,
            start_time: st,
            end_time: et,
            duration: dur,
            promo_id: grp_info.promo_id,
            promo_name: grp_info.promo_name,
            subgroup: grp_info.subgroup,
            group_display: grp_info.clean_name,
            course_ids: Vec::new(),
            dates: Vec::new(),
            teachers: HashMap::new(),
            rooms: HashMap::new(),
        });

        entry.course_ids.push(r.id);
        let d_part = safe_truncate(&clean_start, 10).to_string();
        entry.dates.push(d_part);

        if let Some(t_str) = r.teachers {
            for t in t_str.split(',') {
                let clean_t = sanitize_teacher_name(t);
                if !clean_t.is_empty() {
                    *entry.teachers.entry(clean_t).or_insert(0) += 1;
                }
            }
        }

        if let Some(rm_str) = r.rooms {
            for rm in rm_str.split(',') {
                let clean_rm = sanitize_room_name(rm);
                if !clean_rm.is_empty() {
                    *entry.rooms.entry(clean_rm).or_insert(0) += 1;
                }
            }
        }
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let mut saved_patterns = 0;
    let mut recurring_slots = 0;

    let tx = conn.transaction()?;

    for (p_key, p_data) in patterns {
        let occ_count = p_data.course_ids.len();
        let mut sorted_dates = p_data.dates.clone();
        sorted_dates.sort();
        let first_d = sorted_dates.first().cloned().unwrap_or_default();
        let last_d = sorted_dates.last().cloned().unwrap_or_default();

        let (frequency, is_rec) = if occ_count >= 3 {
            recurring_slots += 1;
            let mut intervals = Vec::new();
            for i in 0..sorted_dates.len().saturating_sub(1) {
                if let (Ok(d1), Ok(d2)) = (
                    NaiveDate::parse_from_str(&sorted_dates[i], "%Y-%m-%d"),
                    NaiveDate::parse_from_str(&sorted_dates[i + 1], "%Y-%m-%d"),
                ) {
                    intervals.push((d2 - d1).num_days());
                }
            }
            intervals.sort();
            let median = if !intervals.is_empty() { intervals[intervals.len() / 2] } else { 7 };
            let freq = if (5..=9).contains(&median) {
                "Hebdomadaire"
            } else if (11..=16).contains(&median) {
                "Bi-hebdomadaire"
            } else {
                "Périodique"
            };
            (freq.to_string(), 1)
        } else if occ_count == 2 {
            recurring_slots += 1;
            ("Bi-hebdomadaire / Périodique".to_string(), 1)
        } else {
            ("Ponctuel".to_string(), 0)
        };

        // Primary teacher & room
        let mut t_vec: Vec<(String, usize)> = p_data.teachers.into_iter().collect();
        t_vec.sort_by(|a, b| b.1.cmp(&a.1));
        let primary_teacher = t_vec.first().map(|x| x.0.clone()).unwrap_or_default();
        let all_teachers = t_vec.iter().map(|x| x.0.as_str()).collect::<Vec<&str>>().join(", ");

        let mut r_vec: Vec<(String, usize)> = p_data.rooms.into_iter().collect();
        r_vec.sort_by(|a, b| b.1.cmp(&a.1));
        let primary_room = r_vec.first().map(|x| x.0.clone()).unwrap_or_default();
        let all_rooms = r_vec.iter().map(|x| x.0.as_str()).collect::<Vec<&str>>().join(", ");

        let confidence = (0.5 + 0.1 * occ_count as f64).min(1.0);

        tx.execute(
            "INSERT INTO recurring_courses (
                pattern_key, course_name, course_code, activity_type, activity_name,
                day_of_week, day_name, start_time, end_time, duration,
                promo_id, promo_name, subgroup, group_display,
                primary_teacher, all_teachers, primary_room, all_rooms,
                occurrences_count, first_date, last_date, frequency,
                is_recurring, confidence_score, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)
            ON CONFLICT(pattern_key) DO UPDATE SET
                course_name = excluded.course_name,
                course_code = excluded.course_code,
                activity_type = excluded.activity_type,
                activity_name = excluded.activity_name,
                day_of_week = excluded.day_of_week,
                day_name = excluded.day_name,
                start_time = excluded.start_time,
                end_time = excluded.end_time,
                duration = excluded.duration,
                promo_id = excluded.promo_id,
                promo_name = excluded.promo_name,
                subgroup = excluded.subgroup,
                group_display = excluded.group_display,
                primary_teacher = excluded.primary_teacher,
                all_teachers = excluded.all_teachers,
                primary_room = excluded.primary_room,
                all_rooms = excluded.all_rooms,
                occurrences_count = excluded.occurrences_count,
                first_date = excluded.first_date,
                last_date = excluded.last_date,
                frequency = excluded.frequency,
                is_recurring = excluded.is_recurring,
                updated_at = excluded.updated_at;",
            params![
                p_key, p_data.course_name, p_data.course_code, p_data.activity_type, p_data.activity_name,
                p_data.day_of_week as i64, p_data.day_name, p_data.start_time, p_data.end_time, p_data.duration,
                p_data.promo_id, p_data.promo_name, p_data.subgroup, p_data.group_display,
                primary_teacher, all_teachers, primary_room, all_rooms,
                occ_count as i64, first_d, last_d, frequency,
                is_rec, confidence, now, now
            ],
        )?;
        saved_patterns += 1;

        if let Ok(rec_id) = tx.query_row::<i64, _, _>(
            "SELECT id FROM recurring_courses WHERE pattern_key = ?1",
            params![p_key],
            |r| r.get(0),
        ) {
            for cid in p_data.course_ids {
                let _ = tx.execute(
                    "UPDATE courses SET recurring_id = ?1, pattern_key = ?2 WHERE id = ?3",
                    params![rec_id, p_key, cid],
                );
            }
        }
    }

    tx.commit()?;

    Ok(json!({
        "success": true,
        "total_patterns": saved_patterns,
        "recurring_slots": recurring_slots,
        "single_slots": saved_patterns - recurring_slots,
    }))
}

pub fn query_recurring_courses(
    conn: &Connection,
    promo_id: Option<&str>,
    subgroup: Option<&str>,
    day_of_week: Option<i64>,
    teacher: Option<&str>,
    keyword: Option<&str>,
) -> rusqlite::Result<serde_json::Value> {
    let mut conditions = vec!["1=1".to_string()];
    let mut params: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(p) = promo_id {
        conditions.push("promo_id = ?".to_string());
        params.push(p.to_string().into());
    }

    if let Some(sg) = subgroup {
        conditions.push("subgroup = ?".to_string());
        params.push(sg.to_string().into());
    }

    if let Some(dow) = day_of_week {
        conditions.push("day_of_week = ?".to_string());
        params.push(dow.into());
    }

    if let Some(t) = teacher {
        let t_clean = t.trim();
        if !t_clean.is_empty() {
            conditions.push("(primary_teacher LIKE ? OR all_teachers LIKE ?)".to_string());
            let pattern = format!("%{}%", t_clean);
            params.push(pattern.clone().into());
            params.push(pattern.into());
        }
    }

    if let Some(k) = keyword {
        let k_clean = k.trim();
        if !k_clean.is_empty() {
            conditions.push("(course_name LIKE ? OR group_display LIKE ?)".to_string());
            let pattern = format!("%{}%", k_clean);
            params.push(pattern.clone().into());
            params.push(pattern.into());
        }
    }

    let sql = format!(
        "SELECT id, pattern_key, course_name, course_code, activity_type, activity_name,
                day_of_week, day_name, start_time, end_time, duration, promo_id, promo_name,
                subgroup, group_display, primary_teacher, all_teachers, primary_room, all_rooms,
                occurrences_count, first_date, last_date, frequency, is_recurring, confidence_score
         FROM recurring_courses
         WHERE {}
         ORDER BY day_of_week ASC, start_time ASC;",
        conditions.join(" AND ")
    );

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    let rows: Vec<serde_json::Value> = stmt
        .query_map(&param_refs[..], |r| {
            Ok(json!({
                "id": r.get::<_, i64>(0)?,
                "pattern_key": r.get::<_, String>(1)?,
                "course_name": r.get::<_, String>(2)?,
                "course_code": r.get::<_, Option<String>>(3)?,
                "activity_type": r.get::<_, String>(4)?,
                "activity_name": r.get::<_, Option<String>>(5)?,
                "day_of_week": r.get::<_, i64>(6)?,
                "day_name": r.get::<_, String>(7)?,
                "start_time": r.get::<_, String>(8)?,
                "end_time": r.get::<_, String>(9)?,
                "duration": r.get::<_, i64>(10)?,
                "promo_id": r.get::<_, Option<String>>(11)?,
                "promo_name": r.get::<_, Option<String>>(12)?,
                "subgroup": r.get::<_, Option<String>>(13)?,
                "group_display": r.get::<_, Option<String>>(14)?,
                "primary_teacher": r.get::<_, Option<String>>(15)?,
                "all_teachers": r.get::<_, Option<String>>(16)?,
                "primary_room": r.get::<_, Option<String>>(17)?,
                "all_rooms": r.get::<_, Option<String>>(18)?,
                "occurrences_count": r.get::<_, i64>(19)?,
                "first_date": r.get::<_, Option<String>>(20)?,
                "last_date": r.get::<_, Option<String>>(21)?,
                "frequency": r.get::<_, Option<String>>(22)?,
                "is_recurring": r.get::<_, i64>(23)?,
                "confidence_score": r.get::<_, f64>(24)?,
            }))
        })?
        .filter_map(Result::ok)
        .collect();

    let day_names = ["Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi"];
    let mut weekly_grid: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    for d in day_names {
        weekly_grid.insert(d.to_string(), Vec::new());
    }

    for item in &rows {
        let dname = item.get("day_name").and_then(|v| v.as_str()).unwrap_or("Lundi");
        if let Some(list) = weekly_grid.get_mut(dname) {
            list.push(item.clone());
        }
    }

    let count = rows.len();
    Ok(json!({
        "success": true,
        "patterns": rows,
        "weekly_grid": weekly_grid,
        "count": count
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

pub fn query_custom_courses(
    conn: &Connection,
    teachers: Option<&[String]>,
    rooms: Option<&[String]>,
    groups: Option<&[String]>,
    activity_types: Option<&[String]>,
    keyword: Option<&str>,
    start_date: Option<&str>,
    end_date: Option<&str>,
    hide_cancelled: bool,
) -> rusqlite::Result<Vec<Course>> {
    let mut conditions = vec!["1=1".to_string()];
    let mut params: Vec<rusqlite::types::Value> = Vec::new();

    if hide_cancelled {
        conditions.push("c.is_cancelled = 0".to_string());
    }

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

    if let Some(types) = activity_types {
        let types_slice = if types.len() > 100 { &types[..100] } else { types };
        if !types_slice.is_empty() {
            let placeholders = types_slice.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            conditions.push(format!("c.activity_type IN ({})", placeholders));
            for t in types_slice {
                params.push(t.to_uppercase().into());
            }
        }
    }

    if let Some(t_list) = teachers {
        let t_slice = if t_list.len() > 100 { &t_list[..100] } else { t_list };
        if !t_slice.is_empty() {
            let placeholders = t_slice.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            conditions.push(format!(
                "c.id IN (
                    SELECT ct.course_id FROM course_teachers ct
                    JOIN teachers t ON ct.teacher_id = t.id
                    WHERE t.name IN ({})
                )",
                placeholders
            ));
            for t in t_slice {
                params.push(t.clone().into());
            }
        }
    }

    if let Some(r_list) = rooms {
        let r_slice = if r_list.len() > 100 { &r_list[..100] } else { r_list };
        if !r_slice.is_empty() {
            let placeholders = r_slice.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            conditions.push(format!(
                "c.id IN (
                    SELECT cr.course_id FROM course_rooms cr
                    JOIN rooms r ON cr.room_id = r.id
                    WHERE r.name IN ({})
                )",
                placeholders
            ));
            for r in r_slice {
                params.push(r.clone().into());
            }
        }
    }

    if let Some(g_list) = groups {
        let g_slice = if g_list.len() > 100 { &g_list[..100] } else { g_list };
        if !g_slice.is_empty() {
            let placeholders = g_slice.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            conditions.push(format!(
                "c.id IN (
                    SELECT cg.course_id FROM course_groups cg
                    JOIN groups g ON cg.group_id = g.id
                    WHERE g.name IN ({})
                )",
                placeholders
            ));
            for g in g_slice {
                params.push(g.clone().into());
            }
        }
    }

    if let Some(k) = keyword {
        let k_clean = k.trim();
        if !k_clean.is_empty() {
            conditions.push("(c.course_name LIKE ? OR c.searchable_text LIKE ? OR c.description LIKE ?)".to_string());
            let pat = format!("%{}%", k_clean);
            params.push(pat.clone().into());
            params.push(pat.clone().into());
            params.push(pat.into());
        }
    }

    let sql = format!(
        "SELECT c.id, c.start, c.end, c.duration, c.course_name, c.course_code,
                c.activity_type, c.activity_name, c.is_exam, c.is_cancelled,
                c.status, c.description, c.searchable_text
         FROM courses c
         WHERE {}
         ORDER BY c.start ASC
         LIMIT 2000;",
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

pub fn query_courses_by_groups(
    conn: &Connection,
    patterns: &[String],
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> rusqlite::Result<Vec<Course>> {
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

    let pat_slice = if patterns.len() > 100 { &patterns[..100] } else { patterns };
    if !pat_slice.is_empty() {
        let mut like_clauses = Vec::new();
        for p in pat_slice {
            like_clauses.push("g.name LIKE ?".to_string());
            params.push(format!("%{}%", p).into());
        }
        let group_filter = like_clauses.join(" OR ");
        conditions.push(format!(
            "c.id IN (
                SELECT cg.course_id FROM course_groups cg
                JOIN groups g ON cg.group_id = g.id
                WHERE {}
            )",
            group_filter
        ));
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_safe_truncate_ascii() {
        assert_eq!(safe_truncate("hello world", 5), "hello");
        assert_eq!(safe_truncate("hello", 10), "hello");
        assert_eq!(safe_truncate("hello", 5), "hello");
        assert_eq!(safe_truncate("hello", 0), "");
        assert_eq!(safe_truncate("", 5), "");
    }

    #[test]
    fn test_safe_truncate_utf8_multibyte() {
        // "É" is 2 bytes: 0xC3 0x89
        let french = "ÉpiKaïros";
        assert_eq!(safe_truncate(french, 1), ""); // Boundary falls inside "É" -> truncates to 0
        assert_eq!(safe_truncate(french, 2), "É");
        assert_eq!(safe_truncate(french, 3), "Ép");

        // "🦀" is 4 bytes: 0xF0 0x9F 0xA6 0x80
        let crab = "🦀 Crab";
        assert_eq!(safe_truncate(crab, 1), "");
        assert_eq!(safe_truncate(crab, 2), "");
        assert_eq!(safe_truncate(crab, 3), "");
        assert_eq!(safe_truncate(crab, 4), "🦀");
        assert_eq!(safe_truncate(crab, 5), "🦀 ");
        assert_eq!(safe_truncate(crab, 10), "🦀 Crab");
    }

    #[test]
    fn test_safe_slice() {
        let ts = "2026-10-14T08:58:09";
        assert_eq!(safe_slice(ts, 0, 10), Some("2026-10-14"));
        assert_eq!(safe_slice(ts, 11, 16), Some("08:58"));
        assert_eq!(safe_slice(ts, 11, 25), None); // Out of bounds
        assert_eq!(safe_slice(ts, 15, 10), None); // Start > End

        // Multibyte string
        let emoji = "🦀🚀"; // Each emoji is 4 bytes
        assert_eq!(safe_slice(emoji, 0, 4), Some("🦀"));
        assert_eq!(safe_slice(emoji, 4, 8), Some("🚀"));
        assert_eq!(safe_slice(emoji, 0, 2), None); // Invalid boundary
        assert_eq!(safe_slice(emoji, 2, 6), None); // Invalid boundary
    }

    #[test]
    fn test_query_chunking_secondary_queries() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        // Generate 850 course IDs to test chunking across batches of 400 (400 + 400 + 50)
        let total_courses = 850;
        let mut courses = Vec::new();
        for i in 0..total_courses {
            courses.push(Course {
                id: format!("course_{:04}", i),
                start: "2026-10-14T09:00:00".to_string(),
                end: "2026-10-14T10:30:00".to_string(),
                duration: 90,
                summary: format!("Course {}", i),
                course_name: format!("Maths {:04}", i),
                course_code: "MATH".to_string(),
                activity_type: "COURS".to_string(),
                activity_name: "Cours magistral".to_string(),
                is_exam: false,
                is_cancelled: false,
                status: "PLANIFIE".to_string(),
                teachers: vec![format!("Prof {:04}", i % 10)],
                rooms: vec![format!("Room {:04}", i % 5)],
                groups: vec![format!("G{:04}", i % 20)],
                description: "Test description".to_string(),
                searchable_text: "".to_string(),
            });
        }

        ingest_courses(&mut conn, &courses, true).unwrap();

        let all_ids: Vec<String> = (0..total_courses).map(|i| format!("course_{:04}", i)).collect();
        assert_eq!(all_ids.len(), 850);

        // Fetch teachers, rooms, groups using batched helpers
        let teacher_map = fetch_course_teachers_batched(&conn, &all_ids).unwrap();
        let room_map = fetch_course_rooms_batched(&conn, &all_ids).unwrap();
        let group_map = fetch_course_groups_batched(&conn, &all_ids).unwrap();

        assert_eq!(teacher_map.len(), total_courses);
        assert_eq!(room_map.len(), total_courses);
        assert_eq!(group_map.len(), total_courses);

        for (i, id) in all_ids.iter().enumerate() {
            assert_eq!(teacher_map.get(id).unwrap(), &[format!("Prof {:04}", i % 10)]);
            assert_eq!(room_map.get(id).unwrap(), &[format!("Room {:04}", i % 5)]);
            assert_eq!(group_map.get(id).unwrap(), &[format!("G{:04}", i % 20)]);
        }
    }

    #[test]
    fn test_cross_cohort_no_overwrite_and_non_unique_index() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        // Two courses with the same slot and course name, but different cohorts/IDs
        let course1 = Course {
            id: "auriga_g1_algo".to_string(),
            start: "2026-10-14T09:00:00".to_string(),
            end: "2026-10-14T10:30:00".to_string(),
            duration: 90,
            summary: "Algo G1".to_string(),
            course_name: "Algorithmique".to_string(),
            course_code: "ALGO".to_string(),
            activity_type: "COURS".to_string(),
            activity_name: "Cours".to_string(),
            is_exam: false,
            is_cancelled: false,
            status: "PLANIFIE".to_string(),
            teachers: vec!["Teacher A".to_string()],
            rooms: vec!["Room 101".to_string()],
            groups: vec!["SUP-1".to_string()],
            description: "SUP group".to_string(),
            searchable_text: "".to_string(),
        };

        let course2 = Course {
            id: "auriga_g2_algo".to_string(),
            start: "2026-10-14T09:00:00".to_string(),
            end: "2026-10-14T10:30:00".to_string(),
            duration: 90,
            summary: "Algo G2".to_string(),
            course_name: "Algorithmique".to_string(),
            course_code: "ALGO".to_string(),
            activity_type: "COURS".to_string(),
            activity_name: "Cours".to_string(),
            is_exam: false,
            is_cancelled: false,
            status: "PLANIFIE".to_string(),
            teachers: vec!["Teacher B".to_string()],
            rooms: vec!["Room 102".to_string()],
            groups: vec!["SPE-1".to_string()],
            description: "SPE group".to_string(),
            searchable_text: "".to_string(),
        };

        ingest_courses(&mut conn, &[course1], true).unwrap();
        ingest_courses(&mut conn, &[course2], true).unwrap();

        // Verify both courses exist independently
        let courses = query_custom_courses(&conn, None, None, None, None, None, None, None, false).unwrap();
        assert_eq!(courses.len(), 2);

        let c1 = courses.iter().find(|c| c.id == "auriga_g1_algo").expect("course 1 must exist");
        let c2 = courses.iter().find(|c| c.id == "auriga_g2_algo").expect("course 2 must exist");

        assert_eq!(c1.groups, vec!["SUP-1"]);
        assert_eq!(c1.teachers, vec!["Teacher A"]);
        assert_eq!(c1.rooms, vec!["Room 101"]);

        assert_eq!(c2.groups, vec!["SPE-1"]);
        assert_eq!(c2.teachers, vec!["Teacher B"]);
        assert_eq!(c2.rooms, vec!["Room 102"]);
    }

    #[test]
    fn test_custom_courses_limit_and_activity_types_cap() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        // Verify cap on activity_types (pass > 100 activity types)
        let many_types: Vec<String> = (0..150).map(|i| format!("TYPE_{}", i)).collect();
        let res = query_custom_courses(&conn, None, None, None, Some(&many_types), None, None, None, false);
        assert!(res.is_ok());

        // Verify default LIMIT 2000
        let mut courses = Vec::new();
        for i in 0..2100 {
            courses.push(Course {
                id: format!("c_{}", i),
                start: format!("2026-10-14T{:02}:{:02}:00", (i / 60) % 24, i % 60),
                end: format!("2026-10-14T{:02}:{:02}:00", (i / 60) % 24, (i % 60) + 1),
                duration: 60,
                summary: format!("C {}", i),
                course_name: format!("Course {}", i),
                course_code: "".to_string(),
                activity_type: "COURS".to_string(),
                activity_name: "".to_string(),
                is_exam: false,
                is_cancelled: false,
                status: "PLANIFIE".to_string(),
                teachers: vec![],
                rooms: vec![],
                groups: vec![],
                description: "".to_string(),
                searchable_text: "".to_string(),
            });
        }
        ingest_courses(&mut conn, &courses, true).unwrap();

        let queried = query_custom_courses(&conn, None, None, None, None, None, None, None, false).unwrap();
        assert_eq!(queried.len(), 2000);
    }

    #[test]
    fn test_init_db_drops_unique_index_and_creates_reverse_indices() {
        let conn = Connection::open_in_memory().unwrap();

        // 1. Manually create tables and a UNIQUE idx_courses_slot to simulate legacy schema
        conn.execute_batch(
            "CREATE TABLE courses (
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
            CREATE UNIQUE INDEX idx_courses_slot ON courses(start, end, course_name);",
        ).unwrap();

        // Verify it was unique
        let is_unique: bool = conn.query_row(
            "SELECT \"unique\" FROM pragma_index_list('courses') WHERE name = 'idx_courses_slot'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(is_unique);

        // 2. Run init_db
        init_db(&conn).unwrap();

        // 3. Verify idx_courses_slot is now non-unique
        let is_unique_after: bool = conn.query_row(
            "SELECT \"unique\" FROM pragma_index_list('courses') WHERE name = 'idx_courses_slot'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(!is_unique_after);

        // 4. Verify reverse indices exist
        let t_idx: bool = conn.query_row(
            "SELECT 1 FROM pragma_index_list('course_teachers') WHERE name = 'idx_course_teachers_tid'",
            [],
            |_| Ok(true),
        ).unwrap();
        assert!(t_idx);

        let r_idx: bool = conn.query_row(
            "SELECT 1 FROM pragma_index_list('course_rooms') WHERE name = 'idx_course_rooms_rid'",
            [],
            |_| Ok(true),
        ).unwrap();
        assert!(r_idx);

        let g_idx: bool = conn.query_row(
            "SELECT 1 FROM pragma_index_list('course_groups') WHERE name = 'idx_course_groups_gid'",
            [],
            |_| Ok(true),
        ).unwrap();
        assert!(g_idx);
    }
}
