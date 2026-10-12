#!/usr/bin/env python3
"""
Auriga Calendar Web Service (Zeus v2)
=====================================
Self-hosted multi-page web platform for EPITA students:
1. Personal schedule preview & regex filtering with Google Calendar sync.
2. Shared knowledge base (courses, teachers, rooms, groups) with user consent.
3. Custom merged calendar generator (by teacher, promo, room).
4. Real-time available classroom finder (free rooms).
"""

from datetime import datetime, date
import os
import sys
import time
from flask import (
    Flask,
    render_template,
    request,
    jsonify,
    Response,
    send_file,
    url_for,
    redirect,
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

app = Flask(__name__)
app.config["SECRET_KEY"] = os.getenv("FLASK_SECRET_KEY", "auriga-epita-secret-key-change-in-prod")
app.config["MAX_CONTENT_LENGTH"] = 16 * 1024 * 1024  # 16 MB max payload

# Cache duration for live Google Calendar feeds (in seconds: default 2 hours)
FEED_CACHE_DURATION = int(os.getenv("FEED_CACHE_DURATION", 7200))

# Ensure database tables exist
database.init_db()


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
    )


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
    now_hour = datetime.now().hour
    start_h = f"{now_hour:02d}:00" if 8 <= now_hour <= 19 else "14:00"
    end_h = f"{(now_hour + 2):02d}:00" if 8 <= now_hour <= 19 else "16:00"

    return render_template(
        "rooms.html",
        today_date=today_str,
        default_start_time=start_h,
        default_end_time=end_h,
        active_page="rooms",
    )


# =============================================================================
# API Endpoints: Auriga Fetch & User Consent
# =============================================================================

@app.route("/api/fetch-planning", methods=["POST"])
def fetch_planning():
    """
    Authenticates to Auriga, retrieves normalized courses,
    and with user consent, ingests them into the collaborative database.
    """
    data = request.get_json(silent=True) or {}
    username = (data.get("username") or "").strip()
    password = (data.get("password") or "").strip()
    access_token = (data.get("access_token") or "").strip()
    start_date = (data.get("start_date") or "").strip()
    end_date = (data.get("end_date") or "").strip()
    consent = bool(data.get("consent", True))

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
        "types": types_count,
        "cancelled": cancelled_count,
    }

    return jsonify({
        "success": True,
        "courses": normalized_courses,
        "stats": stats,
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
        "types": database.get_all_activity_types(),
    })


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
    calendar_name = data.get("calendar_name") or "Auriga - EPITA Emploi du temps"

    if not courses:
        return jsonify({"success": False, "error": "Aucun cours sélectionné pour l'exportation."}), 400

    ics_content = generate_ics_from_courses(courses, calendar_name)

    return Response(
        ics_content,
        mimetype="text/calendar; charset=utf-8",
        headers={
            "Content-Disposition": 'attachment; filename="auriga_planning.ics"',
            "Cache-Control": "no-cache, no-store, must-revalidate",
        },
    )


@app.route("/api/create-feed", methods=["POST"])
def create_feed():
    """Creates a persistent auto-updating feed URL for Google Calendar."""
    data = request.get_json(silent=True) or {}
    username = (data.get("username") or "").strip()
    password = (data.get("password") or "").strip()
    include_regex = data.get("include_regex")
    exclude_regex = data.get("exclude_regex")
    allowed_types = data.get("allowed_types")
    hide_cancelled = bool(data.get("hide_cancelled", False))
    calendar_name = data.get("calendar_name") or "Auriga - EPITA Emploi du temps"
    courses = data.get("courses", [])

    if not username or not password:
        return jsonify({"success": False, "error": "Identifiant et mot de passe requis pour le flux d'abonnement."}), 400

    initial_ics = None
    if courses:
        initial_ics = generate_ics_from_courses(courses, calendar_name)

    feed_id = feed_store.create_feed(
        username=username,
        password=password,
        include_regex=include_regex,
        exclude_regex=exclude_regex,
        allowed_types=allowed_types,
        hide_cancelled=hide_cancelled,
        calendar_name=calendar_name,
        initial_ics=initial_ics,
    )

    feed_url = url_for("serve_feed", feed_id=feed_id, _external=True)
    webcal_url = feed_url.replace("http://", "webcal://").replace("https://", "webcal://")

    return jsonify({
        "success": True,
        "feed_id": feed_id,
        "feed_url": feed_url,
        "webcal_url": webcal_url,
    })


@app.route("/feed/<feed_id>.ics", methods=["GET"])
def serve_feed(feed_id: str):
    """Serves the live iCalendar feed to Google Calendar subscribers."""
    feed = feed_store.get_feed(feed_id)
    if not feed:
        return Response("Flux de calendrier introuvable.", status=404, mimetype="text/plain")

    now = time.time()
    if feed["last_cached_ics"] and (now - feed["last_cached_at"] < FEED_CACHE_DURATION):
        return Response(
            feed["last_cached_ics"],
            mimetype="text/calendar; charset=utf-8",
            headers={"Content-Disposition": 'inline; filename="planning.ics"'},
        )

    # Refresh from Auriga
    try:
        client = AurigaClient(feed["username"], feed["password"])
        client.login()
        start_date, end_date = get_default_school_year_range()
        raw_interventions = client.get_planning(start_date, end_date)
        normalized = [normalize_course(item) for item in raw_interventions]
        filtered, _ = filter_courses(
            normalized,
            include_regex=feed["include_regex"],
            exclude_regex=feed["exclude_regex"],
            allowed_types=feed["allowed_types"],
            hide_cancelled=feed["hide_cancelled"],
        )
        fresh_ics = generate_ics_from_courses(filtered, feed["calendar_name"])
        feed_store.update_feed_cache(feed_id, fresh_ics)
        return Response(
            fresh_ics,
            mimetype="text/calendar; charset=utf-8",
            headers={"Content-Disposition": 'inline; filename="planning.ics"'},
        )
    except Exception as e:
        if feed["last_cached_ics"]:
            return Response(
                feed["last_cached_ics"],
                mimetype="text/calendar; charset=utf-8",
                headers={"Content-Disposition": 'inline; filename="planning.ics"'},
            )
        return Response(f"Erreur lors de la mise à jour du planning : {e}", status=500, mimetype="text/plain")


@app.route("/health")
def health():
    """Healthcheck endpoint for monitoring."""
    return jsonify({"status": "ok", "time": time.time()})


if __name__ == "__main__":
    port = int(os.getenv("PORT", 5000))
    host = os.getenv("HOST", "0.0.0.0")
    print(f"[*] Starting Auriga Calendar Web App on http://{host}:{port}")
    app.run(host=host, port=port, debug=False)
