#!/usr/bin/env python3
"""
Test suite for Flask web application (app.py)
"""

import json
import unittest
from app import app
import feed_store


class TestFlaskApp(unittest.TestCase):
    def setUp(self):
        app.config["TESTING"] = True
        self.client = app.test_client()

    def login_client(self, username="student.test"):
        with self.client.session_transaction() as sess:
            sess["logged_in"] = True
            sess["username"] = username

    def test_index_route(self):
        # Unauthenticated: redirects to /login
        unauth_res = self.client.get("/")
        self.assertEqual(unauth_res.status_code, 302)
        self.assertIn("/login", unauth_res.headers.get("Location", ""))

        # Authenticated: accesses dashboard
        self.login_client()
        res = self.client.get("/")
        self.assertEqual(res.status_code, 200)
        self.assertIn(b"Auriga", res.data)
        self.assertIn(b"EPITA", res.data)

    def test_health_route(self):
        res = self.client.get("/health")
        self.assertEqual(res.status_code, 200)
        data = res.get_json()
        self.assertEqual(data.get("status"), "ok")

    def test_fetch_planning_missing_credentials(self):
        res = self.client.post(
            "/api/fetch-planning",
            json={"username": "", "password": ""},
        )
        self.assertEqual(res.status_code, 400)
        data = res.get_json()
        self.assertFalse(data.get("success"))

    def test_export_ics_route(self):
        self.login_client()
        sample_course = {
            "id": "2001",
            "start": "2024-10-16T08:30:00",
            "end": "2024-10-16T10:00:00",
            "duration": 90,
            "summary": "[CM] Algorithmique et structures de données",
            "course_name": "Algorithmique et structures de données",
            "course_code": "ALGO_1",
            "activity_type": "CM",
            "activity_name": "Cours Magistral",
            "is_exam": False,
            "is_cancelled": False,
            "status": "PLANIFIE",
            "teachers": ["Alan Turing"],
            "rooms": ["Amphi 1"],
            "groups": ["ING1"],
            "description": "",
            "searchable_text": "[CM] Algorithmique et structures de données",
        }

        res = self.client.post(
            "/api/export-ics",
            json={"courses": [sample_course], "calendar_name": "Test Web Calendar"},
        )
        self.assertEqual(res.status_code, 200)
        self.assertIn("text/calendar", res.headers.get("Content-Type", ""))
        self.assertIn(b"BEGIN:VCALENDAR", res.data)
        self.assertIn(b"UID:auriga-2001@epita.fr", res.data)
        self.assertIn(b"SUMMARY:[CM] Algorithmique et structures de", res.data)

    def test_feed_creation_and_serving(self):
        self.login_client()
        sample_course = {
            "id": "3001",
            "start": "2024-10-17T09:00:00",
            "end": "2024-10-17T11:00:00",
            "duration": 120,
            "summary": "[TD] Mathématiques",
            "course_name": "Mathématiques",
            "course_code": "MATH_1",
            "activity_type": "TD",
            "activity_name": "Travaux Dirigés",
            "is_exam": False,
            "is_cancelled": False,
            "status": "PLANIFIE",
            "teachers": ["Ada Lovelace"],
            "rooms": ["Salle 101"],
            "groups": ["ING1"],
            "description": "",
            "searchable_text": "[TD] Mathématiques",
        }

        # Create feed with filters
        res = self.client.post(
            "/api/create-feed",
            json={
                "username": "student.test",
                "password": "secret_password",
                "include_regex": "MATH",
                "hide_cancelled": True,
                "courses": [sample_course],
            },
        )
        self.assertEqual(res.status_code, 200)
        data = res.get_json()
        self.assertTrue(data.get("success"))
        feed_id = data.get("feed_id")
        self.assertIsNotNone(feed_id)
        # Check that filters are set directly in the feed URL query parameters
        self.assertIn("include=MATH", data.get("feed_url", ""))
        self.assertIn("hide_cancelled=1", data.get("feed_url", ""))
        self.assertIn("include=MATH", data.get("webcal_url", ""))
        self.assertEqual(data.get("filter_summary", {}).get("sync_frequency"), "1 à 2 fois par jour (toutes les 12h)")

        # Retrieve feed ICS
        feed_res = self.client.get(f"/feed/{feed_id}.ics")
        self.assertEqual(feed_res.status_code, 200)
        self.assertIn("text/calendar", feed_res.headers.get("Content-Type", ""))
        self.assertIn("max-age=43200", feed_res.headers.get("Cache-Control", ""))
        self.assertIn(b"BEGIN:VCALENDAR", feed_res.data)
        self.assertIn(b"X-PUBLISHED-TTL:PT12H", feed_res.data)
        self.assertIn(b"REFRESH-INTERVAL;VALUE=DURATION:PT12H", feed_res.data)
        self.assertIn(b"UID:auriga-3001@epita.fr", feed_res.data)

        # Test query parameter override (e.g. exclude MATH excludes the course)
        override_res = self.client.get(f"/feed/{feed_id}.ics?exclude=MATH")
        self.assertEqual(override_res.status_code, 200)
        self.assertNotIn(b"UID:auriga-3001@epita.fr", override_res.data)

        # Cleanup test feed
        feed_store.delete_feed(feed_id)

        # Test feed creation without credentials (from loaded courses directly)
        no_creds_res = self.client.post(
            "/api/create-feed",
            json={
                "courses": [sample_course],
            },
        )
        self.assertEqual(no_creds_res.status_code, 200)
        no_creds_data = no_creds_res.get_json()
        self.assertTrue(no_creds_data.get("success"))
        no_creds_id = no_creds_data.get("feed_id")
        no_creds_get = self.client.get(f"/feed/{no_creds_id}.ics")
        self.assertEqual(no_creds_get.status_code, 200)
        self.assertIn(b"UID:auriga-3001@epita.fr", no_creds_get.data)
        feed_store.delete_feed(no_creds_id)

    def test_multi_page_routes(self):
        self.login_client()
        # Accueil Dashboard on /
        res = self.client.get("/")
        self.assertEqual(res.status_code, 200)
        self.assertIn(b"Base Collaborative", res.data)

        # Collaborative Dashboard on /dashboard
        res = self.client.get("/dashboard")
        self.assertEqual(res.status_code, 200)
        self.assertIn(b"Base Collaborative", res.data)

        # Old Zeus page on /old-zeus
        res = self.client.get("/old-zeus")
        self.assertEqual(res.status_code, 200)
        self.assertIn(b"Old Zeus", res.data)

        # Login page
        res = self.client.get("/login")
        self.assertEqual(res.status_code, 200)
        self.assertIn(b"Connexion Auriga", res.data)

        # Personal page
        res = self.client.get("/personal")
        self.assertEqual(res.status_code, 200)
        self.assertIn(b"Mon Emploi du Temps", res.data)

        # Custom calendar page
        res = self.client.get("/custom-calendar")
        self.assertEqual(res.status_code, 200)
        self.assertIn(b"Agendas Sur-Mesure", res.data)

        # Rooms page
        res = self.client.get("/rooms")
        self.assertEqual(res.status_code, 200)
        self.assertIn(b"Salles Libres", res.data)

    def test_database_apis(self):
        self.login_client()
        # Options
        res = self.client.get("/api/database/options")
        self.assertEqual(res.status_code, 200)
        data = res.get_json()
        self.assertIn("teachers", data)
        self.assertIn("rooms", data)
        self.assertIn("groups", data)

        # Stats
        res = self.client.get("/api/database/stats")
        self.assertEqual(res.status_code, 200)
        data = res.get_json()
        self.assertIn("total_courses", data)

        # Free rooms
        res = self.client.post(
            "/api/database/free-rooms",
            json={"date": "2026-10-15", "start_time": "14:00", "end_time": "16:00"}
        )
        self.assertEqual(res.status_code, 200)
        data = res.get_json()
        self.assertIn("free_rooms", data)
        self.assertIn("occupied_rooms", data)

        # Custom courses query
        res = self.client.post(
            "/api/database/custom-courses",
            json={"keyword": "Transistors"}
        )
        self.assertEqual(res.status_code, 200)
        data = res.get_json()
        self.assertTrue(data.get("success"))

        # Promos hierarchy API
        res_promos = self.client.get("/api/database/promos")
        self.assertEqual(res_promos.status_code, 200)
        promos_data = res_promos.get_json()
        self.assertTrue(promos_data.get("success"))
        self.assertIn("promotions", promos_data)
        promo_codes = [p["code"] for p in promos_data["promotions"]]
        self.assertIn("SUP", promo_codes)
        self.assertIn("SPE", promo_codes)
        self.assertIn("ING1", promo_codes)

        # Recurring courses API
        res_rec = self.client.get("/api/database/recurring-courses")
        self.assertEqual(res_rec.status_code, 200)
        rec_data = res_rec.get_json()
        self.assertTrue(rec_data.get("success"))
        self.assertIn("patterns", rec_data)
        self.assertIn("weekly_grid", rec_data)

        # Pattern detection trigger API
        res_detect = self.client.post("/api/database/detect-patterns")
        self.assertEqual(res_detect.status_code, 200)
        detect_data = res_detect.get_json()
        self.assertTrue(detect_data.get("success"))

    def test_zeus_classic_routes(self):
        self.login_client()
        # Old Zeus UI on /old-zeus
        res = self.client.get("/old-zeus")
        self.assertEqual(res.status_code, 200)
        self.assertIn(b"Old Zeus", res.data)
        self.assertIn(b"fernand.huttois@epita.fr", res.data)
        self.assertNotIn(b"Interface classique inspir", res.data)
        self.assertNotIn(b"btnFindRoom", res.data)
        self.assertIn(b"btnToggleSidebar", res.data)
        self.assertIn(b"btnCloseSidebar", res.data)

        # Alias on /zeus
        res_alias = self.client.get("/zeus")
        self.assertEqual(res_alias.status_code, 200)
        self.assertIn(b"Old Zeus", res_alias.data)

        # Groups tree API
        tree_res = self.client.get("/api/groups/tree")
        self.assertEqual(tree_res.status_code, 200)
        tree_data = tree_res.get_json()
        self.assertEqual(tree_data.get("id"), "root")
        self.assertEqual(tree_data.get("label"), "EPITA")

        # Groups schedule API
        sched_res = self.client.post(
            "/api/groups/schedule",
            json={"patterns": ["S3 Lyon L3"]}
        )
        self.assertEqual(sched_res.status_code, 200)
        sched_data = sched_res.get_json()
        self.assertTrue(sched_data.get("success"))

    def test_pwa_and_offline_routes(self):
        # Manifest
        res_m = self.client.get("/manifest.json")
        self.assertEqual(res_m.status_code, 200)
        self.assertIn("application/manifest+json", res_m.headers.get("Content-Type", ""))
        manifest_data = json.loads(res_m.data.decode("utf-8"))
        self.assertEqual(manifest_data.get("short_name"), "EpiKairos")
        self.assertEqual(manifest_data.get("display"), "standalone")
        self.assertTrue(len(manifest_data.get("icons", [])) >= 2)
        res_m.close()

        # Service Worker
        res_sw = self.client.get("/sw.js")
        self.assertEqual(res_sw.status_code, 200)
        self.assertIn("javascript", res_sw.headers.get("Content-Type", ""))
        self.assertEqual(res_sw.headers.get("Service-Worker-Allowed"), "/")
        self.assertIn(b"CACHE_NAME", res_sw.data)
        res_sw.close()

        # Offline fallback page
        res_off = self.client.get("/offline")
        self.assertEqual(res_off.status_code, 200)
        self.assertIn("hors ligne".encode("utf-8"), res_off.data.lower())

    def test_rooms_instant_now_route(self):
        self.login_client()
        res = self.client.get("/rooms?now=1")
        self.assertEqual(res.status_code, 200)
        self.assertIn(b'data-is-now="1"', res.data)
        self.assertIn(b"Maintenant", res.data)

    def test_dark_mode_and_pwa_markup(self):
        self.login_client()
        res = self.client.get("/")
        self.assertEqual(res.status_code, 200)
        html = res.data.decode("utf-8")
        self.assertIn('data-bs-theme="light"', html)
        self.assertIn('id="themeToggleBtn"', html)
        self.assertIn('href="/manifest.json"', html)
        self.assertIn('navigator.serviceWorker.register', html)

    def test_authentication_middleware_and_logout(self):
        # 1. Unauthenticated user visits protected pages -> 302 redirecting to /login
        res_rooms = self.client.get("/rooms")
        self.assertEqual(res_rooms.status_code, 302)
        self.assertIn("/login", res_rooms.headers.get("Location", ""))

        res_personal = self.client.get("/personal")
        self.assertEqual(res_personal.status_code, 302)
        self.assertIn("/login", res_personal.headers.get("Location", ""))

        res_custom = self.client.get("/custom-calendar")
        self.assertEqual(res_custom.status_code, 302)
        self.assertIn("/login", res_custom.headers.get("Location", ""))

        # 2. Unauthenticated user calls protected API -> 401 JSON error
        res_api = self.client.get("/api/database/stats")
        self.assertEqual(res_api.status_code, 401)
        api_data = res_api.get_json()
        self.assertFalse(api_data.get("success"))
        self.assertEqual(api_data.get("redirect"), "/login")

        # 3. Access to login page is permitted without authentication
        res_login = self.client.get("/login")
        self.assertEqual(res_login.status_code, 200)
        self.assertIn(b"Connexion Auriga", res_login.data)

        # 4. Authenticate and verify protected page access
        self.login_client("fernand.huttois")
        res_auth_rooms = self.client.get("/rooms")
        self.assertEqual(res_auth_rooms.status_code, 200)

        # 5. Logout clears session and redirects
        res_logout = self.client.get("/logout")
        self.assertEqual(res_logout.status_code, 302)
        self.assertIn("logged_out=1", res_logout.headers.get("Location", ""))

        # Verify access is restricted again after logout
        res_after = self.client.get("/rooms")
        self.assertEqual(res_after.status_code, 302)

    def test_navbar_cleanups(self):
        self.login_client("fernand.huttois")
        res = self.client.get("/")
        self.assertEqual(res.status_code, 200)
        html = res.data.decode("utf-8")

        # Ensure 'Accueil' duplicate button is not in nav items
        self.assertNotIn('<i class="bi bi-speedometer2 me-1"></i> Accueil', html)

        # Ensure 'Direct' badge next to Salles Libres is removed
        self.assertNotIn('<span class="badge bg-success-subtle text-success border border-success rounded-pill px-2 py-0 ms-1 small" style="font-size: 0.65rem;">Direct</span>', html)

        # Ensure user account button exists next to GitHub link with logout
        self.assertIn("fernand.huttois", html)
        self.assertIn('href="/logout"', html)

    def test_security_headers_and_cookies(self):
        # Verify defensive HTTP headers
        res = self.client.get("/login")
        self.assertEqual(res.status_code, 200)
        self.assertEqual(res.headers.get("X-Content-Type-Options"), "nosniff")
        self.assertEqual(res.headers.get("X-Frame-Options"), "SAMEORIGIN")
        self.assertEqual(res.headers.get("X-XSS-Protection"), "1; mode=block")
        self.assertIn("strict-origin-when-cross-origin", res.headers.get("Referrer-Policy", ""))

        # Verify session cookie flags
        self.assertTrue(app.config.get("SESSION_COOKIE_HTTPONLY"))
        self.assertEqual(app.config.get("SESSION_COOKIE_SAMESITE"), "Lax")

    def test_feed_security_and_unauthenticated_access(self):
        # Create a feed while authenticated
        self.login_client()
        sample_course = {
            "id": "sec-4001",
            "start": "2024-11-01T10:00:00",
            "end": "2024-11-01T12:00:00",
            "duration": 120,
            "summary": "[CM] Sécurité Informatique",
            "course_name": "Sécurité Informatique",
            "course_code": "SEC",
            "activity_type": "CM",
            "activity_name": "Cours Magistral",
            "is_exam": False,
            "is_cancelled": False,
            "status": "PLANIFIE",
            "teachers": ["Expert Cyber"],
            "rooms": ["Salle 404"],
            "groups": ["ING1 Lyon"],
            "description": "Cours de sécurité",
            "searchable_text": "Sécurité Informatique CM Salle 404",
        }
        create_res = self.client.post(
            "/api/create-feed",
            json={"courses": [sample_course], "calendar_name": "Feed Securite"},
        )
        feed_id = create_res.get_json()["feed_id"]

        # 1. Clear session / become unauthenticated (external client like Google Calendar)
        self.client.get("/logout")

        # 2. Access /feed/<feed_id>.ics without any session cookies
        feed_res = self.client.get(f"/feed/{feed_id}.ics")
        self.assertEqual(feed_res.status_code, 200)
        self.assertIn(b"UID:auriga-sec-4001@epita.fr", feed_res.data)

        # 3. Invalid characters in feed ID should be rejected with 400
        bad_feed_res = self.client.get("/feed/bad*token!@.ics")
        self.assertEqual(bad_feed_res.status_code, 400)

        feed_store.delete_feed(feed_id)

    def test_input_validation_and_rate_limiting(self):
        self.login_client()
        # Regex too long (> 300 chars)
        excessive_regex = "A" * 350
        res_regex = self.client.post(
            "/api/create-feed",
            json={"courses": [], "username": "user", "password": "pw", "include_regex": excessive_regex},
        )
        self.assertEqual(res_regex.status_code, 400)
        self.assertIn("trop longue", res_regex.get_json().get("error", "").lower())

        # Invalid regex syntax
        res_syntax = self.client.post(
            "/api/create-feed",
            json={"courses": [], "username": "user", "password": "pw", "include_regex": "[unclosed("},
        )
        self.assertEqual(res_syntax.status_code, 400)
        self.assertIn("invalide", res_syntax.get_json().get("error", "").lower())

        # Rate limiting on /api/fetch-planning
        from app import _LOGIN_ATTEMPTS
        _LOGIN_ATTEMPTS.clear()

        # Send 10 attempts
        for _ in range(10):
            self.client.post("/api/fetch-planning", json={})

        # 11th attempt should trigger 429
        res_rate = self.client.post("/api/fetch-planning", json={})
        self.assertEqual(res_rate.status_code, 429)
        self.assertFalse(res_rate.get_json().get("success"))
        _LOGIN_ATTEMPTS.clear()


if __name__ == "__main__":
    unittest.main()



