use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use chrono_tz::Europe::Paris;
use rand::RngCore;
use regex::Regex;
use reqwest::header::{ACCEPT, CONTENT_TYPE, USER_AGENT};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use crate::db::models::Course;

pub const CLIENT_ID: &str = "np-front";
pub const REDIRECT_URI: &str = "https://auriga.epita.fr/";
pub const AUTH_BASE_URL: &str =
    "https://ionisepita-auth.np-auriga.nfrance.net/auth/realms/npionisepita/protocol/openid-connect/auth";
pub const TOKEN_URL: &str =
    "https://ionisepita-auth.np-auriga.nfrance.net/auth/realms/npionisepita/protocol/openid-connect/token";
pub const API_BASE_URL: &str = "https://auriga.epita.fr/api";

pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148";

pub fn generate_pkce_pair() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    let challenge = URL_SAFE_NO_PAD.encode(hash);

    (verifier, challenge)
}

pub fn get_default_school_year_range() -> (String, String) {
    let now: DateTime<chrono_tz::Tz> = Utc::now().with_timezone(&Paris);
    let (start_year, end_year) = if now.format("%m").to_string().parse::<u32>().unwrap_or(9) >= 8 {
        let y = now.format("%Y").to_string().parse::<i32>().unwrap_or(2026);
        (y, y + 1)
    } else {
        let y = now.format("%Y").to_string().parse::<i32>().unwrap_or(2026);
        (y - 1, y)
    };
    (format!("{}-09-01", start_year), format!("{}-08-31", end_year))
}

pub struct AurigaClient {
    pub username: String,
    pub password: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    client: reqwest::Client,
}

impl AurigaClient {
    pub fn new(username: &str, password: &str) -> Self {
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();

        Self {
            username: username.to_string(),
            password: password.to_string(),
            access_token: None,
            refresh_token: None,
            client,
        }
    }

    pub fn with_access_token(token: &str) -> Result<Self, String> {
        let mut clean = token.trim().to_string();
        if clean.to_lowercase().starts_with("bearer ") {
            clean = clean[7..].trim().to_string();
        }
        clean = clean.trim_matches('"').trim_matches('\'').to_string();

        if clean.contains('\u{2026}') || clean.contains("...") {
            return Err("Le jeton collé est incomplet : il a été tronqué par votre navigateur (présence de '…'). Faites Clic droit sur le header Authorization -> 'Copier la valeur' dans DevTools.".to_string());
        }

        let mut extracted_username = String::new();
        // Check expiration and username
        if let Ok(parts) = clean.split('.').collect::<Vec<&str>>().try_into() {
            let parts: [&str; 3] = parts;
            let mut payload_b64 = parts[1].to_string();
            while payload_b64.len() % 4 != 0 {
                payload_b64.push('=');
            }
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&payload_b64).or_else(|_| URL_SAFE_NO_PAD.decode(&parts[1])) {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&decoded) {
                    if let Some(exp) = v.get("exp").and_then(|e| e.as_i64()) {
                        let now = Utc::now().timestamp();
                        if now > exp {
                            return Err("Ce jeton d'accès Auriga a expiré (durée de validité de 5 minutes dépassée). Veuillez rafraîchir Auriga (F5) et copier le nouveau jeton, ou utilisez vos identifiants.".to_string());
                        }
                    }
                    if let Some(u) = v.get("preferred_username").and_then(|u| u.as_str()) {
                        extracted_username = u.to_string();
                    } else if let Some(u) = v.get("sub").and_then(|u| u.as_str()) {
                        extracted_username = u.to_string();
                    }
                }
            }
        }

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap_or_default();

        Ok(Self {
            username: extracted_username,
            password: String::new(),
            access_token: Some(clean),
            refresh_token: None,
            client,
        })
    }

    pub async fn login(&mut self) -> Result<(), String> {
        let (code_verifier, code_challenge) = generate_pkce_pair();
        let mut state_bytes = [0u8; 16];
        let mut nonce_bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut state_bytes);
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let state = URL_SAFE_NO_PAD.encode(state_bytes);
        let nonce = URL_SAFE_NO_PAD.encode(nonce_bytes);

        let auth_url = reqwest::Url::parse_with_params(
            AUTH_BASE_URL,
            &[
                ("client_id", CLIENT_ID),
                ("redirect_uri", REDIRECT_URI),
                ("state", &state),
                ("response_mode", "query"),
                ("response_type", "code"),
                ("scope", "openid"),
                ("nonce", &nonce),
                ("prompt", "login"),
                ("code_challenge", &code_challenge),
                ("code_challenge_method", "S256"),
            ],
        )
        .map_err(|e| e.to_string())?;

        let res = self
            .client
            .get(auth_url)
            .header(USER_AGENT, DEFAULT_USER_AGENT)
            .send()
            .await
            .map_err(|e| format!("Erreur connexion Keycloak: {}", e))?;

        let html = res.text().await.map_err(|e| e.to_string())?;

        let form_re = Regex::new(r#"(?i)<form[^>]*id="kc-form-login"[^>]*action="([^"]+)""#)
            .or_else(|_| Regex::new(r#"(?i)<form[^>]*action="([^"]+)"[^>]*id="kc-form-login""#))
            .map_err(|e| e.to_string())?;

        let action_cap = form_re
            .captures(&html)
            .ok_or_else(|| "Formulaire de connexion Auriga introuvable.".to_string())?;

        let raw_action = action_cap[1].replace("&amp;", "&");
        let action_url = reqwest::Url::parse(AUTH_BASE_URL)
            .and_then(|base| base.join(&raw_action))
            .map_err(|e| e.to_string())?;

        let login_params = [
            ("username", self.username.as_str()),
            ("password", self.password.as_str()),
            ("credentialId", ""),
        ];

        let post_res = self
            .client
            .post(action_url)
            .header(USER_AGENT, DEFAULT_USER_AGENT)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .form(&login_params)
            .send()
            .await
            .map_err(|e| format!("Erreur soumission identifiants: {}", e))?;

        let status = post_res.status().as_u16();
        let redirect_target = if post_res.status().is_redirection() {
            post_res
                .headers()
                .get("location")
                .and_then(|h| h.to_str().ok())
                .map(|s| s.to_string())
        } else if status == 200 {
            let body = post_res.text().await.unwrap_or_default();
            let err_re = Regex::new(r#"(?is)<span[^>]*class="[^"]*kc-feedback-text[^"]*"[^>]*>(.*?)</span>"#).unwrap();
            let msg = err_re.captures(&body).map(|c| c[1].trim().to_string()).unwrap_or_else(|| "Identifiants invalides".to_string());
            let clean_msg = Regex::new(r"<[^>]+>").unwrap().replace_all(&msg, "").to_string();
            return Err(format!("Échec de connexion Auriga : {}", clean_msg));
        } else {
            return Err(format!("Réponse inattendue lors de la connexion (HTTP {})", status));
        };

        let redir = redirect_target.ok_or_else(|| "Aucune redirection reçue après authentification.".to_string())?;
        let redir_url = reqwest::Url::parse(&redir).map_err(|e| e.to_string())?;

        let code = redir_url
            .query_pairs()
            .find(|(k, _)| k == "code")
            .map(|(_, v)| v.into_owned())
            .ok_or_else(|| "Code d'autorisation introuvable dans la redirection.".to_string())?;

        let token_params = [
            ("grant_type", "authorization_code"),
            ("client_id", CLIENT_ID),
            ("redirect_uri", REDIRECT_URI),
            ("code", code.as_str()),
            ("code_verifier", code_verifier.as_str()),
        ];

        let token_res = self
            .client
            .post(TOKEN_URL)
            .header(USER_AGENT, DEFAULT_USER_AGENT)
            .form(&token_params)
            .send()
            .await
            .map_err(|e| format!("Erreur échange de token: {}", e))?;

        if !token_res.status().is_success() {
            let err_body = token_res.text().await.unwrap_or_default();
            return Err(format!("Échec de récupération du token: {}", err_body));
        }

        let token_json: serde_json::Value = token_res.json().await.map_err(|e| e.to_string())?;

        self.access_token = token_json
            .get("access_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        self.refresh_token = token_json
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if self.access_token.is_none() {
            return Err("Jeton d'accès manquant dans la réponse Keycloak.".to_string());
        }

        Ok(())
    }

    pub async fn get_planning(&self, start_date: &str, end_date: &str) -> Result<Vec<serde_json::Value>, String> {
        let token = self.access_token.as_ref().ok_or_else(|| "Non authentifié.".to_string())?;

        let url = format!(
            "{}/plannings/me?days=0&days=1&days=2&days=3&days=4&days=5&days=6&days=7&startDate={}&endDate={}",
            API_BASE_URL, start_date, end_date
        );

        let res = self
            .client
            .get(&url)
            .header(USER_AGENT, DEFAULT_USER_AGENT)
            .header("Authorization", format!("Bearer {}", token))
            .header(ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| format!("Erreur réseau Auriga API: {}", e))?;

        if res.status().as_u16() == 401 {
            return Err("Accès refusé (401) par Auriga : Votre jeton est expiré ou invalide. Les jetons Auriga ont une validité de 5 minutes. Rafraîchissez Auriga (F5) et recopiez le jeton, ou utilisez l'onglet Identifiant & Mot de passe.".to_string());
        }

        if !res.status().is_success() {
            let err_txt = res.text().await.unwrap_or_default();
            let clean = Regex::new(r"<[^>]+>").unwrap().replace_all(&err_txt, " ").trim().to_string();
            return Err(format!("Erreur API Auriga: {}", &clean[..clean.len().min(250)]));
        }

        let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;

        if let Some(arr) = data.as_array() {
            Ok(arr.clone())
        } else if let Some(interventions) = data.get("interventions").and_then(|v| v.as_array()) {
            Ok(interventions.clone())
        } else {
            Ok(vec![])
        }
    }
}

pub fn normalize_course(item: &serde_json::Value) -> Course {
    let id = item.get("id").and_then(|v| v.as_i64()).map(|n| n.to_string()).unwrap_or_else(|| "0".to_string());
    let start = item.get("startDateTime").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let end = item.get("endDateTime").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let mut course_code = String::new();
    let mut course_name = String::new();

    if let Some(units) = item.get("interventionPedagogicalUnits").and_then(|v| v.as_array()) {
        if let Some(first_unit) = units.first().and_then(|u| u.get("pedagogicalUnit")) {
            course_code = first_unit.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if let Some(caption) = first_unit.get("caption") {
                course_name = caption.get("fr").or_else(|| caption.get("en")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
        }
    }

    if course_name.is_empty() {
        course_name = item.get("name").and_then(|v| v.get("name")).and_then(|v| v.as_str()).unwrap_or("Cours").to_string();
    }

    let act = item.get("activityType");
    let act_code = act.and_then(|v| v.get("code")).and_then(|v| v.as_str()).unwrap_or("AUTRE").to_string();
    let act_name = act.and_then(|v| v.get("caption")).and_then(|v| v.get("fr")).and_then(|v| v.as_str()).unwrap_or(&act_code).to_string();
    let is_exam = act.and_then(|v| v.get("isExam")).and_then(|v| v.as_bool()).unwrap_or(false);

    let status_code = item.get("interventionStatus").and_then(|v| v.get("code")).and_then(|v| v.as_str()).unwrap_or("PLANIFIE");
    let is_cancelled = status_code == "ANNULE" || status_code == "CANCELLED";

    let mut teachers = Vec::new();
    if Some(true) == item.get("interventionInstructors").map(|v| v.is_array()) {
        if let Some(arr) = item.get("interventionInstructors").and_then(|v| v.as_array()) {
            for inst in arr {
                if let Some(person) = inst.get("person") {
                    let first = person.get("currentFirstName").and_then(|v| v.as_str()).unwrap_or("").trim();
                    let last = person.get("currentLastName").and_then(|v| v.as_str()).unwrap_or("").trim();
                    let full = format!("{} {}", first, last).trim().to_string();
                    if !full.is_empty() {
                        teachers.push(full);
                    }
                }
            }
        }
    }

    let mut rooms = Vec::new();
    if Some(true) == item.get("interventionResources").map(|v| v.is_array()) {
        if let Some(arr) = item.get("interventionResources").and_then(|v| v.as_array()) {
            for res in arr {
                if let Some(r_name) = res.get("resource").and_then(|r| r.get("name")).and_then(|v| v.as_str()) {
                    let clean = r_name.trim();
                    if !clean.is_empty() {
                        rooms.push(clean.to_string());
                    }
                }
            }
        }
    }

    let mut groups = Vec::new();
    if Some(true) == item.get("interventionPromotions").map(|v| v.is_array()) {
        if let Some(arr) = item.get("interventionPromotions").and_then(|v| v.as_array()) {
            for promo in arr {
                if let Some(p_name) = promo.get("promotion").and_then(|p| p.get("name")).and_then(|v| v.as_str()) {
                    let clean = p_name.trim();
                    if !clean.is_empty() {
                        groups.push(clean.to_string());
                    }
                }
            }
        }
    }

    let duration = if !start.is_empty() && !end.is_empty() {
        if let (Ok(dt_start), Ok(dt_end)) = (DateTime::parse_from_rfc3339(&start), DateTime::parse_from_rfc3339(&end)) {
            (dt_end.timestamp() - dt_start.timestamp()) / 60
        } else {
            90
        }
    } else {
        90
    };

    let summary = format!("[{}] {}", act_code, course_name);
    let searchable_text = format!(
        "{} {} {} {} {} {}",
        course_name,
        course_code,
        act_name,
        teachers.join(" "),
        rooms.join(" "),
        groups.join(" ")
    );

    Course {
        id,
        start,
        end,
        duration,
        summary,
        course_name,
        course_code,
        activity_type: act_code,
        activity_name: act_name,
        is_exam,
        is_cancelled,
        status: status_code.to_string(),
        teachers,
        rooms,
        groups,
        description: String::new(),
        searchable_text,
    }
}

pub fn escape_ics_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\\n")
}

pub fn fold_line(line: &str, max_octets: usize) -> String {
    let bytes = line.as_bytes();
    if bytes.len() <= max_octets {
        return line.to_string();
    }

    let mut parts = Vec::new();
    let mut remaining = line;
    let mut limit = max_octets;

    while remaining.as_bytes().len() > limit {
        let mut cut = limit;
        while cut > 0 && !remaining.is_char_boundary(cut) {
            cut -= 1;
        }
        parts.push(&remaining[..cut]);
        remaining = &remaining[cut..];
        limit = 74;
    }

    if !remaining.is_empty() {
        parts.push(remaining);
    }

    parts.join("\r\n ")
}

pub fn format_ics_datetime(iso_str: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso_str) {
        let in_paris = dt.with_timezone(&Paris);
        in_paris.format("%Y%m%dT%H%M%S").to_string()
    } else {
        iso_str.replace(['-', ':', 'Z'], "")
    }
}

pub fn generate_ics_from_courses(courses: &[Course], calendar_name: &str) -> String {
    let now_utc = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut lines = Vec::new();

    lines.push("BEGIN:VCALENDAR".to_string());
    lines.push("VERSION:2.0".to_string());
    lines.push("PRODID:-//EpiKairos//EPITA Calendar Rust//FR".to_string());
    lines.push("CALSCALE:GREGORIAN".to_string());
    lines.push("METHOD:PUBLISH".to_string());
    let clean_cal_name = calendar_name.replace(['\r', '\n'], " ");
    lines.push(fold_line(&format!("X-WR-CALNAME:{}", escape_ics_text(&clean_cal_name)), 75));
    lines.push("X-WR-TIMEZONE:Europe/Paris".to_string());

    for c in courses {
        let clean_id: String = c
            .id
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
            .collect();
        lines.push("BEGIN:VEVENT".to_string());
        lines.push(format!("UID:auriga-{}@epita.fr", clean_id));
        lines.push(format!("DTSTAMP:{}", now_utc));
        lines.push(format!("DTSTART;TZID=Europe/Paris:{}", format_ics_datetime(&c.start)));
        lines.push(format!("DTEND;TZID=Europe/Paris:{}", format_ics_datetime(&c.end)));
        lines.push(fold_line(&format!("SUMMARY:{}", escape_ics_text(&c.summary)), 75));

        if !c.rooms.is_empty() {
            lines.push(fold_line(&format!("LOCATION:{}", escape_ics_text(&c.rooms.join(", "))), 75));
        }

        let mut desc_parts = Vec::new();
        if !c.course_name.is_empty() {
            desc_parts.push(format!("Matière: {}", c.course_name));
        }
        if !c.teachers.is_empty() {
            desc_parts.push(format!("Enseignant: {}", c.teachers.join(", ")));
        }
        if !c.groups.is_empty() {
            desc_parts.push(format!("Groupe: {}", c.groups.join(", ")));
        }
        if c.is_cancelled {
            desc_parts.push("STATUT: COURS ANNULE".to_string());
        }
        let desc = desc_parts.join("\n");
        lines.push(fold_line(&format!("DESCRIPTION:{}", escape_ics_text(&desc)), 75));

        if c.is_cancelled {
            lines.push("STATUS:CANCELLED".to_string());
        } else {
            lines.push("STATUS:CONFIRMED".to_string());
        }

        lines.push("END:VEVENT".to_string());
    }

    lines.push("END:VCALENDAR".to_string());
    lines.push(String::new());
    lines.join("\r\n")
}

pub fn filter_courses(
    courses: &[Course],
    include_regex: Option<&str>,
    exclude_regex: Option<&str>,
    allowed_types: Option<&[String]>,
    allowed_courses: Option<&[String]>,
    hide_cancelled: bool,
) -> Result<Vec<Course>, String> {
    let inc_re = if let Some(r) = include_regex {
        if !r.trim().is_empty() {
            Some(Regex::new(r.trim()).map_err(|e| format!("Regex d'inclusion invalide: {}", e))?)
        } else {
            None
        }
    } else {
        None
    };

    let exc_re = if let Some(r) = exclude_regex {
        if !r.trim().is_empty() {
            Some(Regex::new(r.trim()).map_err(|e| format!("Regex d'exclusion invalide: {}", e))?)
        } else {
            None
        }
    } else {
        None
    };

    let types_set: Option<HashSet<String>> = allowed_types.map(|list| list.iter().map(|s| s.to_uppercase()).collect());
    let courses_set: Option<HashSet<String>> = allowed_courses.map(|list| list.iter().map(|s| s.trim().to_lowercase()).collect());

    let mut filtered = Vec::new();
    for c in courses {
        if hide_cancelled && c.is_cancelled {
            continue;
        }

        if let Some(ref t_set) = types_set {
            if !t_set.contains(&c.activity_type.to_uppercase()) {
                continue;
            }
        }

        if let Some(ref c_set) = courses_set {
            if !c_set.contains(&c.course_name.trim().to_lowercase()) {
                continue;
            }
        }

        if let Some(ref inc) = inc_re {
            if !inc.is_match(&c.searchable_text) {
                continue;
            }
        }

        if let Some(ref exc) = exc_re {
            if exc.is_match(&c.searchable_text) {
                continue;
            }
        }

        filtered.push(c.clone());
    }

    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ics_crlf_sanitization() {
        let course = Course {
            id: "123\r\nX-INJECTED:PWNED\r\n456".to_string(),
            start: "2026-10-13T09:00:00".to_string(),
            end: "2026-10-13T11:00:00".to_string(),
            duration: 120,
            summary: "Test Course".to_string(),
            rooms: vec!["Salle 1".to_string()],
            teachers: vec!["Prof".to_string()],
            groups: vec!["G1".to_string()],
            course_name: "Rust 101".to_string(),
            course_code: "CM".to_string(),
            activity_type: "CM".to_string(),
            activity_name: "Cours".to_string(),
            is_exam: false,
            is_cancelled: false,
            status: "PLANIFIE".to_string(),
            description: String::new(),
            searchable_text: String::new(),
        };

        let cal_name = "Calendar\r\nEVIL-HEADER:VALUE\r\nName";
        let ics = generate_ics_from_courses(&[course], cal_name);

        assert!(!ics.contains("X-INJECTED:PWNED"));
        assert!(!ics.contains("EVIL-HEADER:VALUE\r\n"));
        assert!(ics.contains("UID:auriga-123X-INJECTEDPWNED456@epita.fr"));
    }
}
