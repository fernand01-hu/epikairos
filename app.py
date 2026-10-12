#!/usr/bin/env python3
"""
Auriga Calendar Web Service
===========================
Self-hosted web application for EPITA students to connect to Auriga,
preview & filter courses with regex, and export or subscribe in Google Calendar.
"""

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
)

from auriga_core import (
    AurigaClient,
    get_default_school_year_range,
    normalize_course,
    filter_courses,
    generate_ics_from_courses,
)
import feed_store

app = Flask(__name__)
app.config["SECRET_KEY"] = os.getenv("FLASK_SECRET_KEY", "auriga-epita-secret-key-change-in-prod")
app.config["MAX_CONTENT_LENGTH"] = 16 * 1024 * 1024  # 16 MB max payload

# Cache duration for live Google Calendar feeds (in seconds: default 2 hours)
FEED_CACHE_DURATION = int(os.getenv("FEED_CACHE_DURATION", 7200))


@app.route("/")
def index():
    """Serves the main web interface."""
    start_date, end_date = get_default_school_year_range()
    return render_template(
        "index.html",
        default_start=start_date,
        default_end=end_date,
    )


@app.route("/api/fetch-planning", methods=["POST"])
def fetch_planning():
    """Authenticates to Auriga and retrieves normalized courses."""
    data = request.get_json(silent=True) or {}
    username = (data.get("username") or "").strip()
    password = (data.get("password") or "").strip()
    access_token = (data.get("access_token") or "").strip()
    start_date = (data.get("start_date") or "").strip()
    end_date = (data.get("end_date") or "").strip()

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


@app.route("/api/filter-preview", methods=["POST"])
def filter_preview():
    """Applies regex and category filters to courses on the server."""
    data = request.get_json(silent=True) or {}
    courses = data.get("courses", [])
    include_regex = data.get("include_regex")
    exclude_regex = data.get("exclude_regex")
    allowed_types = data.get("allowed_types")
    hide_cancelled = bool(data.get("hide_cancelled", False))

    filtered, err = filter_courses(
        courses,
        include_regex=include_regex,
        exclude_regex=exclude_regex,
        allowed_types=allowed_types,
        hide_cancelled=hide_cancelled,
    )

    if err:
        return jsonify({"success": False, "error": err}), 400

    return jsonify({
        "success": True,
        "filtered": filtered,
        "count": len(filtered),
    })


@app.route("/api/export-ics", methods=["POST"])
def export_ics():
    """Generates and downloads the .ics file from the selected courses."""
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

    # Generate initial cached ICS if courses provided
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
    """
    Serves the live iCalendar feed to external subscribers like Google Calendar.
    Caches results to avoid hammering Auriga.
    """
    feed = feed_store.get_feed(feed_id)
    if not feed:
        return Response("Flux de calendrier introuvable.", status=404, mimetype="text/plain")

    now = time.time()
    # Check if cached version is recent
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
        # If refreshing fails but we have an old cached version, return it gracefully
        if feed["last_cached_ics"]:
            return Response(
                feed["last_cached_ics"],
                mimetype="text/calendar; charset=utf-8",
                headers={"Content-Disposition": 'inline; filename="planning.ics"'},
            )
        return Response(f"Erreur lors de la mise à jour du planning : {e}", status=500, mimetype="text/plain")


@app.route("/health")
def health():
    """Healthcheck endpoint for Docker / reverse proxies."""
    return jsonify({"status": "ok", "time": time.time()})


if __name__ == "__main__":
    port = int(os.getenv("PORT", 5000))
    host = os.getenv("HOST", "0.0.0.0")
    print(f"[*] Starting Auriga Calendar Web App on http://{host}:{port}")
    app.run(host=host, port=port, debug=False)
