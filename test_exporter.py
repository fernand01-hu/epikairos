#!/usr/bin/env python3
"""
Unit tests for auriga_core.py and auriga_to_ics.py
"""

import hashlib
import unittest
from auriga_core import (
    base64url_encode,
    generate_pkce_pair,
    escape_ics_text,
    fold_line,
    parse_iso_datetime,
    format_ics_datetime,
    normalize_course,
    filter_courses,
    course_to_vevent,
    generate_ics_from_courses,
    PARIS_TZ,
)


class TestAurigaCore(unittest.TestCase):
    def test_pkce_generation(self):
        verifier, challenge = generate_pkce_pair()
        self.assertNotIn("=", verifier)
        self.assertNotIn("+", verifier)
        self.assertNotIn("/", verifier)
        self.assertNotIn("=", challenge)

        expected_digest = hashlib.sha256(verifier.encode("utf-8")).digest()
        expected_challenge = base64url_encode(expected_digest)
        self.assertEqual(challenge, expected_challenge)

    def test_escape_ics_text(self):
        raw = "Cours d'algo; salle 101, bât A\\B\nProf: M. Dupont\r\nAutre ligne"
        escaped = escape_ics_text(raw)
        self.assertIn("\\;", escaped)
        self.assertIn("\\,", escaped)
        self.assertIn("\\\\", escaped)
        self.assertIn("\\n", escaped)
        self.assertNotIn("\n", escaped)
        self.assertNotIn("\r", escaped)

    def test_fold_line(self):
        long_line = "DESCRIPTION:" + "A" * 120
        folded = fold_line(long_line, max_octets=75)
        lines = folded.split("\r\n")
        self.assertGreater(len(lines), 1)
        for i, l in enumerate(lines):
            self.assertLessEqual(len(l.encode("utf-8")), 75)
            if i > 0:
                self.assertTrue(l.startswith(" "))

    def test_datetime_formatting(self):
        dt1 = parse_iso_datetime("2024-10-15T08:30:00")
        self.assertEqual(format_ics_datetime(dt1), "20241015T083000")

        dt2 = parse_iso_datetime("2024-10-15T06:30:00Z")
        self.assertEqual(format_ics_datetime(dt2), "20241015T083000")

    def test_normalize_and_filter_courses(self):
        sample_interventions = [
            {
                "id": 1001,
                "startDateTime": "2024-10-15T08:30:00",
                "endDateTime": "2024-10-15T10:00:00",
                "duration": 90,
                "interventionPedagogicalUnits": [
                    {
                        "pedagogicalUnit": {
                            "code": "ALGO_1",
                            "caption": {"fr": "Algorithmique et structures de données"},
                        }
                    }
                ],
                "activityType": {
                    "code": "CM",
                    "caption": {"fr": "Cours Magistral"},
                    "isExam": False,
                },
                "interventionStatus": {"code": "PLANIFIE"},
                "interventionInstructors": [
                    {
                        "person": {
                            "currentFirstName": "Alan",
                            "currentLastName": "Turing",
                        }
                    }
                ],
                "interventionResources": [
                    {
                        "resource": {
                            "code": "LOC_KB_AMPHI_1",
                            "caption": {"fr": "Amphi 1"},
                            "floor": 0,
                        }
                    }
                ],
                "interventionPopulations": [
                    {
                        "population": {
                            "code": "ING1",
                            "caption": {"fr": "Promotion ING1 Promo 2027"},
                        }
                    }
                ],
            },
            {
                "id": 1002,
                "startDateTime": "2024-10-15T10:15:00",
                "endDateTime": "2024-10-15T12:15:00",
                "duration": 120,
                "interventionPedagogicalUnits": [
                    {
                        "pedagogicalUnit": {
                            "code": "MATH_S3",
                            "caption": {"fr": "Mathématiques appliquées"},
                        }
                    }
                ],
                "activityType": {
                    "code": "EXAM",
                    "caption": {"fr": "Partiel / Examen"},
                    "isExam": True,
                },
                "interventionStatus": {"code": "ANNULE"},
                "interventionInstructors": [],
                "interventionResources": [
                    {
                        "resource": {
                            "code": "LOC_KB_304",
                            "caption": {"fr": "Salle 304"},
                            "floor": 3,
                        }
                    }
                ],
                "interventionPopulations": [],
            },
        ]

        courses = [normalize_course(item) for item in sample_interventions]
        self.assertEqual(len(courses), 2)
        self.assertEqual(courses[0]["summary"], "[CM] Algorithmique et structures de données")
        self.assertEqual(courses[1]["summary"], "[ANNULÉ] [EXAM] Mathématiques appliquées")

        # Regex filtering
        filtered_algo, _ = filter_courses(courses, include_regex=r"ALGO|structures")
        self.assertEqual(len(filtered_algo), 1)
        self.assertEqual(filtered_algo[0]["id"], "1001")

        # Exclude regex
        filtered_no_math, _ = filter_courses(courses, exclude_regex=r"Mathématiques")
        self.assertEqual(len(filtered_no_math), 1)
        self.assertEqual(filtered_no_math[0]["id"], "1001")

        # Hide cancelled
        filtered_active, _ = filter_courses(courses, hide_cancelled=True)
        self.assertEqual(len(filtered_active), 1)
        self.assertEqual(filtered_active[0]["id"], "1001")

        # Type filter
        filtered_exam, _ = filter_courses(courses, allowed_types=["EXAM"])
        self.assertEqual(len(filtered_exam), 1)
        self.assertEqual(filtered_exam[0]["id"], "1002")

        # Course name filter
        filtered_by_course, _ = filter_courses(courses, allowed_courses=["Algorithmique et structures de données"])
        self.assertEqual(len(filtered_by_course), 1)
        self.assertEqual(filtered_by_course[0]["id"], "1001")

        # ICS Generation
        ics = generate_ics_from_courses(courses, "Test Calendar")
        self.assertIn("BEGIN:VCALENDAR\r\n", ics)
        self.assertIn("UID:auriga-1001@epita.fr\r\n", ics)
        self.assertIn("UID:auriga-1002@epita.fr\r\n", ics)
        self.assertIn("SUMMARY:[CM] Algorithmique et structures de données\r\n", ics)
        self.assertIn("SUMMARY:[ANNULÉ] [EXAM] Mathématiques appliquées\r\n", ics)


if __name__ == "__main__":
    unittest.main()
