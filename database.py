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


def init_db(db_path: str = DB_PATH) -> None:
    """Initializes tables and indexes for the school calendar database."""
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

        CREATE INDEX IF NOT EXISTS idx_courses_date ON courses(date_str);
        CREATE INDEX IF NOT EXISTS idx_courses_start ON courses(start);
        CREATE INDEX IF NOT EXISTS idx_courses_type ON courses(activity_type);
        CREATE INDEX IF NOT EXISTS idx_teachers_name ON teachers(name);
        CREATE INDEX IF NOT EXISTS idx_rooms_name ON rooms(name);
        CREATE INDEX IF NOT EXISTS idx_groups_name ON groups(name);
    """)

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

            # Check if course exists
            cur.execute("SELECT id FROM courses WHERE id = ?", (c_id,))
            exists = cur.fetchone() is not None

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
                t_name = teacher_name.strip()
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
                r_name = room_name.strip()
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
                cur.execute("INSERT INTO groups (name) VALUES (?) ON CONFLICT(name) DO NOTHING;", (g_name,))
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
        "min_date": min_date,
        "max_date": max_date,
        "types_breakdown": types_breakdown,
    }


def get_all_teachers(db_path: str = DB_PATH) -> List[str]:
    """Returns sorted list of all distinct teachers."""
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()
    cur.execute("SELECT name FROM teachers ORDER BY name COLLATE NOCASE ASC;")
    teachers = [r["name"] for r in cur.fetchall()]
    conn.close()
    return teachers


def get_all_rooms(db_path: str = DB_PATH) -> List[str]:
    """Returns sorted list of all distinct rooms."""
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()
    cur.execute("SELECT name FROM rooms ORDER BY name COLLATE NOCASE ASC;")
    rooms = [r["name"] for r in cur.fetchall()]
    conn.close()
    return rooms


def get_all_groups(db_path: str = DB_PATH) -> List[str]:
    """Returns sorted list of all distinct groups."""
    init_db(db_path)
    conn = get_db_connection(db_path)
    cur = conn.cursor()
    cur.execute("SELECT name FROM groups ORDER BY name COLLATE NOCASE ASC;")
    groups = [r["name"] for r in cur.fetchall()]
    conn.close()
    return groups


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
                        "id": "ing3_lyon",
                        "label": "ING3 Lyon (S9/S10)",
                        "expanded": False,
                        "children": [
                            {"id": "ing3_scia", "label": "SI9 Lyon SCIA", "patterns": ["SI9 Lyon SCIA", "SCIA-G", "SI9 SCIA"]},
                            {"id": "ing3_promo", "label": "Promo SI9 Lyon", "patterns": ["2627 - SI9 Lyon", "SI9 Lyon"]}
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

