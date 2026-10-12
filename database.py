#!/usr/bin/env python3
"""
Database Layer for EPITA Auriga Calendar Knowledge Base
=======================================================
Stores shared courses, rooms, teachers, and groups with user consent.
Powers custom calendar generation and real-time free room search.
"""

import os
import re
import sqlite3
import time
from typing import Any, Dict, List, Optional, Tuple

DB_PATH = os.getenv("SCHOOL_DB_PATH", "school.db")


def get_db_connection(db_path: str = DB_PATH) -> sqlite3.Connection:
    """Returns a SQLite connection with row factory and foreign keys enabled."""
    conn = sqlite3.connect(db_path, timeout=30.0)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON;")
    return conn


EPITA_PROMOTIONS_DEF = [
    {
        "id": "sup_lyon",
        "code": "SUP",
        "name": "SUP Lyon (1ère année)",
        "cycle": "Classes Préparatoires",
        "year": 1,
        "semesters": "S1/S2",
    },
    {
        "id": "spe_lyon",
        "code": "SPE",
        "name": "SPÉ Lyon (2ème année)",
        "cycle": "Classes Préparatoires",
        "year": 2,
        "semesters": "S3/S4",
    },
    {
        "id": "ing1_lyon",
        "code": "ING1",
        "name": "ING1 Lyon (Cycle Ingénieur)",
        "cycle": "Cycle Ingénieur",
        "year": 3,
        "semesters": "SI5/SI6",
    },
    {
        "id": "ing2_lyon",
        "code": "ING2",
        "name": "ING2 Lyon",
        "cycle": "Cycle Ingénieur",
        "year": 4,
        "semesters": "SI7/SI8",
    },
    {
        "id": "ing3_lyon",
        "code": "ING3",
        "name": "ING3 / Majeures Lyon",
        "cycle": "Cycle Ingénieur",
        "year": 5,
        "semesters": "SI9/SI10",
    },
    {
        "id": "autre",
        "code": "AUTRE",
        "name": "Autres Groupes",
        "cycle": "Divers",
        "year": 99,
        "semesters": "",
    },
]


def sanitize_room_name(name: str) -> str:
    """Removes floor / level / étage indicators from room names."""
    if not name:
        return ""
    # Strip (Xe étage), (X étage), - Xe étage, or (RDC)
    cleaned = re.sub(r'[\s\-_]*\(\s*\d+e?\s*étage\s*\)', '', name, flags=re.I)
    cleaned = re.sub(r'[\s\-_]*\d+e?\s*étage', '', cleaned, flags=re.I)
    cleaned = re.sub(r'\s*\(\s*RDC\s*\)', '', cleaned, flags=re.I)
    cleaned = cleaned.strip()
    return cleaned if cleaned else name.strip()


def sanitize_teacher_name(name: str) -> str:
    """Sanitizes teacher names by removing multiline remnants, groups, or room tags."""
    if not name:
        return ""
    first_part = name.replace("\\n", "\n").split("\n")[0].strip()
    cleaned = re.sub(r'(Groupe|Salle|Statut)\(s\):.*', '', first_part, flags=re.I).strip()
    return cleaned if cleaned else name.strip()


def parse_group_identity(raw_name: str) -> Dict[str, Any]:
    """
    Parses and recognizes EPITA promotion, cycle, year, and class subgroup from raw group names.
    e.g. '2627 - S3 Lyon L3' -> Promo SPÉ Lyon, Classe L3.
         '2627 - S1 Lyon' -> Promo SUP Lyon, Promotion Complète.
         '2627 - SI9 Lyon SCIA-G' -> Promo ING3 Lyon, Majeure SCIA Graphes.
    """
    n = (raw_name or "").strip()
    upper = n.upper()

    # Detect promotion
    if "S1" in upper or "S2" in upper or "SUP" in upper:
        promo = EPITA_PROMOTIONS_DEF[0]
    elif "S3" in upper or "S4" in upper or "SPE" in upper or "SPÉ" in upper:
        promo = EPITA_PROMOTIONS_DEF[1]
    elif "SI5" in upper or "SI6" in upper or "ING1" in upper or "S5" in upper:
        promo = EPITA_PROMOTIONS_DEF[2]
    elif "SI7" in upper or "SI8" in upper or "ING2" in upper or "S7" in upper:
        promo = EPITA_PROMOTIONS_DEF[3]
    elif "SI9" in upper or "SI10" in upper or "ING3" in upper or "SCIA" in upper or "RDI" in upper or "SANTE" in upper or "IF" in upper or "S9" in upper:
        promo = EPITA_PROMOTIONS_DEF[4]
    else:
        promo = EPITA_PROMOTIONS_DEF[5]

    # Detect subgroup (L1, L2, L3, Major, or Full Promo)
    subgroup = "Promo"
    display = "Promotion Complète (Tous)"
    l_match = re.search(r'\b(L[1-9])\b', upper)
    if l_match:
        subgroup = l_match.group(1)
        display = f"Classe {subgroup}"
    elif "SCIA" in upper:
        subgroup = "SCIA-G"
        display = "Majeure SCIA Graphes"
    elif "RDI" in upper:
        subgroup = "RDI-L"
        display = "Majeure RDI-L"
    elif "SANTE" in upper:
        subgroup = "SANTE"
        display = "Majeure Santé"
    elif "IF" in upper:
        subgroup = "IF"
        display = "Majeure Ingénierie Financière"

    clean_name = f"{promo['name']} - {display}"

    return {
        "raw_name": n,
        "promo_id": promo["id"],
        "promo_code": promo["code"],
        "promo_name": promo["name"],
        "cycle": promo["cycle"],
        "year": promo["year"],
        "semesters": promo["semesters"],
        "subgroup": subgroup,
        "display_name": display,
        "clean_name": clean_name,
        "is_promo_wide": subgroup == "Promo",
    }


def init_db(db_path: str = DB_PATH) -> None:
    """Initializes tables, migrations, and indexes for the school calendar database."""
    conn = get_db_connection(db_path)
    cur = conn.cursor()

    cur.executescript("""
        CREATE TABLE IF NOT EXISTS courses (
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
        CREATE UNIQUE INDEX IF NOT EXISTS idx_courses_slot ON courses(start, end, course_name);
        CREATE INDEX IF NOT EXISTS idx_teachers_name ON teachers(name);
        CREATE INDEX IF NOT EXISTS idx_rooms_name ON rooms(name);
        CREATE INDEX IF NOT EXISTS idx_groups_name ON groups(name);
        CREATE INDEX IF NOT EXISTS idx_rec_day ON recurring_courses(day_of_week, start_time);
        CREATE INDEX IF NOT EXISTS idx_rec_promo ON recurring_courses(promo_id);
    """)

    # Dynamic migrations: ensure columns exist
    cur.execute("PRAGMA table_info(courses);")
    c_cols = [r["name"] for r in cur.fetchall()]
    if "recurring_id" not in c_cols:
        cur.execute("ALTER TABLE courses ADD COLUMN recurring_id INTEGER REFERENCES recurring_courses(id);")
    if "pattern_key" not in c_cols:
        cur.execute("ALTER TABLE courses ADD COLUMN pattern_key TEXT;")

    cur.execute("PRAGMA table_info(groups);")
    g_cols = [r["name"] for r in cur.fetchall()]
    if "clean_name" not in g_cols:
        cur.execute("ALTER TABLE groups ADD COLUMN clean_name TEXT;")
    if "promo_id" not in g_cols:
        cur.execute("ALTER TABLE groups ADD COLUMN promo_id TEXT REFERENCES promotions(id);")
    if "subgroup" not in g_cols:
        cur.execute("ALTER TABLE groups ADD COLUMN subgroup TEXT;")
    if "display_name" not in g_cols:
        cur.execute("ALTER TABLE groups ADD COLUMN display_name TEXT;")

    # Populate promotions table
    for p in EPITA_PROMOTIONS_DEF:
        cur.execute("""
            INSERT INTO promotions (id, code, name, cycle, year, semesters)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                code = excluded.code,
                name = excluded.name,
                cycle = excluded.cycle,
                year = excluded.year,
                semesters = excluded.semesters;
        """, (p["id"], p["code"], p["name"], p["cycle"], p["year"], p["semesters"]))

    conn.commit()
    conn.close()


def ingest_courses(courses: List[Dict[str, Any]], consent: bool = True, db_path: str = DB_PATH) -> Dict[str, Any]:
    """
    Ingests or updates a batch of normalized courses into the relational database.
    Requires user consent.
    """
    if not consent:
        return {"success": False, "reason": "Consent not given", "inserted": 0, "updated": 0}

    if not courses:
        return {"success": True, "inserted": 0, "updated": 0}

    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()

    now = time.time()
    inserted_count = 0
    updated_count = 0

    try:
        cur.execute("BEGIN TRANSACTION;")

        for c in courses:
            c_id = str(c.get("id") or "")
            start = str(c.get("start") or "")
            end = str(c.get("end") or "")

            if not c_id or not start or not end:
                continue

            date_str = start[:10]  # YYYY-MM-DD
            start_time_str = start[11:16] if len(start) >= 16 else "00:00"
            end_time_str = end[11:16] if len(end) >= 16 else "23:59"

            course_name = c.get("course_name") or "Cours"
            course_code = c.get("course_code") or ""
            activity_type = (c.get("activity_type") or "AUTRE").strip().upper()
            activity_name = c.get("activity_name") or ""
            is_exam = 1 if c.get("is_exam") else 0
            is_cancelled = 1 if c.get("is_cancelled") else 0
            status = c.get("status") or "PLANIFIE"
            description = c.get("description") or ""
            duration = int(c.get("duration") or 90)
            searchable_text = c.get("searchable_text") or f"{course_name} {activity_type}"

            # Check if course exists by ID or by same slot (start, end, course_name)
            cur.execute("SELECT id FROM courses WHERE id = ?", (c_id,))
            row_by_id = cur.fetchone()
            if not row_by_id:
                cur.execute("SELECT id FROM courses WHERE start = ? AND end = ? AND course_name = ?", (start, end, course_name))
                row_by_slot = cur.fetchone()
                if row_by_slot:
                    c_id = row_by_slot["id"]
                    exists = True
                else:
                    exists = False
            else:
                exists = True

            if exists:
                cur.execute("""
                    UPDATE courses SET
                        start = ?, end = ?, date_str = ?, start_time_str = ?, end_time_str = ?,
                        duration = ?, course_name = ?, course_code = ?, activity_type = ?,
                        activity_name = ?, is_exam = ?, is_cancelled = ?, status = ?,
                        description = ?, searchable_text = ?, updated_at = ?
                    WHERE id = ?
                """, (
                    start, end, date_str, start_time_str, end_time_str,
                    duration, course_name, course_code, activity_type,
                    activity_name, is_exam, is_cancelled, status,
                    description, searchable_text, now, c_id
                ))
                updated_count += 1
            else:
                cur.execute("""
                    INSERT INTO courses (
                        id, start, end, date_str, start_time_str, end_time_str,
                        duration, course_name, course_code, activity_type,
                        activity_name, is_exam, is_cancelled, status,
                        description, searchable_text, updated_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """, (
                    c_id, start, end, date_str, start_time_str, end_time_str,
                    duration, course_name, course_code, activity_type,
                    activity_name, is_exam, is_cancelled, status,
                    description, searchable_text, now
                ))
                inserted_count += 1

            # Ingest Teachers
            teachers = c.get("teachers") or []
            cur.execute("DELETE FROM course_teachers WHERE course_id = ?", (c_id,))
            for teacher_name in teachers:
                t_name = sanitize_teacher_name(teacher_name)
                if not t_name:
                    continue
                cur.execute("INSERT INTO teachers (name) VALUES (?) ON CONFLICT(name) DO NOTHING;", (t_name,))
                cur.execute("SELECT id FROM teachers WHERE name = ?", (t_name,))
                t_row = cur.fetchone()
                if t_row:
                    cur.execute("INSERT OR IGNORE INTO course_teachers (course_id, teacher_id) VALUES (?, ?);", (c_id, t_row["id"]))

            # Ingest Rooms
            rooms = c.get("rooms") or []
            cur.execute("DELETE FROM course_rooms WHERE course_id = ?", (c_id,))
            for room_name in rooms:
                r_name = sanitize_room_name(room_name)
                if not r_name:
                    continue
                cur.execute("INSERT INTO rooms (name) VALUES (?) ON CONFLICT(name) DO NOTHING;", (r_name,))
                cur.execute("SELECT id FROM rooms WHERE name = ?", (r_name,))
                r_row = cur.fetchone()
                if r_row:
                    cur.execute("INSERT OR IGNORE INTO course_rooms (course_id, room_id) VALUES (?, ?);", (c_id, r_row["id"]))

            # Ingest Groups
            groups = c.get("groups") or []
            cur.execute("DELETE FROM course_groups WHERE course_id = ?", (c_id,))
            for group_name in groups:
                g_name = group_name.strip()
                if not g_name:
                    continue
                g_id_info = parse_group_identity(g_name)
                cur.execute("""
                    INSERT INTO groups (name, clean_name, promo_id, subgroup, display_name)
                    VALUES (?, ?, ?, ?, ?)
                    ON CONFLICT(name) DO UPDATE SET
                        clean_name = excluded.clean_name,
                        promo_id = excluded.promo_id,
                        subgroup = excluded.subgroup,
                        display_name = excluded.display_name;
                """, (g_name, g_id_info["clean_name"], g_id_info["promo_id"], g_id_info["subgroup"], g_id_info["display_name"]))
                cur.execute("SELECT id FROM groups WHERE name = ?", (g_name,))
                g_row = cur.fetchone()
                if g_row:
                    cur.execute("INSERT OR IGNORE INTO course_groups (course_id, group_id) VALUES (?, ?);", (c_id, g_row["id"]))

        # Log contribution
        cur.execute("INSERT INTO contributions (timestamp, course_count, consent) VALUES (?, ?, ?);",
                    (now, len(courses), 1))

        conn.commit()
    except Exception as e:
        conn.rollback()
        raise e
    finally:
        conn.close()

    # Automatically detect and sync recurring course patterns
    try:
        detect_and_sync_recurring_courses(db_path)
    except Exception as err:
        print(f"[Warning] Recurring course pattern sync failed: {err}")

    return {
        "success": True,
        "inserted": inserted_count,
        "updated": updated_count,
        "total": inserted_count + updated_count,
    }


def get_database_stats(db_path: str = DB_PATH) -> Dict[str, Any]:
    """Returns general statistics of the database knowledge base."""
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()

    cur.execute("SELECT COUNT(*) AS cnt FROM courses;")
    total_courses = cur.fetchone()["cnt"]

    cur.execute("SELECT COUNT(*) AS cnt FROM teachers;")
    total_teachers = cur.fetchone()["cnt"]

    cur.execute("SELECT COUNT(*) AS cnt FROM rooms;")
    total_rooms = cur.fetchone()["cnt"]

    cur.execute("SELECT COUNT(*) AS cnt FROM groups;")
    total_groups = cur.fetchone()["cnt"]

    cur.execute("SELECT COUNT(*) AS cnt FROM promotions;")
    total_promos = cur.fetchone()["cnt"]

    cur.execute("SELECT COUNT(*) AS cnt FROM recurring_courses;")
    total_patterns = cur.fetchone()["cnt"]

    cur.execute("SELECT COUNT(*) AS cnt FROM recurring_courses WHERE is_recurring = 1;")
    recurring_patterns = cur.fetchone()["cnt"]

    cur.execute("SELECT MIN(date_str) AS min_d, MAX(date_str) AS max_d FROM courses;")
    range_row = cur.fetchone()
    min_date = range_row["min_d"] if range_row and range_row["min_d"] else None
    max_date = range_row["max_d"] if range_row and range_row["max_d"] else None

    # Activity types breakdown
    cur.execute("SELECT activity_type, COUNT(*) AS cnt FROM courses GROUP BY activity_type ORDER BY cnt DESC;")
    type_rows = cur.fetchall()
    types_breakdown = {r["activity_type"]: r["cnt"] for r in type_rows}

    conn.close()

    return {
        "total_courses": total_courses,
        "total_teachers": total_teachers,
        "total_rooms": total_rooms,
        "total_groups": total_groups,
        "total_promotions": total_promos,
        "total_patterns": total_patterns,
        "recurring_patterns": recurring_patterns,
        "min_date": min_date,
        "max_date": max_date,
        "types_breakdown": types_breakdown,
    }


def get_all_teachers(db_path: str = DB_PATH) -> List[str]:
    """Returns sorted list of all distinct teachers, sanitized."""
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()
    cur.execute("SELECT name FROM teachers ORDER BY name COLLATE NOCASE ASC;")
    teachers = [sanitize_teacher_name(r["name"]) for r in cur.fetchall()]
    conn.close()
    return sorted(list(set(t for t in teachers if t)))


def get_all_rooms(db_path: str = DB_PATH) -> List[str]:
    """Returns sorted list of all distinct rooms without floor/level suffix."""
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()
    cur.execute("SELECT name FROM rooms ORDER BY name COLLATE NOCASE ASC;")
    rooms = [sanitize_room_name(r["name"]) for r in cur.fetchall()]
    conn.close()
    return sorted(list(set(r for r in rooms if r)))


def get_all_groups(db_path: str = DB_PATH) -> List[str]:
    """Returns sorted list of all distinct group raw names."""
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()
    cur.execute("SELECT name FROM groups ORDER BY name COLLATE NOCASE ASC;")
    groups = [r["name"] for r in cur.fetchall()]
    conn.close()
    return groups


def get_all_groups_structured(db_path: str = DB_PATH) -> List[Dict[str, Any]]:
    """Returns groups organized with their clean promo and subgroup labels."""
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()
    cur.execute("""
        SELECT g.id, g.name, g.clean_name, g.promo_id, g.subgroup, g.display_name,
               p.name AS promo_name, p.cycle, p.year
        FROM groups g
        LEFT JOIN promotions p ON g.promo_id = p.id
        ORDER BY p.year ASC, g.subgroup ASC;
    """)
    rows = [dict(r) for r in cur.fetchall()]
    conn.close()
    return rows


def get_structured_promotions_and_groups(db_path: str = DB_PATH) -> List[Dict[str, Any]]:
    """
    Returns EPITA Lyon promotions with their recognized classes (L1, L2, L3, Promo)
    and course counts.
    """
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()

    cur.execute("SELECT * FROM promotions ORDER BY year ASC;")
    promos = [dict(r) for r in cur.fetchall()]

    cur.execute("""
        SELECT g.id, g.name, g.clean_name, g.promo_id, g.subgroup, g.display_name,
               COUNT(cg.course_id) AS course_count
        FROM groups g
        LEFT JOIN course_groups cg ON g.id = cg.group_id
        GROUP BY g.id
        ORDER BY g.subgroup ASC;
    """)
    group_rows = [dict(r) for r in cur.fetchall()]
    conn.close()

    promo_map = {p["id"]: {**p, "classes": []} for p in promos}
    other_classes = []

    for gr in group_rows:
        pid = gr.get("promo_id")
        if pid in promo_map:
            promo_map[pid]["classes"].append(gr)
        else:
            other_classes.append(gr)

    result = list(promo_map.values())
    if other_classes:
        result.append({
            "id": "autre",
            "code": "AUTRE",
            "name": "Autres Groupes",
            "cycle": "Divers",
            "year": 0,
            "semesters": "",
            "classes": other_classes
        })

    return result


def detect_and_sync_recurring_courses(db_path: str = DB_PATH) -> Dict[str, Any]:
    """
    Analyzes all course sessions in the database, detects recurring course patterns
    (subject, activity type, day of week, timeslot, typical classroom, teacher, promo/group),
    stores them into the `recurring_courses` table, and links individual course instances
    to their recognized pattern.
    """
    from collections import Counter
    from datetime import datetime, date

    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()

    # Query all active courses with associated relations
    cur.execute("""
        SELECT c.id, c.course_name, c.course_code, c.activity_type, c.activity_name,
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
        ORDER BY c.start ASC;
    """)
    rows = cur.fetchall()

    day_names = ["Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi", "Dimanche"]
    patterns: Dict[str, Dict[str, Any]] = {}

    for r in rows:
        raw_start = r["start"].replace("Z", "").split("+")[0]
        try:
            dt = datetime.fromisoformat(raw_start)
            dow = dt.weekday()
        except Exception:
            dow = 0
        dname = day_names[dow]

        cname = (r["course_name"] or "").strip()
        act_type = (r["activity_type"] or "AUTRE").strip().upper()
        st = (r["start_time_str"] or "08:30").strip()
        et = (r["end_time_str"] or "10:00").strip()
        dur = int(r["duration"] or 90)

        # Parse primary group and promotion
        raw_groups = [g.strip() for g in (r["groups"] or "").split(",") if g.strip()]
        first_group = raw_groups[0] if raw_groups else ""
        grp_identity = parse_group_identity(first_group)

        # Build clean pattern key
        clean_cname_slug = re.sub(r'[^a-zA-Z0-9]+', '_', cname.lower()).strip('_')
        pattern_key = f"{clean_cname_slug}_{act_type.lower()}_d{dow}_{st.replace(':', '')}_{et.replace(':', '')}_{grp_identity['promo_id']}_{grp_identity['subgroup'].lower()}"

        if pattern_key not in patterns:
            patterns[pattern_key] = {
                "pattern_key": pattern_key,
                "course_name": cname,
                "course_code": r["course_code"] or "",
                "activity_type": act_type,
                "activity_name": r["activity_name"] or "",
                "day_of_week": dow,
                "day_name": dname,
                "start_time": st,
                "end_time": et,
                "duration": dur,
                "promo_id": grp_identity["promo_id"],
                "promo_name": grp_identity["promo_name"],
                "subgroup": grp_identity["subgroup"],
                "group_display": grp_identity["clean_name"],
                "course_ids": [],
                "dates": [],
                "teachers": Counter(),
                "rooms": Counter(),
            }

        patterns[pattern_key]["course_ids"].append(r["id"])
        patterns[pattern_key]["dates"].append(raw_start[:10])

        if r["teachers"]:
            for t in r["teachers"].split(","):
                t_clean = sanitize_teacher_name(t)
                if t_clean:
                    patterns[pattern_key]["teachers"][t_clean] += 1

        if r["rooms"]:
            for rm in r["rooms"].split(","):
                rm_clean = sanitize_room_name(rm)
                if rm_clean:
                    patterns[pattern_key]["rooms"][rm_clean] += 1

    now = time.time()
    saved_patterns = 0
    recurring_slots = 0

    for p_key, p_data in patterns.items():
        occ_count = len(p_data["course_ids"])
        sorted_dates = sorted(p_data["dates"])
        first_d = sorted_dates[0] if sorted_dates else ""
        last_d = sorted_dates[-1] if sorted_dates else ""

        # Determine recurrence and cadence
        if occ_count >= 3:
            try:
                d_objs = [date.fromisoformat(d) for d in sorted_dates]
                intervals = [(d_objs[i+1] - d_objs[i]).days for i in range(len(d_objs)-1)]
                median_interval = sorted(intervals)[len(intervals)//2] if intervals else 7
                if 5 <= median_interval <= 9:
                    frequency = "Hebdomadaire"
                elif 11 <= median_interval <= 16:
                    frequency = "Bi-hebdomadaire"
                else:
                    frequency = "Périodique"
            except Exception:
                frequency = "Hebdomadaire"
            is_rec = 1
            recurring_slots += 1
        elif occ_count == 2:
            frequency = "Bi-hebdomadaire / Périodique"
            is_rec = 1
            recurring_slots += 1
        else:
            frequency = "Ponctuel"
            is_rec = 0

        # Primary teacher & room (sanitized without floor)
        primary_teacher = p_data["teachers"].most_common(1)[0][0] if p_data["teachers"] else ""
        all_teachers = ", ".join(t for t, _ in p_data["teachers"].most_common())
        primary_room = p_data["rooms"].most_common(1)[0][0] if p_data["rooms"] else ""
        all_rooms = ", ".join(rm for rm, _ in p_data["rooms"].most_common())

        cur.execute("""
            INSERT INTO recurring_courses (
                pattern_key, course_name, course_code, activity_type, activity_name,
                day_of_week, day_name, start_time, end_time, duration,
                promo_id, promo_name, subgroup, group_display,
                primary_teacher, all_teachers, primary_room, all_rooms,
                occurrences_count, first_date, last_date, frequency,
                is_recurring, confidence_score, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                updated_at = excluded.updated_at;
        """, (
            p_key, p_data["course_name"], p_data["course_code"], p_data["activity_type"], p_data["activity_name"],
            p_data["day_of_week"], p_data["day_name"], p_data["start_time"], p_data["end_time"], p_data["duration"],
            p_data["promo_id"], p_data["promo_name"], p_data["subgroup"], p_data["group_display"],
            primary_teacher, all_teachers, primary_room, all_rooms,
            occ_count, first_d, last_d, frequency,
            is_rec, min(1.0, 0.5 + 0.1 * occ_count), now, now
        ))
        saved_patterns += 1

        # Link individual course instances to recurring_courses.id
        cur.execute("SELECT id FROM recurring_courses WHERE pattern_key = ?", (p_key,))
        rec_row = cur.fetchone()
        if rec_row:
            rec_id = rec_row["id"]
            for cid in p_data["course_ids"]:
                cur.execute("UPDATE courses SET recurring_id = ?, pattern_key = ? WHERE id = ?", (rec_id, p_key, cid))

    conn.commit()
    conn.close()

    return {
        "success": True,
        "total_patterns": saved_patterns,
        "recurring_slots": recurring_slots,
        "single_slots": saved_patterns - recurring_slots,
    }


def query_recurring_courses(
    promo_id: Optional[str] = None,
    subgroup: Optional[str] = None,
    day_of_week: Optional[int] = None,
    teacher: Optional[str] = None,
    keyword: Optional[str] = None,
    db_path: str = DB_PATH,
) -> Dict[str, Any]:
    """
    Queries recognized recurring course patterns.
    Returns:
    - patterns: flat list of recurring courses
    - weekly_grid: weekly schedule template matrix (Lundi to Samedi)
    """
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()

    conditions = ["1=1"]
    params: List[Any] = []

    if promo_id:
        conditions.append("promo_id = ?")
        params.append(promo_id)

    if subgroup:
        conditions.append("subgroup = ?")
        params.append(subgroup)

    if day_of_week is not None:
        conditions.append("day_of_week = ?")
        params.append(int(day_of_week))

    if teacher and teacher.strip():
        conditions.append("(primary_teacher LIKE ? OR all_teachers LIKE ?)")
        t = f"%{teacher.strip()}%"
        params.extend([t, t])

    if keyword and keyword.strip():
        conditions.append("(course_name LIKE ? OR group_display LIKE ?)")
        k = f"%{keyword.strip()}%"
        params.extend([k, k])

    where_clause = " AND ".join(conditions)
    cur.execute(f"""
        SELECT * FROM recurring_courses
        WHERE {where_clause}
        ORDER BY day_of_week ASC, start_time ASC;
    """, params)

    rows = [dict(r) for r in cur.fetchall()]
    conn.close()

    day_names = ["Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi"]
    weekly_grid: Dict[str, List[Dict[str, Any]]] = {d: [] for d in day_names}

    for r in rows:
        d_name = r.get("day_name") or "Lundi"
        if d_name in weekly_grid:
            weekly_grid[d_name].append(r)

    return {
        "success": True,
        "patterns": rows,
        "weekly_grid": weekly_grid,
        "count": len(rows),
    }


def clean_and_migrate_database(db_path: str = DB_PATH) -> None:
    """
    Cleans up existing database records:
    - Strips floor/level suffixes from room names
    - Cleans teacher names from multiline descriptions
    - Populates promo and subgroup metadata on all groups
    - Re-runs pattern recognition to detect recurring courses
    """
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()

    # 1. Clean room names (remove floor/level suffixes)
    cur.execute("SELECT id, name FROM rooms;")
    rooms = cur.fetchall()
    for r in rooms:
        clean_name = sanitize_room_name(r["name"])
        if clean_name and clean_name != r["name"]:
            cur.execute("SELECT id FROM rooms WHERE name = ? AND id != ?", (clean_name, r["id"]))
            existing = cur.fetchone()
            if existing:
                cur.execute("UPDATE OR IGNORE course_rooms SET room_id = ? WHERE room_id = ?", (existing["id"], r["id"]))
                cur.execute("DELETE FROM rooms WHERE id = ?", (r["id"],))
            else:
                cur.execute("UPDATE rooms SET name = ? WHERE id = ?", (clean_name, r["id"]))

    # 2. Clean teacher names
    cur.execute("SELECT id, name FROM teachers;")
    teachers = cur.fetchall()
    for t in teachers:
        clean_t = sanitize_teacher_name(t["name"])
        if clean_t and clean_t != t["name"]:
            cur.execute("SELECT id FROM teachers WHERE name = ? AND id != ?", (clean_t, t["id"]))
            existing_t = cur.fetchone()
            if existing_t:
                cur.execute("UPDATE OR IGNORE course_teachers SET teacher_id = ? WHERE teacher_id = ?", (existing_t["id"], t["id"]))
                cur.execute("DELETE FROM teachers WHERE id = ?", (t["id"],))
            else:
                cur.execute("UPDATE teachers SET name = ? WHERE id = ?", (clean_t, t["id"]))

    # 3. Categorize all groups with proper promotions and subgroups
    cur.execute("SELECT id, name FROM groups;")
    groups = cur.fetchall()
    for g in groups:
        grp_info = parse_group_identity(g["name"])
        cur.execute("""
            UPDATE groups SET
                clean_name = ?,
                promo_id = ?,
                subgroup = ?,
                display_name = ?
            WHERE id = ?
        """, (
            grp_info["clean_name"],
            grp_info["promo_id"],
            grp_info["subgroup"],
            grp_info["display_name"],
            g["id"]
        ))

    conn.commit()
    conn.close()

    # 4. Trigger recurring course pattern recognition
    detect_and_sync_recurring_courses(db_path)


def get_all_activity_types(db_path: str = DB_PATH) -> List[str]:
    """Returns sorted list of all distinct activity types."""
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()
    cur.execute("SELECT DISTINCT activity_type FROM courses ORDER BY activity_type ASC;")
    types = [r["activity_type"] for r in cur.fetchall()]
    conn.close()
    return types


def query_custom_courses(
    teachers: Optional[List[str]] = None,
    rooms: Optional[List[str]] = None,
    groups: Optional[List[str]] = None,
    activity_types: Optional[List[str]] = None,
    keyword: Optional[str] = None,
    start_date: Optional[str] = None,
    end_date: Optional[str] = None,
    hide_cancelled: bool = False,
    db_path: str = DB_PATH,
) -> List[Dict[str, Any]]:
    """
    Queries courses from the database matching custom filters.
    Returns normalized course dictionaries compatible with the calendar UI and ICS exporter.
    """
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()

    conditions = ["1=1"]
    params: List[Any] = []

    if hide_cancelled:
        conditions.append("c.is_cancelled = 0")

    if start_date:
        conditions.append("c.date_str >= ?")
        params.append(start_date[:10])

    if end_date:
        conditions.append("c.date_str <= ?")
        params.append(end_date[:10])

    if activity_types:
        placeholders = ",".join("?" for _ in activity_types)
        conditions.append(f"c.activity_type IN ({placeholders})")
        params.extend([t.upper() for t in activity_types])

    if teachers:
        placeholders = ",".join("?" for _ in teachers)
        conditions.append(f"""
            c.id IN (
                SELECT ct.course_id FROM course_teachers ct
                JOIN teachers t ON ct.teacher_id = t.id
                WHERE t.name IN ({placeholders})
            )
        """)
        params.extend(teachers)

    if rooms:
        placeholders = ",".join("?" for _ in rooms)
        conditions.append(f"""
            c.id IN (
                SELECT cr.course_id FROM course_rooms cr
                JOIN rooms r ON cr.room_id = r.id
                WHERE r.name IN ({placeholders})
            )
        """)
        params.extend(rooms)

    if groups:
        placeholders = ",".join("?" for _ in groups)
        conditions.append(f"""
            c.id IN (
                SELECT cg.course_id FROM course_groups cg
                JOIN groups g ON cg.group_id = g.id
                WHERE g.name IN ({placeholders})
            )
        """)
        params.extend(groups)

    if keyword and keyword.strip():
        k = f"%{keyword.strip()}%"
        conditions.append("(c.course_name LIKE ? OR c.searchable_text LIKE ? OR c.description LIKE ?)")
        params.extend([k, k, k])

    where_clause = " AND ".join(conditions)
    sql = f"""
        SELECT c.* FROM courses c
        WHERE {where_clause}
        ORDER BY c.start ASC;
    """

    cur.execute(sql, params)
    course_rows = cur.fetchall()

    if not course_rows:
        conn.close()
        return []

    # Collect all IDs to bulk-fetch teachers, rooms, and groups
    course_ids = [r["id"] for r in course_rows]
    id_placeholders = ",".join("?" for _ in course_ids)

    # Fetch teachers mapping
    cur.execute(f"""
        SELECT ct.course_id, t.name FROM course_teachers ct
        JOIN teachers t ON ct.teacher_id = t.id
        WHERE ct.course_id IN ({id_placeholders})
        ORDER BY t.name ASC;
    """, course_ids)
    teacher_map: Dict[str, List[str]] = {}
    for r in cur.fetchall():
        teacher_map.setdefault(r["course_id"], []).append(r["name"])

    # Fetch rooms mapping
    cur.execute(f"""
        SELECT cr.course_id, r.name FROM course_rooms cr
        JOIN rooms r ON cr.room_id = r.id
        WHERE cr.course_id IN ({id_placeholders})
        ORDER BY r.name ASC;
    """, course_ids)
    room_map: Dict[str, List[str]] = {}
    for r in cur.fetchall():
        room_map.setdefault(r["course_id"], []).append(r["name"])

    # Fetch groups mapping
    cur.execute(f"""
        SELECT cg.course_id, g.name FROM course_groups cg
        JOIN groups g ON cg.group_id = g.id
        WHERE cg.course_id IN ({id_placeholders})
        ORDER BY g.name ASC;
    """, course_ids)
    group_map: Dict[str, List[str]] = {}
    for r in cur.fetchall():
        group_map.setdefault(r["course_id"], []).append(r["name"])

    conn.close()

    result = []
    for r in course_rows:
        cid = r["id"]
        c_teachers = teacher_map.get(cid, [])
        c_rooms = room_map.get(cid, [])
        c_groups = group_map.get(cid, [])

        summary = f"[{r['activity_type']}] {r['course_name']}"

        result.append({
            "id": cid,
            "start": r["start"],
            "end": r["end"],
            "duration": r["duration"],
            "summary": summary,
            "course_name": r["course_name"],
            "course_code": r["course_code"] or "",
            "activity_type": r["activity_type"],
            "activity_name": r["activity_name"] or "",
            "is_exam": bool(r["is_exam"]),
            "is_cancelled": bool(r["is_cancelled"]),
            "status": r["status"] or "PLANIFIE",
            "teachers": c_teachers,
            "rooms": c_rooms,
            "groups": c_groups,
            "description": r["description"] or "",
            "searchable_text": r["searchable_text"] or "",
        })

    return result


def query_free_rooms(
    target_date: str,
    start_time: str,
    end_time: str,
    db_path: str = DB_PATH,
) -> Dict[str, Any]:
    """
    Checks the availability of all indexed rooms on a given date and time interval.
    Returns:
    - free_rooms: rooms with no overlapping course, with indication of when the next course starts.
    - occupied_rooms: rooms currently occupied, with the course details and teacher.
    """
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()

    target_d = target_date[:10]
    t_start = start_time.strip()
    t_end = end_time.strip()

    # Get all known rooms
    cur.execute("SELECT id, name FROM rooms ORDER BY name COLLATE NOCASE ASC;")
    all_rooms = cur.fetchall()

    if not all_rooms:
        conn.close()
        return {
            "date": target_d,
            "start_time": t_start,
            "end_time": t_end,
            "free_rooms": [],
            "occupied_rooms": [],
            "total_rooms": 0,
        }

    # Fetch all courses on target date
    cur.execute("""
        SELECT c.id, c.start, c.end, c.start_time_str, c.end_time_str,
               c.course_name, c.activity_type, c.is_cancelled, cr.room_id
        FROM courses c
        JOIN course_rooms cr ON c.id = cr.course_id
        WHERE c.date_str = ? AND c.is_cancelled = 0
        ORDER BY c.start ASC;
    """, (target_d,))
    day_courses = cur.fetchall()

    # Map room_id -> list of courses that day
    room_courses_map: Dict[int, List[sqlite3.Row]] = {}
    for dc in day_courses:
        room_courses_map.setdefault(dc["room_id"], []).append(dc)

    # Get teachers for day courses
    course_ids = [dc["id"] for dc in day_courses]
    teacher_map: Dict[str, List[str]] = {}
    group_map: Dict[str, List[str]] = {}
    if course_ids:
        placeholders = ",".join("?" for _ in course_ids)
        cur.execute(f"""
            SELECT ct.course_id, t.name FROM course_teachers ct
            JOIN teachers t ON ct.teacher_id = t.id
            WHERE ct.course_id IN ({placeholders})
        """, course_ids)
        for r in cur.fetchall():
            teacher_map.setdefault(r["course_id"], []).append(r["name"])

        cur.execute(f"""
            SELECT cg.course_id, g.name FROM course_groups cg
            JOIN groups g ON cg.group_id = g.id
            WHERE cg.course_id IN ({placeholders})
        """, course_ids)
        for r in cur.fetchall():
            group_map.setdefault(r["course_id"], []).append(r["name"])

    conn.close()

    free_rooms = []
    occupied_rooms = []

    for r in all_rooms:
        r_id = r["id"]
        r_name = r["name"]
        courses_in_room = room_courses_map.get(r_id, [])

        # Check overlapping courses
        overlapping = []
        subsequent_courses = []

        for c in courses_in_room:
            c_start = c["start_time_str"]
            c_end = c["end_time_str"]

            # Overlap condition: c_start < t_end AND c_end > t_start
            if c_start < t_end and c_end > t_start:
                overlapping.append(c)
            elif c_start >= t_end:
                subsequent_courses.append(c)

        if overlapping:
            # Room is occupied
            active_c = overlapping[0]
            cid = active_c["id"]
            teachers = teacher_map.get(cid, [])
            groups = group_map.get(cid, [])
            occupied_rooms.append({
                "room_name": r_name,
                "is_free": False,
                "current_course": {
                    "id": cid,
                    "title": active_c["course_name"],
                    "type": active_c["activity_type"],
                    "start": active_c["start_time_str"],
                    "end": active_c["end_time_str"],
                    "teachers": teachers,
                    "groups": groups,
                },
                "total_overlapping": len(overlapping),
            })
        else:
            # Room is free
            free_until_text = "Toute la journée"
            next_course_info = None

            if subsequent_courses:
                # Sort to get the earliest next course
                subsequent_courses.sort(key=lambda x: x["start_time_str"])
                next_c = subsequent_courses[0]
                free_until_text = f"Jusqu'à {next_c['start_time_str']}"
                next_course_info = {
                    "title": next_c["course_name"],
                    "type": next_c["activity_type"],
                    "start": next_c["start_time_str"],
                    "end": next_c["end_time_str"],
                }

            free_rooms.append({
                "room_name": r_name,
                "is_free": True,
                "free_until": free_until_text,
                "next_course": next_course_info,
                "courses_count_today": len(courses_in_room),
            })

    # Sort free rooms alphabetically
    free_rooms.sort(key=lambda x: x["room_name"])
    occupied_rooms.sort(key=lambda x: x["room_name"])

    return {
        "date": target_d,
        "start_time": t_start,
        "end_time": t_end,
        "free_rooms": free_rooms,
        "occupied_rooms": occupied_rooms,
        "free_count": len(free_rooms),
        "occupied_count": len(occupied_rooms),
        "total_rooms": len(all_rooms),
    }


def get_epita_group_tree() -> Dict[str, Any]:
    """
    Returns the EPITA Lyon organizational tree matching the classic Zeus app.
    Contains only Lyon classes (Prepa Lyon & Cycle Ingénieur Lyon).
    """
    return {
        "id": "root",
        "label": "EPITA",
        "expanded": True,
        "children": [
            {
                "id": "prepa",
                "label": "CLASSES PREPARATOIRES",
                "expanded": True,
                "children": [
                    {
                        "id": "spe_lyon",
                        "label": "SPE Lyon",
                        "expanded": True,
                        "children": [
                            {"id": "spe_lyon_l1", "label": "S3S4 L1", "patterns": ["S3 Lyon L1", "S3S4 L1", "S3 L1"]},
                            {"id": "spe_lyon_l2", "label": "S3S4 L2", "patterns": ["S3 Lyon L2", "S3S4 L2", "S3 L2"]},
                            {"id": "spe_lyon_l3", "label": "S3S4 L3", "patterns": ["S3 Lyon L3", "S3S4 L3", "S3 L3", "2627 - S3 Lyon L3"]},
                            {"id": "spe_lyon_promo", "label": "Promo SPE Lyon", "patterns": ["2627 - S3 Lyon", "S3 Lyon"]}
                        ]
                    },
                    {
                        "id": "sup_lyon",
                        "label": "SUP Lyon",
                        "expanded": False,
                        "children": [
                            {"id": "sup_lyon_l1", "label": "S1S2 L1", "patterns": ["S1 Lyon L1", "S1S2 L1", "S1 L1"]},
                            {"id": "sup_lyon_l2", "label": "S1S2 L2", "patterns": ["S1 Lyon L2", "S1S2 L2", "S1 L2"]},
                            {"id": "sup_lyon_l3", "label": "S1S2 L3", "patterns": ["S1 Lyon L3", "S1S2 L3", "S1 L3"]},
                            {"id": "sup_lyon_promo", "label": "Promo SUP Lyon", "patterns": ["2627 - S1 Lyon", "S1 Lyon"]}
                        ]
                    }
                ]
            },
            {
                "id": "cycle_ingenieur",
                "label": "CYCLE INGENIEUR",
                "expanded": False,
                "children": [
                    {
                        "id": "ing1_lyon",
                        "label": "ING1 Lyon (S5/S6)",
                        "expanded": False,
                        "children": [
                            {"id": "ing1_lyon_l1", "label": "SI5 Lyon L1", "patterns": ["SI5 Lyon L1", "ING1 Lyon L1", "SI5 L1"]},
                            {"id": "ing1_lyon_l2", "label": "SI5 Lyon L2", "patterns": ["SI5 Lyon L2", "ING1 Lyon L2", "SI5 L2"]},
                            {"id": "ing1_lyon_l3", "label": "SI5 Lyon L3", "patterns": ["SI5 Lyon L3", "ING1 Lyon L3", "SI5 L3"]},
                            {"id": "ing1_lyon_promo", "label": "Promo SI5 Lyon", "patterns": ["2627 - SI5 Lyon", "SI5 Lyon"]}
                        ]
                    },
                    {
                        "id": "majeures_lyon",
                        "label": "MAJEURES LYON",
                        "expanded": False,
                        "children": [
                            {"id": "maj_if", "label": "IF", "patterns": ["IF", "Ingénierie Financière", "SI7 IF", "SI9 IF"]},
                            {"id": "maj_rdil", "label": "RDI-L", "patterns": ["RDI-L", "RDI", "SI7 RDI", "SI9 RDI"]},
                            {"id": "maj_sante", "label": "SANTE", "patterns": ["SANTE", "Santé", "SI7 SANTE", "SI9 SANTE"]},
                            {"id": "maj_scia_graphes", "label": "SCIA Graphes", "patterns": ["SCIA Graphes", "SCIA-G", "SI9 Lyon SCIA-G", "SI9 Lyon SCIA", "SCIA"]}
                        ]
                    }
                ]
            }
        ]
    }


def query_courses_by_groups(
    patterns: List[str],
    start_date: Optional[str] = None,
    end_date: Optional[str] = None,
    db_path: str = DB_PATH
) -> List[Dict[str, Any]]:
    """
    Queries courses from the database matching group patterns or aliases.
    """
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()

    conditions = ["c.is_cancelled = 0"]
    params: List[Any] = []

    if start_date:
        conditions.append("c.date_str >= ?")
        params.append(start_date[:10])

    if end_date:
        conditions.append("c.date_str <= ?")
        params.append(end_date[:10])

    if patterns:
        like_clauses = []
        for p in patterns:
            like_clauses.append("g.name LIKE ?")
            params.append(f"%{p}%")
        group_filter = " OR ".join(like_clauses)
        conditions.append(f"""
            c.id IN (
                SELECT cg.course_id FROM course_groups cg
                JOIN groups g ON cg.group_id = g.id
                WHERE {group_filter}
            )
        """)

    where_sql = " AND ".join(conditions)
    sql = f"""
        SELECT c.* FROM courses c
        WHERE {where_sql}
        ORDER BY c.start ASC;
    """
    cur.execute(sql, params)
    course_rows = cur.fetchall()

    if not course_rows:
        conn.close()
        return []

    course_ids = [r["id"] for r in course_rows]
    id_placeholders = ",".join("?" for _ in course_ids)

    cur.execute(f"""
        SELECT ct.course_id, t.name FROM course_teachers ct
        JOIN teachers t ON ct.teacher_id = t.id
        WHERE ct.course_id IN ({id_placeholders})
    """, course_ids)
    teacher_map = {}
    for r in cur.fetchall():
        teacher_map.setdefault(r["course_id"], []).append(r["name"])

    cur.execute(f"""
        SELECT cr.course_id, r.name FROM course_rooms cr
        JOIN rooms r ON cr.room_id = r.id
        WHERE cr.course_id IN ({id_placeholders})
    """, course_ids)
    room_map = {}
    for r in cur.fetchall():
        room_map.setdefault(r["course_id"], []).append(r["name"])

    cur.execute(f"""
        SELECT cg.course_id, g.name FROM course_groups cg
        JOIN groups g ON cg.group_id = g.id
        WHERE cg.course_id IN ({id_placeholders})
    """, course_ids)
    group_map = {}
    for r in cur.fetchall():
        group_map.setdefault(r["course_id"], []).append(r["name"])

    conn.close()

    result = []
    for r in course_rows:
        cid = r["id"]
        result.append({
            "id": cid,
            "start": r["start"],
            "end": r["end"],
            "duration": r["duration"],
            "summary": f"[{r['activity_type']}] {r['course_name']}",
            "course_name": r["course_name"],
            "course_code": r["course_code"] or "",
            "activity_type": r["activity_type"],
            "activity_name": r["activity_name"] or "",
            "is_exam": bool(r["is_exam"]),
            "is_cancelled": bool(r["is_cancelled"]),
            "status": r["status"] or "PLANIFIE",
            "teachers": teacher_map.get(cid, []),
            "rooms": room_map.get(cid, []),
            "groups": group_map.get(cid, []),
            "description": r["description"] or "",
            "searchable_text": r["searchable_text"] or "",
        })

    return result

