"""
Feed Store Module
=================
Lightweight SQLite storage for managing recurring calendar subscription feeds.
Encrypts credentials so students can subscribe in Google Calendar via URL.
"""

import base64
import hashlib
import json
import os
import secrets
import sqlite3
import time

try:
    from cryptography.fernet import Fernet
    HAS_FERNET = True
except ImportError:
    HAS_FERNET = False


DB_PATH = os.getenv("FEEDS_DB_PATH", "feeds.db")
SECRET_FILE = ".server_secret"


def _get_server_key() -> bytes:
    """Retrieves or creates a 32-byte secret key for password encryption."""
    env_secret = os.getenv("SERVER_SECRET_KEY")
    if env_secret:
        return hashlib.sha256(env_secret.encode("utf-8")).digest()

    if os.path.exists(SECRET_FILE):
        try:
            with open(SECRET_FILE, "rb") as f:
                key = f.read().strip()
                if len(key) == 32:
                    return key
        except Exception:
            pass

    # Generate a fresh 32-byte key and store it
    key = secrets.token_bytes(32)
    try:
        with open(SECRET_FILE, "wb") as f:
            f.write(key)
    except Exception:
        pass
    return key


SERVER_KEY = _get_server_key()


def encrypt_secret(text: str) -> str:
    """Encrypts a string using Fernet or XOR-fallback."""
    if not text:
        return ""
    if HAS_FERNET:
        fernet_key = base64.urlsafe_b64encode(SERVER_KEY)
        f = Fernet(fernet_key)
        return f.encrypt(text.encode("utf-8")).decode("utf-8")
    else:
        # Fallback stream cipher
        key = SERVER_KEY
        raw = text.encode("utf-8")
        cipher = bytes([b ^ key[i % len(key)] for i, b in enumerate(raw)])
        return base64.urlsafe_b64encode(cipher).decode("utf-8")


def decrypt_secret(encrypted_text: str) -> str:
    """Decrypts a string using Fernet or XOR-fallback."""
    if not encrypted_text:
        return ""
    if HAS_FERNET:
        fernet_key = base64.urlsafe_b64encode(SERVER_KEY)
        f = Fernet(fernet_key)
        try:
            return f.decrypt(encrypted_text.encode("utf-8")).decode("utf-8")
        except Exception:
            return ""
    else:
        try:
            key = SERVER_KEY
            cipher = base64.urlsafe_b64decode(encrypted_text.encode("utf-8"))
            raw = bytes([b ^ key[i % len(key)] for i, b in enumerate(cipher)])
            return raw.decode("utf-8")
        except Exception:
            return ""


def init_db():
    """Initializes the SQLite tables."""
    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()
    cur.execute("""
        CREATE TABLE IF NOT EXISTS feeds (
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
            created_at REAL NOT NULL
        )
    """)
    cur.execute("PRAGMA table_info(feeds);")
    cols = [r[1] for r in cur.fetchall()]
    if "allowed_courses" not in cols:
        cur.execute("ALTER TABLE feeds ADD COLUMN allowed_courses TEXT;")
    if "cached_courses_json" not in cols:
        cur.execute("ALTER TABLE feeds ADD COLUMN cached_courses_json TEXT;")

    conn.commit()
    conn.close()


def create_feed(
    username: str = "",
    password: str = "",
    include_regex: str | None = None,
    exclude_regex: str | None = None,
    allowed_types: list[str] | None = None,
    allowed_courses: list[str] | None = None,
    hide_cancelled: bool = False,
    calendar_name: str = "EpiKairos - EPITA Emploi du temps",
    initial_ics: str | None = None,
    cached_courses: list[dict] | None = None,
) -> str:
    """Creates a new calendar feed subscription and returns its unique feed_id."""
    init_db()
    feed_id = secrets.token_urlsafe(20)
    enc_pw = encrypt_secret(password) if password else ""
    types_str = ",".join(allowed_types) if allowed_types else ""
    
    if allowed_courses is None:
        courses_str = ""
    elif len(allowed_courses) == 0:
        courses_str = "__EMPTY__"
    else:
        courses_str = "\t".join(allowed_courses)

    courses_json = json.dumps(cached_courses) if cached_courses else ""
    now = time.time()

    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()
    cur.execute(
        """
        INSERT INTO feeds (
            feed_id, username, encrypted_password,
            include_regex, exclude_regex, allowed_types,
            hide_cancelled, calendar_name, last_cached_ics,
            last_cached_at, created_at, allowed_courses,
            cached_courses_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """,
        (
            feed_id,
            username or "",
            enc_pw,
            include_regex or "",
            exclude_regex or "",
            types_str,
            1 if hide_cancelled else 0,
            calendar_name,
            initial_ics or "",
            now if initial_ics else 0,
            now,
            courses_str,
            courses_json,
        ),
    )
    conn.commit()
    conn.close()
    return feed_id


def get_feed(feed_id: str) -> dict | None:
    """Retrieves a feed configuration by its ID."""
    init_db()
    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()
    cur.execute(
        """
        SELECT feed_id, username, encrypted_password,
               include_regex, exclude_regex, allowed_types,
               hide_cancelled, calendar_name, last_cached_ics,
               last_cached_at, created_at, allowed_courses,
               cached_courses_json
        FROM feeds WHERE feed_id = ?
        """,
        (feed_id,),
    )
    row = cur.fetchone()
    conn.close()

    if not row:
        return None

    allowed_types = [t.strip() for t in row[5].split(",") if t.strip()] if row[5] else []
    
    courses_col = row[11] if len(row) > 11 and row[11] is not None else ""
    if courses_col == "__EMPTY__":
        allowed_courses = []
    elif courses_col:
        allowed_courses = [c.strip() for c in courses_col.split("\t") if c.strip()]
    else:
        allowed_courses = None

    cached_courses = []
    if len(row) > 12 and row[12]:
        try:
            cached_courses = json.loads(row[12])
        except Exception:
            cached_courses = []

    return {
        "feed_id": row[0],
        "username": row[1],
        "password": decrypt_secret(row[2]),
        "include_regex": row[3] or None,
        "exclude_regex": row[4] or None,
        "allowed_types": allowed_types or None,
        "allowed_courses": allowed_courses,
        "hide_cancelled": bool(row[6]),
        "calendar_name": row[7] or "EpiKairos - EPITA Emploi du temps",
        "last_cached_ics": row[8] or "",
        "last_cached_at": float(row[9] or 0),
        "created_at": float(row[10] or 0),
        "cached_courses": cached_courses,
    }


def update_feed_cache(feed_id: str, ics_content: str):
    """Updates the cached .ics content and timestamp for a feed."""
    init_db()
    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()
    cur.execute(
        "UPDATE feeds SET last_cached_ics = ?, last_cached_at = ? WHERE feed_id = ?",
        (ics_content, time.time(), feed_id),
    )
    conn.commit()
    conn.close()


def update_feed_courses(feed_id: str, courses: list[dict]):
    """Updates the cached raw courses JSON for a feed."""
    init_db()
    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()
    cur.execute(
        "UPDATE feeds SET cached_courses_json = ? WHERE feed_id = ?",
        (json.dumps(courses), feed_id),
    )
    conn.commit()
    conn.close()


def delete_feed(feed_id: str) -> bool:
    """Deletes a feed by its ID."""
    init_db()
    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()
    cur.execute("DELETE FROM feeds WHERE feed_id = ?", (feed_id,))
    deleted = cur.rowcount > 0
    conn.commit()
    conn.close()
    return deleted
