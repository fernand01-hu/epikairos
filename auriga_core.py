"""
Auriga Core Module
==================
Shared logic for Keycloak PKCE authentication, Auriga API client,
course normalization, regex filtering, and RFC 5545 iCalendar generation.
"""

import base64
import datetime
import hashlib
import json
import os
import re
import secrets
import sys
import urllib.parse
import zoneinfo

try:
    import requests
except ImportError:
    requests = None


# ==============================================================================
# Configuration & Constants
# ==============================================================================

CLIENT_ID = "np-front"
REDIRECT_URI = "https://auriga.epita.fr/"
AUTH_BASE_URL = (
    "https://ionisepita-auth.np-auriga.nfrance.net/auth/realms/npionisepita/protocol/openid-connect/auth"
)
TOKEN_URL = (
    "https://ionisepita-auth.np-auriga.nfrance.net/auth/realms/npionisepita/protocol/openid-connect/token"
)
API_BASE_URL = "https://auriga.epita.fr/api"

PARIS_TZ = zoneinfo.ZoneInfo("Europe/Paris")
UTC_TZ = zoneinfo.ZoneInfo("UTC")

DEFAULT_USER_AGENT = (
    "Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) "
    "AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148"
)


# ==============================================================================
# Cryptography & Utilities
# ==============================================================================

def base64url_encode(data: bytes) -> str:
    """Encode bytes to URL-safe base64 without padding."""
    return base64.urlsafe_b64encode(data).decode("utf-8").rstrip("=")


def generate_pkce_pair():
    """Generate PKCE code_verifier and code_challenge (S256)."""
    verifier = base64url_encode(secrets.token_bytes(32))
    digest = hashlib.sha256(verifier.encode("utf-8")).digest()
    challenge = base64url_encode(digest)
    return verifier, challenge


def get_default_school_year_range():
    """Computes academic year date range (Sept 1st to Aug 31st)."""
    now = datetime.datetime.now(PARIS_TZ)
    if now.month >= 8:
        start_year = now.year
    else:
        start_year = now.year - 1
    return f"{start_year}-09-01", f"{start_year + 1}-08-31"


# ==============================================================================
# Auriga API Client
# ==============================================================================

class AurigaClient:
    """Handles Keycloak PKCE authentication and calls to the Auriga API."""

    def __init__(self, username: str = "", password: str = "", access_token: str | None = None):
        if requests is None:
            raise RuntimeError("The 'requests' library is required to use AurigaClient.")
        self.username = username
        self.password = password
        self.session = requests.Session()
        self.session.headers.update({
            "User-Agent": DEFAULT_USER_AGENT,
            "Accept-Language": "fr-FR,fr;q=0.9,en;q=0.8",
        })
        if access_token:
            clean_token = access_token.strip()
            if clean_token.lower().startswith("bearer "):
                clean_token = clean_token[7:].strip()
            clean_token = clean_token.strip('"\'')
            if "\u2026" in clean_token or "..." in clean_token:
                raise ValueError("Le jeton collé est tronqué ('…'). Dans vos DevTools, faites Clic droit -> 'Copier la valeur' pour obtenir le jeton complet.")
            self.access_token = clean_token
        else:
            self.access_token = None
        self.refresh_token: str | None = None

    def _perform_login_flow(self, user: str, pw: str) -> str:
        """Internal Keycloak PKCE flow execution."""
        code_verifier, code_challenge = generate_pkce_pair()
        state = base64url_encode(secrets.token_bytes(16))
        nonce = base64url_encode(secrets.token_bytes(16))

        auth_params = {
            "client_id": CLIENT_ID,
            "redirect_uri": REDIRECT_URI,
            "state": state,
            "response_mode": "query",
            "response_type": "code",
            "scope": "openid",
            "nonce": nonce,
            "prompt": "login",
            "code_challenge": code_challenge,
            "code_challenge_method": "S256",
        }

        # Step 1: Get login form
        login_page_res = self.session.get(
            AUTH_BASE_URL,
            params=auth_params,
            headers={"Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"}
        )
        if not login_page_res.ok:
            raise RuntimeError(f"Erreur HTTP {login_page_res.status_code} sur la page de connexion")

        html = login_page_res.text
        form_match = re.search(
            r'<form[^>]*id="kc-form-login"[^>]*action="([^"]+)"', html, re.IGNORECASE
        ) or re.search(
            r'<form[^>]*action="([^"]+)"[^>]*id="kc-form-login"', html, re.IGNORECASE
        )

        if not form_match:
            raise RuntimeError("Formulaire de connexion introuvable.")

        action_url = form_match.group(1).replace("&amp;", "&")
        action_url = urllib.parse.urljoin(login_page_res.url, action_url)

        # Step 2: Post user credentials
        login_data = {
            "username": user,
            "password": pw,
            "credentialId": "",
        }

        login_res = self.session.post(
            action_url,
            data=login_data,
            headers={"Content-Type": "application/x-www-form-urlencoded"},
            allow_redirects=False,
        )

        redirect_target = None
        if login_res.status_code in (301, 302, 303, 307, 308):
            redirect_target = login_res.headers.get("Location")
        elif login_res.status_code == 200:
            err_match = re.search(
                r'<span[^>]*class="[^"]*kc-feedback-text[^"]*"[^>]*>(.*?)</span>',
                login_res.text,
                re.DOTALL | re.IGNORECASE,
            ) or re.search(
                r'<div[^>]*class="[^"]*instruction[^"]*"[^>]*>(.*?)</div>',
                login_res.text,
                re.DOTALL | re.IGNORECASE,
            )
            msg = err_match.group(1).strip() if err_match else "Identifiants invalides ou échec de connexion"
            clean_msg = re.sub(r"<[^>]+>", "", msg).strip()
            raise ValueError(clean_msg)
        else:
            raise RuntimeError(f"Réponse inattendue lors de la connexion (HTTP {login_res.status_code})")

        if not redirect_target:
            raise RuntimeError("Aucune URL de redirection après soumission des identifiants.")

        # Step 3: Extract Authorization Code
        parsed_target = urllib.parse.urlparse(redirect_target)
        target_params = urllib.parse.parse_qs(parsed_target.query)
        auth_code = target_params.get("code", [None])[0]

        if not auth_code and parsed_target.fragment:
            fragment_params = urllib.parse.parse_qs(parsed_target.fragment)
            auth_code = fragment_params.get("code", [None])[0]

        if not auth_code:
            error_desc = target_params.get("error_description", [None])[0]
            raise RuntimeError(
                f"Code d'autorisation absent. "
                + (f"Détail : {error_desc}" if error_desc else f"Cible : {redirect_target}")
            )

        # Step 4: Exchange code for tokens
        token_data = {
            "client_id": CLIENT_ID,
            "grant_type": "authorization_code",
            "code": auth_code,
            "redirect_uri": REDIRECT_URI,
            "code_verifier": code_verifier,
        }

        token_res = self.session.post(
            TOKEN_URL,
            data=token_data,
            headers={
                "Content-Type": "application/x-www-form-urlencoded",
                "Accept": "application/json",
            },
        )

        if not token_res.ok:
            raise RuntimeError(f"Échec de l'échange du token (HTTP {token_res.status_code})")

        tokens = token_res.json()
        self.access_token = tokens.get("access_token")
        self.refresh_token = tokens.get("refresh_token")

        if not self.access_token:
            raise RuntimeError("Le serveur d'authentification n'a renvoyé aucun access_token.")

        return self.access_token

    def login(self) -> str:
        """Executes Keycloak OIDC PKCE flow. Retries with @epita.fr if needed."""
        if self.access_token:
            return self.access_token

        attempts = [self.username]
        if "@" not in self.username:
            attempts.append(f"{self.username}@epita.fr")

        last_error = ""
        for u in attempts:
            try:
                return self._perform_login_flow(u, self.password)
            except ValueError as e:
                last_error = str(e)
            except Exception as e:
                raise e

        # If we reach here, credentials were rejected
        hint = (
            "Échec de l'authentification : Nom d'utilisateur ou mot de passe invalide. "
            "(Rappel : Votre mot de passe Auriga IONIS est indépendant de votre mot de passe Microsoft EPITA / Office 365. "
            "Si vous ne l'avez jamais créé, allez sur auriga.epita.fr et cliquez sur 'Mot de passe oublié ?')."
        )
        raise RuntimeError(hint)

    def get_planning(self, start_date: str, end_date: str) -> list[dict]:
        """Fetches schedule interventions between start_date and end_date."""
        if not self.access_token:
            raise RuntimeError("Non authentifié. Appelez login() d'abord.")

        endpoint = (
            f"{API_BASE_URL}/plannings/me"
            f"?days=0&days=1&days=2&days=3&days=4&days=5&days=6&days=7"
            f"&startDate={start_date}&endDate={end_date}"
        )

        headers = {
            "Authorization": f"Bearer {self.access_token}",
            "Accept": "application/json",
        }

        res = self.session.get(endpoint, headers=headers)
        if res.status_code == 401:
            raise RuntimeError(
                "Accès refusé (401) par Auriga : Votre jeton d'accès est expiré ou invalide. "
                "Les jetons Auriga ont une durée de vie très courte (5 minutes). "
                "Veuillez rafraîchir Auriga (F5) et copier immédiatement le nouveau jeton, "
                "ou utilisez l'onglet 'Identifiant & Mot de passe' pour une connexion automatique permanente."
            )
        elif not res.ok:
            clean_err = re.sub(r"<[^>]+>", " ", res.text).strip()
            clean_err = re.sub(r"\s+", " ", clean_err)[:250]
            raise RuntimeError(f"Erreur API Auriga ({res.status_code}) : {clean_err}")

        data = res.json()
        if isinstance(data, list):
            return data
        elif isinstance(data, dict) and "interventions" in data:
            return data["interventions"]
        return []


# ==============================================================================
# Course Normalization & Regex Filtering
# ==============================================================================

def normalize_course(item: dict) -> dict:
    """Extracts structured and human-readable course metadata from an Auriga item."""
    item_id = str(item.get("id", "0"))
    start_raw = item.get("startDateTime") or ""
    end_raw = item.get("endDateTime") or ""

    # Subject / Course Details
    ped_units = item.get("interventionPedagogicalUnits") or []
    course_code = ""
    course_name = ""
    if isinstance(ped_units, list) and len(ped_units) > 0:
        unit = ped_units[0].get("pedagogicalUnit") or {}
        course_code = unit.get("code") or ""
        caption = unit.get("caption") or {}
        course_name = caption.get("fr") or caption.get("en") or course_code

    if not course_name:
        course_name = item.get("name", {}).get("name") if isinstance(item.get("name"), dict) else "Cours"

    # Activity Type (CM, TD, TP, EXAM...)
    act = item.get("activityType") or {}
    act_code = act.get("code") or "AUTRE"
    act_name = (act.get("caption") or {}).get("fr") or act_code
    is_exam = bool(act.get("isExam", False))

    # Status
    status_obj = item.get("interventionStatus") or {}
    status_code = status_obj.get("code", "PLANIFIE")
    is_cancelled = status_code in ("ANNULE", "CANCELLED")

    # Teachers
    instructors = item.get("interventionInstructors") or []
    teacher_names = []
    for inst in instructors:
        person = inst.get("person") or {}
        first = person.get("currentFirstName") or ""
        last = person.get("currentLastName") or ""
        full = f"{first} {last}".strip()
        if full:
            teacher_names.append(full)

    # Rooms / Locations
    resources = item.get("interventionResources") or []
    rooms = []
    for res in resources:
        resource = res.get("resource") or {}
        caption = (resource.get("caption") or {}).get("fr") or ""
        code = resource.get("code") or ""
        loc_str = caption if caption else code
        if loc_str:
            rooms.append(loc_str)

    # Populations / Groups
    populations = item.get("interventionPopulations") or []
    groups = []
    for pop in populations:
        p = pop.get("population") or {}
        caption = (p.get("caption") or {}).get("fr") or p.get("code") or ""
        if caption:
            groups.append(caption)

    prefix = f"[{act_code}] " if act_code else ""
    summary = f"{prefix}{course_name}"
    if is_cancelled:
        summary = f"[ANNULÉ] {summary}"

    # Build full searchable text for regex
    searchable_parts = [
        summary,
        course_name,
        course_code,
        act_code,
        act_name,
        " ".join(teacher_names),
        " ".join(rooms),
        " ".join(groups),
        status_code,
        item.get("description") or "",
    ]
    searchable_text = " ".join(filter(None, searchable_parts))

    return {
        "id": item_id,
        "start": start_raw,
        "end": end_raw,
        "duration": item.get("duration") or 90,
        "summary": summary,
        "course_name": course_name,
        "course_code": course_code,
        "activity_type": act_code,
        "activity_name": act_name,
        "is_exam": is_exam,
        "is_cancelled": is_cancelled,
        "status": status_code,
        "teachers": teacher_names,
        "rooms": rooms,
        "groups": groups,
        "description": item.get("description") or "",
        "searchable_text": searchable_text,
        "raw": item,
    }


def filter_courses(
    courses: list[dict],
    include_regex: str | None = None,
    exclude_regex: str | None = None,
    allowed_types: list[str] | None = None,
    allowed_courses: list[str] | None = None,
    hide_cancelled: bool = False,
    selected_ids: set[str] | None = None,
) -> tuple[list[dict], str | None]:
    """
    Filters a list of normalized courses using regex, types, courses, and cancelled status.
    Returns: (filtered_courses, error_message_if_any)
    """
    include_pattern = None
    exclude_pattern = None

    if include_regex and include_regex.strip():
        try:
            include_pattern = re.compile(include_regex.strip(), re.IGNORECASE)
        except re.error as e:
            return [], f"Erreur dans l'expression Regex d'inclusion : {e}"

    if exclude_regex and exclude_regex.strip():
        try:
            exclude_pattern = re.compile(exclude_regex.strip(), re.IGNORECASE)
        except re.error as e:
            return [], f"Erreur dans l'expression Regex d'exclusion : {e}"

    allowed_types_set = set(t.upper() for t in allowed_types) if allowed_types is not None else None
    allowed_courses_set = set(c.strip().lower() for c in allowed_courses) if allowed_courses is not None else None

    filtered = []
    for c in courses:
        # Check explicit ID selection
        if selected_ids is not None and c["id"] not in selected_ids:
            continue

        # Check cancelled filter
        if hide_cancelled and c["is_cancelled"]:
            continue

        # Check activity type filter
        if allowed_types_set is not None and c["activity_type"].upper() not in allowed_types_set:
            continue

        # Check course name filter
        if allowed_courses_set is not None and (c.get("course_name") or "").strip().lower() not in allowed_courses_set:
            continue

        text = c["searchable_text"]

        # Check include regex
        if include_pattern and not include_pattern.search(text):
            continue

        # Check exclude regex
        if exclude_pattern and exclude_pattern.search(text):
            continue

        filtered.append(c)

    return filtered, None


# ==============================================================================
# iCalendar (.ics) Generation
# ==============================================================================

def escape_ics_text(text: str) -> str:
    """Escapes special characters in text fields per RFC 5545."""
    if not text:
        return ""
    text = text.replace("\\", "\\\\")
    text = text.replace(";", "\\;")
    text = text.replace(",", "\\,")
    text = text.replace("\r\n", "\n").replace("\r", "\n").replace("\n", "\\n")
    return text


def fold_line(line: str, max_octets: int = 75) -> str:
    """Folds a long line according to RFC 5545 (<= 75 octets)."""
    encoded = line.encode("utf-8")
    if len(encoded) <= max_octets:
        return line

    parts = []
    limit = max_octets
    while len(encoded) > limit:
        cut = limit
        while cut > 0:
            try:
                encoded[:cut].decode("utf-8")
                break
            except UnicodeDecodeError:
                cut -= 1
        parts.append(encoded[:cut].decode("utf-8"))
        encoded = encoded[cut:]
        limit = 74
    if encoded:
        parts.append(encoded.decode("utf-8"))
    return "\r\n ".join(parts)


def parse_iso_datetime(dt_str: str) -> datetime.datetime:
    """Parses Auriga ISO datetime string into a Paris timezone aware datetime."""
    clean_str = dt_str.replace("Z", "+00:00")
    dt = datetime.datetime.fromisoformat(clean_str)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=PARIS_TZ)
    else:
        dt = dt.astimezone(PARIS_TZ)
    return dt


def format_ics_datetime(dt: datetime.datetime) -> str:
    """Formats datetime as YYYYMMDDTHHMMSS."""
    return dt.strftime("%Y%m%dT%H%M%S")


def course_to_vevent(course: dict, now_stamp: str) -> list[str]:
    """Converts a normalized course dictionary to RFC 5545 VEVENT lines."""
    item_id = course["id"]
    start_raw = course["start"]
    end_raw = course["end"]

    if not start_raw:
        return []

    try:
        dt_start = parse_iso_datetime(start_raw)
    except Exception:
        return []

    if end_raw:
        try:
            dt_end = parse_iso_datetime(end_raw)
        except Exception:
            dt_end = dt_start + datetime.timedelta(minutes=course.get("duration", 90))
    else:
        dt_end = dt_start + datetime.timedelta(minutes=course.get("duration", 90))

    summary_text = course["summary"]
    is_cancelled = course["is_cancelled"]

    # Build description
    desc_lines = []
    if course["course_name"]:
        desc_lines.append(
            f"Matière: {course['course_name']}"
            + (f" ({course['course_code']})" if course["course_code"] else "")
        )
    if course["activity_name"]:
        desc_lines.append(f"Type: {course['activity_name']} ({course['activity_type']})")
    if course["teachers"]:
        desc_lines.append(f"Enseignant(s): {', '.join(course['teachers'])}")
    if course["groups"]:
        desc_lines.append(f"Groupe(s): {', '.join(course['groups'])}")
    if course["rooms"]:
        desc_lines.append(f"Salle(s): {', '.join(course['rooms'])}")
    desc_lines.append(f"Statut: {course['status']}")

    if course.get("description"):
        desc_lines.append(f"Détails: {course['description']}")

    location_text = ", ".join(course["rooms"])
    description_text = "\n".join(desc_lines)

    lines = [
        "BEGIN:VEVENT",
        f"UID:auriga-{item_id}@epita.fr",
        f"DTSTAMP:{now_stamp}",
        f"DTSTART;TZID=Europe/Paris:{format_ics_datetime(dt_start)}",
        f"DTEND;TZID=Europe/Paris:{format_ics_datetime(dt_end)}",
        fold_line(f"SUMMARY:{escape_ics_text(summary_text)}"),
        fold_line(f"LOCATION:{escape_ics_text(location_text)}"),
        fold_line(f"DESCRIPTION:{escape_ics_text(description_text)}"),
        f"STATUS:{'CANCELLED' if is_cancelled else 'CONFIRMED'}",
        "END:VEVENT",
    ]

    return lines


def generate_ics_from_courses(
    courses: list[dict],
    calendar_name: str = "Auriga - EPITA Emploi du temps"
) -> str:
    """Generates the full RFC 5545 iCalendar content string from normalized courses."""
    now_utc = datetime.datetime.now(UTC_TZ)
    now_stamp = now_utc.strftime("%Y%m%dT%H%M%SZ")

    vtimezone = [
        "BEGIN:VTIMEZONE",
        "TZID:Europe/Paris",
        "X-LIC-LOCATION:Europe/Paris",
        "BEGIN:DAYLIGHT",
        "TZOFFSETFROM:+0100",
        "TZOFFSETTO:+0200",
        "TZNAME:CEST",
        "DTSTART:19700329T020000",
        "RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU",
        "END:DAYLIGHT",
        "BEGIN:STANDARD",
        "TZOFFSETFROM:+0200",
        "TZOFFSETTO:+0100",
        "TZNAME:CET",
        "DTSTART:19701025T030000",
        "RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU",
        "END:STANDARD",
        "END:VTIMEZONE",
    ]

    header = [
        "BEGIN:VCALENDAR",
        "VERSION:2.0",
        "PRODID:-//EpiKairos Calendar Exporter//EPITA//FR",
        "CALSCALE:GREGORIAN",
        "METHOD:PUBLISH",
        fold_line(f"X-WR-CALNAME:{escape_ics_text(calendar_name)}"),
        "X-WR-TIMEZONE:Europe/Paris",
        "X-PUBLISHED-TTL:PT12H",
        "REFRESH-INTERVAL;VALUE=DURATION:PT12H",
    ]

    event_lines = []
    for c in courses:
        vevent = course_to_vevent(c, now_stamp)
        if vevent:
            event_lines.extend(vevent)

    footer = ["END:VCALENDAR"]

    all_lines = header + vtimezone + event_lines + footer
    return "\r\n".join(all_lines) + "\r\n"
