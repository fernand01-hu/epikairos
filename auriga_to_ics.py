#!/usr/bin/env python3
"""
Auriga to iCalendar (.ics) CLI Exporter for EPITA Students
==========================================================
CLI wrapper around auriga_core.
"""

import argparse
import datetime
import getpass
import os
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

from auriga_core import (
    AurigaClient,
    get_default_school_year_range,
    normalize_course,
    filter_courses,
    generate_ics_from_courses,
)


def load_env_file(filepath: str = ".env"):
    """Simple parser for .env files without extra dependencies."""
    if not os.path.exists(filepath):
        return
    try:
        with open(filepath, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#") or "=" not in line:
                    continue
                key, val = line.split("=", 1)
                key = key.strip()
                val = val.strip().strip("'\"")
                if key and key not in os.environ:
                    os.environ[key] = val
    except Exception:
        pass


class CalendarServerHandler(BaseHTTPRequestHandler):
    """Simple HTTP request handler to serve the .ics file over webcal/http."""
    ics_filepath = "auriga_planning.ics"

    def do_GET(self):
        if self.path in ("/", "/calendar.ics", "/planning.ics", "/auriga_planning.ics"):
            if not os.path.exists(self.ics_filepath):
                self.send_response(404)
                self.send_header("Content-Type", "text/plain; charset=utf-8")
                self.end_headers()
                self.wfile.write(b"Calendar file not found.")
                return

            with open(self.ics_filepath, "rb") as f:
                content = f.read()

            self.send_response(200)
            self.send_header("Content-Type", "text/calendar; charset=utf-8")
            self.send_header("Content-Length", str(len(content)))
            self.send_header("Content-Disposition", 'inline; filename="planning.ics"')
            self.send_header("Cache-Control", "no-cache, no-store, must-revalidate")
            self.end_headers()
            self.wfile.write(content)
        else:
            self.send_response(404)
            self.end_headers()


def run_http_server(filepath: str, port: int = 8080):
    """Runs a local HTTP server that provides a live subscription URL."""
    CalendarServerHandler.ics_filepath = filepath
    server_address = ("", port)
    httpd = HTTPServer(server_address, CalendarServerHandler)
    print(f"\n[+] Local calendar server running at http://localhost:{port}/planning.ics")
    print(f"[*] To subscribe in Google Calendar: Settings -> Add calendar -> From URL")
    print("[*] Press Ctrl+C to stop the server.")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping server.")
        httpd.server_close()


def main():
    parser = argparse.ArgumentParser(
        description="Export EPITA Auriga planning to an iCalendar (.ics) file for Google Calendar."
    )
    parser.add_argument(
        "-u", "--username", help="Auriga / EPITA login (e.g. prenom.nom)"
    )
    parser.add_argument(
        "-p", "--password", help="Auriga password"
    )
    parser.add_argument(
        "-o", "--output", default="auriga_planning.ics", help="Output .ics file path (default: auriga_planning.ics)"
    )
    parser.add_argument(
        "--start", help="Start date (YYYY-MM-DD). Defaults to start of academic year."
    )
    parser.add_argument(
        "--end", help="End date (YYYY-MM-DD). Defaults to end of academic year."
    )
    parser.add_argument(
        "--name", default="Auriga - EPITA Emploi du temps", help="Calendar name inside the .ics"
    )
    parser.add_argument(
        "--include-regex", help="Regex pattern to include only matching courses (e.g. 'MATH|ALGO')"
    )
    parser.add_argument(
        "--exclude-regex", help="Regex pattern to exclude matching courses (e.g. 'Anglais|Soutenance')"
    )
    parser.add_argument(
        "--types", nargs="+", help="Allowed activity types (e.g. CM TD TP EXAM)"
    )
    parser.add_argument(
        "--hide-cancelled", action="store_true", help="Exclude cancelled courses"
    )
    parser.add_argument(
        "--serve", type=int, nargs="?", const=8080, help="Serve the generated ICS file via a local HTTP server on the specified port (default: 8080)"
    )
    parser.add_argument(
        "--env-file", default=".env", help="Path to .env configuration file (default: .env)"
    )

    args = parser.parse_args()

    # Load environment variables
    load_env_file(args.env_file)

    username = args.username or os.getenv("AURIGA_USERNAME")
    password = args.password or os.getenv("AURIGA_PASSWORD")
    output_path = args.output or os.getenv("CALENDAR_OUTPUT", "auriga_planning.ics")
    calendar_name = args.name or os.getenv("CALENDAR_NAME", "Auriga - EPITA Emploi du temps")

    # Dates
    default_start, default_end = get_default_school_year_range()
    start_date = args.start or os.getenv("CALENDAR_START_DATE", default_start)
    end_date = args.end or os.getenv("CALENDAR_END_DATE", default_end)

    # Prompt if credentials missing
    if not username:
        print("=== Connexion Auriga (EPITA) ===")
        username = input("Identifiant Auriga (prenom.nom) : ").strip()
    if not password:
        password = getpass.getpass("Mot de passe : ").strip()

    if not username or not password:
        print("Error: Both username and password are required.", file=sys.stderr)
        sys.exit(1)

    print(f"\n[1/4] Connexion à Auriga via Keycloak (PKCE)...")
    client = AurigaClient(username, password)
    try:
        client.login()
        print("      Connexion réussie ! Token d'accès obtenu.")
    except Exception as e:
        print(f"\n[!] Erreur lors de l'authentification : {e}", file=sys.stderr)
        sys.exit(1)

    print(f"\n[2/4] Récupération du planning du {start_date} au {end_date}...")
    try:
        raw_interventions = client.get_planning(start_date, end_date)
        print(f"      {len(raw_interventions)} cours / interventions trouvés au total.")
    except Exception as e:
        print(f"\n[!] Erreur lors de la récupération du planning : {e}", file=sys.stderr)
        sys.exit(1)

    print(f"\n[3/4] Traitement et filtrage des cours...")
    normalized = [normalize_course(item) for item in raw_interventions]
    filtered, err = filter_courses(
        normalized,
        include_regex=args.include_regex,
        exclude_regex=args.exclude_regex,
        allowed_types=args.types,
        hide_cancelled=args.hide_cancelled,
    )

    if err:
        print(f"\n[!] Erreur de filtrage : {err}", file=sys.stderr)
        sys.exit(1)

    print(f"      {len(filtered)} cours conservés après filtrage.")

    print(f"\n[4/4] Génération du fichier iCalendar (.ics)...")
    ics_text = generate_ics_from_courses(filtered, calendar_name)

    with open(output_path, "w", encoding="utf-8") as f:
        f.write(ics_text)

    print(f"      Fichier enregistré avec succès dans : {output_path}")

    # Summary
    print("\n" + "=" * 50)
    print(" Exportation terminée avec succès !")
    print(f" Fichier : {output_path}")
    print(f" Nombre d'événements retenus : {len(filtered)} / {len(raw_interventions)}")
    print("=" * 50)
    print("\nComment importer dans Google Calendar :")
    print("1. Ouvrez Google Calendar (https://calendar.google.com).")
    print("2. Paramètres -> Importer et exporter -> Sélectionner un fichier sur votre ordinateur.")
    print(f"3. Choisissez '{output_path}' et validez.")

    # If --serve is requested
    if args.serve:
        run_http_server(output_path, port=args.serve)


if __name__ == "__main__":
    main()
