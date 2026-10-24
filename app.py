#!/usr/bin/env python3
"""
EpiKairos Web Service (EPITA Calendar)
=====================================
Self-hosted multi-page web platform for EPITA students:
1. Personal schedule preview & regex filtering with Google Calendar sync.
2. Shared knowledge base (courses, teachers, rooms, groups) with user consent.
3. Custom merged calendar generator (by teacher, promo, room).
4. Real-time available classroom finder (free rooms).
5. Old Zeus classic interface replica with group treeview and 24h timetable.
"""

from datetime import datetime, date, timedelta
import base64
import json
import os
import sys
import time
import urllib.parse
from flask import (
    Flask,
    render_template,
    request,
    jsonify,
    Response,
    send_file,
    send_from_directory,
    url_for,
    redirect,
    session,
)

from auriga_core import (
    AurigaClient,
    get_default_school_year_range,
    normalize_course,
    filter_courses,
    generate_ics_from_courses,
)
import database
import feed_store

def _get_flask_secret_key() -> str:
    """
    Retrieves or generates a cryptographically secure, persistent secret key for Flask sessions.
    Prevents session cookie forgery by avoiding default hardcoded strings.
    """
    env_secret = os.getenv("SESSION_SECRET_KEY") or os.getenv("FLASK_SECRET_KEY")
    if env_secret and env_secret != "auriga-epita-secret-key-change-in-prod":
        return env_secret

    secret_file = os.path.join(os.getenv("SECRET_DIR", "."), ".flask_secret")
    if os.path.exists(secret_file):
        try:
            try:
                os.chmod(secret_file, 0o600)
            except OSError:
                pass
            with open(secret_file, "r", encoding="utf-8") as f:
                key = f.read().strip()
                if len(key) >= 32:
                    return key
        except Exception:
            pass

    import secrets
    new_key = secrets.token_hex(32)
    try:
        fd = os.open(secret_file, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(new_key)
    except Exception:
        pass
    return new_key


app = Flask(__name__)
app.config["SECRET_KEY"] = _get_flask_secret_key()
app.config["MAX_CONTENT_LENGTH"] = 16 * 1024 * 1024  # 16 MB max payload
app.config["SESSION_COOKIE_HTTPONLY"] = True
app.config["SESSION_COOKIE_SAMESITE"] = "Lax"
app.config["SESSION_COOKIE_SECURE"] = os.getenv("SESSION_COOKIE_SECURE", "false").lower() in ("true", "1")
app.permanent_session_lifetime = timedelta(days=30)

# In-memory sliding window rate limiter for authentication endpoints
_LOGIN_ATTEMPTS: dict[str, list[float]] = {}
RATE_LIMIT_WINDOW = 60  # seconds
MAX_LOGIN_ATTEMPTS_PER_WINDOW = 10

def check_login_rate_limit(client_ip: str) -> bool:
    """Returns False if client_ip has exceeded MAX_LOGIN_ATTEMPTS_PER_WINDOW in the last RATE_LIMIT_WINDOW seconds."""
    now = time.time()
    attempts = _LOGIN_ATTEMPTS.setdefault(client_ip, [])
    # Keep attempts within the sliding window
    _LOGIN_ATTEMPTS[client_ip] = [t for t in attempts if now - t < RATE_LIMIT_WINDOW]
    if len(_LOGIN_ATTEMPTS[client_ip]) >= MAX_LOGIN_ATTEMPTS_PER_WINDOW:
        return False
    _LOGIN_ATTEMPTS[client_ip].append(now)
    return True

# Cache duration for live Google Calendar feeds (in seconds: default 12 hours = sync 1-2 times a day)
FEED_CACHE_DURATION = int(os.getenv("FEED_CACHE_DURATION", 43200))

# Ensure database tables exist
database.init_db()


# =============================================================================
# Security Headers Middleware
# =============================================================================

@app.after_request
def set_security_headers(response):
    """Inject defensive HTTP security headers into all responses."""
    response.headers["X-Content-Type-Options"] = "nosniff"
    response.headers["X-Frame-Options"] = "SAMEORIGIN"
    response.headers["X-XSS-Protection"] = "1; mode=block"
    response.headers["Referrer-Policy"] = "strict-origin-when-cross-origin"
    response.headers["Permissions-Policy"] = "geolocation=(), microphone=(), camera=()"
    response.headers["Content-Security-Policy"] = (
        "default-src 'self'; "
        "script-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; "
        "style-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; "
        "font-src 'self' https://cdn.jsdelivr.net data:; "
        "img-src 'self' data: https:; "
        "connect-src 'self'; "
        "frame-ancestors 'self';"
    )
    return response


# =============================================================================
# Authentication Middleware (Mandatory Auriga Account)
# =============================================================================

@app.before_request
def require_auriga_login():
    """
    Enforce mandatory Auriga login for all platform routes.
    Unauthenticated visitors are restricted and redirected to /login.
    """
    path = request.path

    # Public routes that do not require an active Auriga session
    if (
        path.startswith("/static/")
        or path.startswith("/feed/")
        or path in (
            "/login",
            "/logout",
            "/api/fetch-planning",
            "/manifest.json",
            "/sw.js",
            "/offline",
            "/health",
        )
    ):
        return None

    # Check if user has an active authenticated session
    if not session.get("logged_in"):
        if path.startswith("/api/"):
            return jsonify({
                "success": False,
                "error": "Authentification requise. Veuillez vous connecter avec votre compte Auriga.",
                "redirect": "/login",
            }), 401
        return redirect(url_for("login_page"))


# =============================================================================
# Multi-page Application Routes
# =============================================================================

@app.route("/")
@app.route("/dashboard")
def dashboard():
    """Dashboard view of the collaborative database (Accueil)."""
    stats = database.get_database_stats()
    return render_template(
        "dashboard.html",
        stats=stats,
        active_page="dashboard",
    )


@app.route("/old-zeus")
@app.route("/zeus")
def old_zeus():
    """Classic Zeus interface with EPITA group treeview and timetable grid."""
    start_date, end_date = get_default_school_year_range()
    today_str = date.today().isoformat()
    return render_template(
        "zeus_classic.html",
        default_start=start_date,
        default_end=end_date,
        today_date=today_str,
        active_page="old-zeus",
    )


@app.route("/login")
def login_page():
    """Dedicated login page for Auriga authentication."""
    start_date, end_date = get_default_school_year_range()
    return render_template(
        "login.html",
        default_start=start_date,
        default_end=end_date,
        active_page="login",
        logged_out=request.args.get("logged_out") == "1",
    )


@app.route("/logout")
def logout():
    """Clears user session and redirects to login page."""
    session.clear()
    return redirect(url_for("login_page", logged_out="1"))


@app.route("/personal")
@app.route("/my-calendar")
def personal_calendar():
    """Personal calendar page with regex filters, color chips, and export."""
    return render_template(
        "personal.html",
        active_page="personal",
    )


@app.route("/custom-calendar")
def custom_calendar():
    """Custom calendar builder (by teachers, rooms, or groups)."""
    start_date, end_date = get_default_school_year_range()
    return render_template(
        "custom_calendar.html",
        default_start=start_date,
        default_end=end_date,
        active_page="custom",
    )


@app.route("/rooms")
def rooms_page():
    """Available classrooms finder at any given time slot."""
    today_str = date.today().isoformat()
    now_dt = datetime.now()
    now_hour = now_dt.hour
    now_minute = now_dt.minute

    # If ?now=1 is specified or requested
    is_now = request.args.get("now") in ("1", "true", "yes")
    if is_now:
        start_h = f"{now_hour:02d}:{now_minute:02d}"
        end_hour = (now_hour + 2) % 24
        end_h = f"{end_hour:02d}:{now_minute:02d}"
    else:
        start_h = f"{now_hour:02d}:00" if 8 <= now_hour <= 19 else "14:00"
        end_h = f"{(now_hour + 2):02d}:00" if 8 <= now_hour <= 19 else "16:00"

    return render_template(
        "rooms.html",
        today_date=today_str,
        default_start_time=start_h,
        default_end_time=end_h,
        is_now=is_now,
        active_page="rooms",
    )


# =============================================================================
# Progressive Web App (PWA) & Offline Shell Routes
# =============================================================================

@app.route("/manifest.json")
def pwa_manifest():
    """Serve the Progressive Web App manifest."""
    return send_from_directory(
        os.path.join(app.root_path, "static"),
        "manifest.json",
        mimetype="application/manifest+json",
    )


@app.route("/sw.js")
def service_worker():
    """Serve the root Service Worker script with unrestricted root scope."""
    response = send_from_directory(
        os.path.join(app.root_path, "static"),
        "sw.js",
        mimetype="application/javascript",
    )
    response.headers["Service-Worker-Allowed"] = "/"
    response.headers["Cache-Control"] = "no-cache, no-store, must-revalidate"
    return response


@app.route("/offline")
def offline_page():
    """Offline fallback page served by the service worker."""
    return render_template("offline.html")



# =============================================================================
# API Endpoints: Auriga Fetch & User Consent
# =============================================================================

@app.route("/api/fetch-planning", methods=["POST"])
def fetch_planning():
    """
    Authenticates to Auriga, retrieves normalized courses,
    and with user consent, ingests them into the collaborative database.
    """
    trusted_proxy = os.getenv("TRUSTED_PROXY", "false").lower() in ("true", "1")
    if trusted_proxy and request.headers.get("X-Forwarded-For"):
        client_ip = request.headers.get("X-Forwarded-For").split(",")[0].strip()
    else:
        client_ip = request.remote_addr or "unknown"

    if not check_login_rate_limit(client_ip):
        return jsonify({
            "success": False,
            "error": "Trop de tentatives de connexion. Veuillez patienter une minute avant de réessayer."
        }), 429

    data = request.get_json(silent=True) or {}
    username = (data.get("username") or "").strip()
    password = (data.get("password") or "").strip()
    access_token = (data.get("access_token") or "").strip()
    start_date = (data.get("start_date") or "").strip()
    end_date = (data.get("end_date") or "").strip()
    consent = bool(data.get("consent", True))
    if access_token:
        if access_token.lower().startswith("bearer "):
            access_token = access_token[7:].strip()
        access_token = access_token.strip('"\'')
        if "\u2026" in access_token or "..." in access_token:
            return jsonify({
                "success": False,
                "error": "Le jeton collé est incomplet : il a été tronqué par votre navigateur (présence de points de suspension '…'). Dans l'onglet Réseau (Network) de vos DevTools (F12), faites un Clic droit sur le header 'Authorization' -> 'Copier la valeur' (Copy value)."
            }), 400
        try:
            access_token.encode("ascii")
        except UnicodeEncodeError:
            return jsonify({
                "success": False,
                "error": "Le jeton collé contient des caractères non autorisés. Veuillez vous assurer d'avoir copié le jeton JWT complet sans caractères spéciaux."
            }), 400

        try:
            parts = access_token.split(".")
            if len(parts) == 3:
                payload_b64 = parts[1] + "=" * ((4 - len(parts[1]) % 4) % 4)
                jwt_data = json.loads(base64.urlsafe_b64decode(payload_b64))
                exp = jwt_data.get("exp")
                if exp and time.time() > exp:
                    return jsonify({
                        "success": False,
                        "error": "Ce jeton d'accès Auriga a expiré (durée de validité de 5 minutes dépassée). Veuillez rafraîchir votre page Auriga (F5) et copier immédiatement le nouveau jeton, ou utilisez l'onglet 'Identifiant & Mot de passe' pour une connexion automatique permanente."
                    }), 400
                token_user = jwt_data.get("preferred_username")
                if not username and token_user:
                    username = str(token_user)
        except Exception:
            pass

    if not access_token and (not username or not password):
        return jsonify({"success": False, "error": "Veuillez renseigner vos identifiants ou un jeton d'accès."}), 400

    default_start, default_end = get_default_school_year_range()
    start_date = start_date or default_start
    end_date = end_date or default_end

    try:
        if access_token:
            client = AurigaClient(access_token=access_token)
        else:
            client = AurigaClient(username, password)
            client.login()
        raw_interventions = client.get_planning(start_date, end_date)
    except Exception as e:
        return jsonify({"success": False, "error": str(e)}), 400

    normalized_courses = [normalize_course(item) for item in raw_interventions]

    # Ingest into collaborative database if user consented
    if consent and normalized_courses:
        try:
            database.ingest_courses(normalized_courses, consent=True)
        except Exception as e:
            app.logger.warning(f"Database ingestion warning: {e}")

    # Compute statistics
    types_count = {}
    cancelled_count = 0
    for c in normalized_courses:
        t = c["activity_type"] or "AUTRE"
        types_count[t] = types_count.get(t, 0) + 1
        if c["is_cancelled"]:
            cancelled_count += 1

    stats = {
        "total": len(normalized_courses),
        "total_courses": len(normalized_courses),
        "by_type": types_count,
        "cancelled": cancelled_count,
    }

    # Establish authenticated user session with fresh session state
    session.clear()
    session.permanent = True
    session["logged_in"] = True
    user_display = username.split("@")[0] if "@" in username else (username or "Étudiant Auriga")
    session["username"] = user_display

    return jsonify({
        "success": True,
        "courses": normalized_courses,
        "stats": stats,
        "username": user_display,
    })


@app.route("/api/ingest-consent", methods=["POST"])
def ingest_consent():
    """Allows user to share their courses if they decide to contribute after fetching."""
    data = request.get_json(silent=True) or {}
    courses = data.get("courses", [])
    if not courses:
        return jsonify({"success": False, "error": "Aucun cours à enregistrer."}), 400

    try:
        res = database.ingest_courses(courses, consent=True)
        return jsonify({"success": True, "result": res})
    except Exception as e:
        return jsonify({"success": False, "error": str(e)}), 500


# =============================================================================
# API Endpoints: Database Knowledge Base & Custom Queries
# =============================================================================

@app.route("/api/database/stats", methods=["GET"])
def db_stats():
    """Returns database knowledge base statistics."""
    return jsonify(database.get_database_stats())


@app.route("/api/database/options", methods=["GET"])
def db_options():
    """Returns available teachers, rooms, groups, and activity types for dropdowns."""
    return jsonify({
        "teachers": database.get_all_teachers(),
        "rooms": database.get_all_rooms(),
        "groups": database.get_all_groups(),
        "groups_structured": database.get_all_groups_structured(),
        "promotions": database.get_structured_promotions_and_groups(),
        "types": database.get_all_activity_types(),
    })


@app.route("/api/database/promos", methods=["GET"])
def db_promos():
    """Returns the recognized EPITA promotions hierarchy (SUP, SPÉ, ING1, ING3/Majeures)."""
    return jsonify({
        "success": True,
        "promotions": database.get_structured_promotions_and_groups(),
    })


@app.route("/api/database/recurring-courses", methods=["GET"])
def db_recurring_courses():
    """Returns detected recurring course patterns and weekly schedule template."""
    promo_id = request.args.get("promo") or None
    subgroup = request.args.get("subgroup") or None
    teacher = request.args.get("teacher") or None
    keyword = request.args.get("keyword") or None
    day_str = request.args.get("day")
    day_of_week = int(day_str) if day_str is not None and day_str.isdigit() else None

    result = database.query_recurring_courses(
        promo_id=promo_id,
        subgroup=subgroup,
        day_of_week=day_of_week,
        teacher=teacher,
        keyword=keyword,
    )
    return jsonify(result)


@app.route("/api/database/detect-patterns", methods=["POST"])
def db_detect_patterns():
    """Triggers the recurring course pattern recognition algorithm."""
    result = database.detect_and_sync_recurring_courses()
    return jsonify(result)



@app.route("/api/database/custom-courses", methods=["POST"])
def custom_courses_query():
    """Queries courses matching custom selected teachers, rooms, groups, or keywords."""
    data = request.get_json(silent=True) or {}
    teachers = data.get("teachers") or None
    rooms = data.get("rooms") or None
    groups = data.get("groups") or None
    activity_types = data.get("activity_types") or None
    keyword = data.get("keyword") or None
    start_date = data.get("start_date") or None
    end_date = data.get("end_date") or None
    hide_cancelled = bool(data.get("hide_cancelled", False))

    courses = database.query_custom_courses(
        teachers=teachers,
        rooms=rooms,
        groups=groups,
        activity_types=activity_types,
        keyword=keyword,
        start_date=start_date,
        end_date=end_date,
        hide_cancelled=hide_cancelled,
    )

    return jsonify({
        "success": True,
        "courses": courses,
        "count": len(courses),
    })


@app.route("/api/database/free-rooms", methods=["POST"])
def find_free_rooms():
    """Checks classroom availability on a specific date and time interval."""
    data = request.get_json(silent=True) or {}
    target_date = data.get("date") or date.today().isoformat()
    start_time = data.get("start_time") or "14:00"
    end_time = data.get("end_time") or "16:00"

    result = database.query_free_rooms(
        target_date=target_date,
        start_time=start_time,
        end_time=end_time,
    )

    return jsonify(result)


@app.route("/api/groups/tree", methods=["GET"])
def get_group_tree():
    """Returns the EPITA organizational tree for the treeview."""
    return jsonify(database.get_epita_group_tree())


@app.route("/api/groups/schedule", methods=["POST"])
def get_group_schedule():
    """Queries courses for selected treeview groups."""
    data = request.get_json(silent=True) or {}
    patterns = data.get("patterns") or []
    start_date = data.get("start_date") or None
    end_date = data.get("end_date") or None

    courses = database.query_courses_by_groups(
        patterns=patterns,
        start_date=start_date,
        end_date=end_date,
    )
    return jsonify({
        "success": True,
        "courses": courses,
        "count": len(courses),
    })


# =============================================================================
# API Endpoints: ICS Export & Live Google Calendar Feeds
# =============================================================================

@app.route("/api/export-ics", methods=["POST"])
def export_ics():
    """Generates and downloads the .ics file from selected courses."""
    data = request.get_json(silent=True) or {}
    courses = data.get("courses", [])
    calendar_name = (data.get("calendar_name") or "EpiKairos - EPITA Emploi du temps").strip()[:120]

    if not courses:
        return jsonify({"success": False, "error": "Aucun cours sélectionné pour l'exportation."}), 400

    ics_content = generate_ics_from_courses(courses, calendar_name)

    return Response(
        ics_content,
        mimetype="text/calendar; charset=utf-8",
        headers={
            "Content-Disposition": 'attachment; filename="epikairos_planning.ics"',
            "Cache-Control": "no-cache, no-store, must-revalidate",
        },
    )


@app.route("/api/create-feed", methods=["POST"])
def create_feed():
    """Creates a persistent auto-updating feed URL for Google Calendar."""
    data = request.get_json(silent=True) or {}
    username = (data.get("username") or "").strip()
    password = (data.get("password") or "").strip()
    include_regex = (data.get("include_regex") or "").strip() or None
    exclude_regex = (data.get("exclude_regex") or "").strip() or None
    allowed_types = data.get("allowed_types")
    allowed_courses = data.get("allowed_courses")
    hide_cancelled = bool(data.get("hide_cancelled", False))
    calendar_name = (data.get("calendar_name") or "EpiKairos - EPITA Emploi du temps").strip()[:120]
    courses = data.get("courses", [])

    # Validate regex lengths and syntax to prevent ReDoS
    for r_label, r_val in [("d'inclusion", include_regex), ("d'exclusion", exclude_regex)]:
        if r_val:
            if len(r_val) > 300:
                return jsonify({"success": False, "error": f"L'expression Regex {r_label} est trop longue (max 300 caractères)."}), 400
            try:
                import re as _re
                _re.compile(r_val)
            except _re.error as e:
                return jsonify({"success": False, "error": f"Expression Regex {r_label} invalide : {e}"}), 400

    if not courses and (not username or not password):
        return jsonify({
            "success": False,
            "error": "Veuillez renseigner vos identifiants Auriga ou vous assurer que votre planning contient des cours.",
        }), 400

    initial_ics = None
    if courses:
        initial_ics = generate_ics_from_courses(courses, calendar_name)

    feed_id = feed_store.create_feed(
        username=username,
        password=password,
        include_regex=include_regex,
        exclude_regex=exclude_regex,
        allowed_types=allowed_types,
        allowed_courses=allowed_courses,
        hide_cancelled=hide_cancelled,
        calendar_name=calendar_name,
        initial_ics=initial_ics,
        cached_courses=courses,
    )

    # Encode active filters into the ICS URL query params so they are explicitly set in the URL
    params = {}
    if include_regex:
        params["include"] = include_regex
    if exclude_regex:
        params["exclude"] = exclude_regex
    if hide_cancelled:
        params["hide_cancelled"] = "1"

    query_string = urllib.parse.urlencode(params)
    base_feed_url = url_for("serve_feed", feed_id=feed_id, _external=True)
    feed_url = f"{base_feed_url}?{query_string}" if query_string else base_feed_url
    webcal_url = feed_url.replace("http://", "webcal://").replace("https://", "webcal://")

    # Build human-readable filter summary for user verification
    filter_summary = {
        "include_regex": include_regex or "Aucun",
        "exclude_regex": exclude_regex or "Aucun",
        "hide_cancelled": hide_cancelled,
        "allowed_courses_count": len(allowed_courses) if isinstance(allowed_courses, list) else None,
        "total_courses": len(courses),
        "sync_frequency": "1 à 2 fois par jour (toutes les 12h)",
        "has_credentials": bool(username and password),
        "username": username if (username and password) else "",
    }

    return jsonify({
        "success": True,
        "feed_id": feed_id,
        "feed_url": feed_url,
        "webcal_url": webcal_url,
        "filter_summary": filter_summary,
    })


@app.route("/feed/<feed_id>.ics", methods=["GET"])
def serve_feed(feed_id: str):
    """
    Serves the live iCalendar feed to external subscribers (Google Calendar, Apple, Outlook).
    Respects query parameter filter overrides and auto-sync headers (1-2 times a day).
    """
    import re as _re
    if not _re.match(r"^[A-Za-z0-9_\-]{1,64}$", feed_id):
        return Response("Identifiant de flux invalide.", status=400, mimetype="text/plain")

    feed = feed_store.get_feed(feed_id)
    if not feed:
        return Response("Flux de calendrier introuvable.", status=404, mimetype="text/plain")

    # Read filter overrides from URL query parameters (if provided)
    include_regex = request.args.get("include") or request.args.get("include_regex") or feed.get("include_regex")
    exclude_regex = request.args.get("exclude") or request.args.get("exclude_regex") or feed.get("exclude_regex")

    # Validate regex lengths to prevent ReDoS
    if include_regex and len(include_regex) > 300:
        include_regex = include_regex[:300]
    if exclude_regex and len(exclude_regex) > 300:
        exclude_regex = exclude_regex[:300]

    if "hide_cancelled" in request.args:
        hide_cancelled = request.args.get("hide_cancelled", "").lower() in ("1", "true", "yes")
    else:
        hide_cancelled = feed.get("hide_cancelled", False)

    req_courses = request.args.getlist("course") or request.args.getlist("courses")
    if not req_courses and request.args.get("allowed_courses"):
        req_courses = [c.strip() for c in request.args.get("allowed_courses").split("\t") if c.strip()]
    allowed_courses = req_courses if req_courses else feed.get("allowed_courses")

    now = time.time()
    cache_age = now - feed.get("last_cached_at", 0)

    has_query_overrides = any(k in request.args for k in ("include", "include_regex", "exclude", "exclude_regex", "hide_cancelled", "course", "courses", "allowed_courses"))

    # If no custom query overrides and cache is fresh (< 12 hours), return cached ICS directly
    if not has_query_overrides and feed.get("last_cached_ics") and (cache_age < FEED_CACHE_DURATION):
        return Response(
            feed["last_cached_ics"],
            mimetype="text/calendar; charset=utf-8",
            headers={
                "Content-Disposition": 'inline; filename="planning.ics"',
                "Cache-Control": f"public, max-age={FEED_CACHE_DURATION}, must-revalidate",
            },
        )

    # We need to refresh or apply query parameters
    raw_courses = feed.get("cached_courses") or []

    # Refresh from Auriga if credentials exist and cache is older than FEED_CACHE_DURATION
    username = feed.get("username")
    password = feed.get("password")
    if username and password and (cache_age >= FEED_CACHE_DURATION or not raw_courses):
        try:
            client = AurigaClient(username, password)
            client.login()
            start_date, end_date = get_default_school_year_range()
            raw_interventions = client.get_planning(start_date, end_date)
            normalized = [normalize_course(item) for item in raw_interventions]
            if normalized:
                raw_courses = normalized
                feed_store.update_feed_courses(feed_id, normalized)
        except Exception as e:
            app.logger.warning(f"Could not refresh Auriga feed {feed_id} from remote: {e}")

    # Apply filters to courses
    if raw_courses:
        filtered, _ = filter_courses(
            raw_courses,
            include_regex=include_regex,
            exclude_regex=exclude_regex,
            allowed_types=feed.get("allowed_types"),
            allowed_courses=allowed_courses,
            hide_cancelled=hide_cancelled,
        )
        fresh_ics = generate_ics_from_courses(filtered, feed.get("calendar_name", "EpiKairos - EPITA Emploi du temps"))
        if not has_query_overrides:
            feed_store.update_feed_cache(feed_id, fresh_ics)
        return Response(
            fresh_ics,
            mimetype="text/calendar; charset=utf-8",
            headers={
                "Content-Disposition": 'inline; filename="planning.ics"',
                "Cache-Control": f"public, max-age={FEED_CACHE_DURATION}, must-revalidate",
            },
        )

    # Fallback to last cached ICS
    if feed.get("last_cached_ics"):
        return Response(
            feed["last_cached_ics"],
            mimetype="text/calendar; charset=utf-8",
            headers={
                "Content-Disposition": 'inline; filename="planning.ics"',
                "Cache-Control": f"public, max-age={FEED_CACHE_DURATION}, must-revalidate",
            },
        )

    return Response("Erreur lors de la mise à jour du planning ou aucun cours disponible.", status=500, mimetype="text/plain")


@app.route("/health")
def health():
    """Healthcheck endpoint for monitoring."""
    return jsonify({"status": "ok", "time": time.time()})


if __name__ == "__main__":
    port = int(os.getenv("PORT", 5000))
    host = os.getenv("HOST", "0.0.0.0")
    print(f"[*] Starting EpiKairos Web App on http://{host}:{port}")
    app.run(host=host, port=port, debug=False)
