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

    def test_index_route(self):
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
            "rooms": ["Amphi 1 (0e étage)"],
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

        # Create feed
        res = self.client.post(
            "/api/create-feed",
            json={
                "username": "student.test",
                "password": "secret_password",
                "include_regex": "MATH",
                "courses": [sample_course],
            },
        )
        self.assertEqual(res.status_code, 200)
        data = res.get_json()
        self.assertTrue(data.get("success"))
        feed_id = data.get("feed_id")
        self.assertIsNotNone(feed_id)

        # Retrieve feed ICS
        feed_res = self.client.get(f"/feed/{feed_id}.ics")
        self.assertEqual(feed_res.status_code, 200)
        self.assertIn("text/calendar", feed_res.headers.get("Content-Type", ""))
        self.assertIn(b"BEGIN:VCALENDAR", feed_res.data)
        self.assertIn(b"UID:auriga-3001@epita.fr", feed_res.data)

        # Cleanup test feed
        feed_store.delete_feed(feed_id)

    def test_multi_page_routes(self):
        # Dashboard
        res = self.client.get("/")
        self.assertEqual(res.status_code, 200)
        self.assertIn(b"Base Collaborative", res.data)

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


if __name__ == "__main__":
    unittest.main()

