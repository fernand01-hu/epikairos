#!/usr/bin/env python3
"""
Unit tests for database.py (School Knowledge Base)
"""

import os
import unittest
import database


class TestDatabaseModule(unittest.TestCase):
    def setUp(self):
        self.test_db = "test_school.db"
        if os.path.exists(self.test_db):
            os.remove(self.test_db)
        database.init_db(self.test_db)

    def tearDown(self):
        if os.path.exists(self.test_db):
            os.remove(self.test_db)

    def test_ingest_and_query_courses(self):
        sample_courses = [
            {
                "id": "c1",
                "start": "2026-10-15T08:30:00",
                "end": "2026-10-15T10:00:00",
                "duration": 90,
                "course_name": "Algorithmique et structures de données",
                "course_code": "ALGO_1",
                "activity_type": "CME",
                "activity_name": "Cours Magistral",
                "is_exam": False,
                "is_cancelled": False,
                "status": "PLANIFIE",
                "teachers": ["Alan Turing"],
                "rooms": ["Amphi 1"],
                "groups": ["ING1 Promo 2027"],
                "description": "Introduction aux graphes",
                "searchable_text": "Algorithmique Alan Turing Amphi 1 CME",
            },
            {
                "id": "c2",
                "start": "2026-10-15T10:15:00",
                "end": "2026-10-15T12:15:00",
                "duration": 120,
                "course_name": "Mathématiques Appliquées",
                "course_code": "MATH_S3",
                "activity_type": "TDE",
                "activity_name": "Travaux Dirigés",
                "is_exam": False,
                "is_cancelled": False,
                "status": "PLANIFIE",
                "teachers": ["Ada Lovelace"],
                "rooms": ["Salle 302"],
                "groups": ["ING1 Promo 2027", "G1"],
                "description": "Calcul matriciel",
                "searchable_text": "Mathématiques Ada Lovelace Salle 302 TDE",
            },
            {
                "id": "c3",
                "start": "2026-10-15T14:00:00",
                "end": "2026-10-15T16:00:00",
                "duration": 120,
                "course_name": "Architecture des Ordinateurs",
                "course_code": "ARCHI_1",
                "activity_type": "TPE",
                "activity_name": "Travaux Pratiques",
                "is_exam": False,
                "is_cancelled": False,
                "status": "PLANIFIE",
                "teachers": ["Alan Turing"],
                "rooms": ["Labo Info"],
                "groups": ["ING2"],
                "description": "VHDL",
                "searchable_text": "Architecture Alan Turing Labo Info TPE",
            },
        ]

        # Ingest without consent should fail
        res_no_consent = database.ingest_courses(sample_courses, consent=False, db_path=self.test_db)
        self.assertFalse(res_no_consent["success"])

        # Ingest with consent
        res = database.ingest_courses(sample_courses, consent=True, db_path=self.test_db)
        self.assertTrue(res["success"])
        self.assertEqual(res["inserted"], 3)

        # Verify database stats
        stats = database.get_database_stats(self.test_db)
        self.assertEqual(stats["total_courses"], 3)
        self.assertEqual(stats["total_teachers"], 2)  # Alan Turing, Ada Lovelace
        self.assertEqual(stats["total_rooms"], 3)     # Amphi 1, Salle 302, Labo Info
        self.assertEqual(stats["total_groups"], 3)    # ING1 Promo 2027, G1, ING2

        # Query teachers
        teachers = database.get_all_teachers(self.test_db)
        self.assertIn("Alan Turing", teachers)
        self.assertIn("Ada Lovelace", teachers)

        # Query by teacher "Alan Turing"
        turing_courses = database.query_custom_courses(teachers=["Alan Turing"], db_path=self.test_db)
        self.assertEqual(len(turing_courses), 2)
        c_names = [c["course_name"] for c in turing_courses]
        self.assertIn("Algorithmique et structures de données", c_names)
        self.assertIn("Architecture des Ordinateurs", c_names)

        # Query by room "Salle 302"
        salle_courses = database.query_custom_courses(rooms=["Salle 302"], db_path=self.test_db)
        self.assertEqual(len(salle_courses), 1)
        self.assertEqual(salle_courses[0]["course_name"], "Mathématiques Appliquées")

        # Query by type "CME"
        cme_courses = database.query_custom_courses(activity_types=["CME"], db_path=self.test_db)
        self.assertEqual(len(cme_courses), 1)

    def test_query_free_rooms(self):
        sample_courses = [
            {
                "id": "c10",
                "start": "2026-10-15T08:30:00",
                "end": "2026-10-15T10:00:00",
                "course_name": "Transistors",
                "activity_type": "TD",
                "is_cancelled": False,
                "rooms": ["Salle 303"],
                "teachers": ["M. Dupont"],
                "groups": ["ING1"],
            },
            {
                "id": "c11",
                "start": "2026-10-15T14:00:00",
                "end": "2026-10-15T16:00:00",
                "course_name": "Réseaux",
                "activity_type": "TD",
                "is_cancelled": False,
                "rooms": ["Salle 303"],
                "teachers": ["M. Dupont"],
                "groups": ["ING1"],
            },
            {
                "id": "c12",
                "start": "2026-10-15T10:00:00",
                "end": "2026-10-15T18:00:00",
                "course_name": "Projet Annuel",
                "activity_type": "PRJ",
                "is_cancelled": False,
                "rooms": ["Salle B12"],
                "teachers": ["Mme Martin"],
                "groups": ["ING2"],
            },
        ]

        database.ingest_courses(sample_courses, consent=True, db_path=self.test_db)

        # Check between 10:30 and 12:30 on 2026-10-15:
        # Salle 303 should be FREE (c10 ends at 10:00, c11 starts at 14:00)
        # Salle B12 should be OCCUPIED (c12 runs 10:00 to 18:00)
        res = database.query_free_rooms(
            target_date="2026-10-15",
            start_time="10:30",
            end_time="12:30",
            db_path=self.test_db,
        )

        free_room_names = [r["room_name"] for r in res["free_rooms"]]
        occupied_room_names = [r["room_name"] for r in res["occupied_rooms"]]

        self.assertIn("Salle 303", free_room_names)
        self.assertIn("Salle B12", occupied_room_names)

        # Check free_until for Salle 303
        salle_303_info = next(r for r in res["free_rooms"] if r["room_name"] == "Salle 303")
        self.assertEqual(salle_303_info["free_until"], "Jusqu'à 14:00")


if __name__ == "__main__":
    unittest.main()
