//! Auriga client helpers that run without network access.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use epikairos::auriga::{
    decode_jwt_claims, get_default_school_year_range, is_valid_date_format, normalize_course, token_username, AurigaClient,
};
use serde_json::json;

fn jwt(claims: serde_json::Value) -> String {
    format!(
        "{}.{}.sig",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    )
}

#[test]
fn token_username_prefers_preferred_username() {
    let t = jwt(json!({ "preferred_username": "fernand.huttois", "sub": "1234-uuid" }));
    assert_eq!(token_username(&t).as_deref(), Some("fernand.huttois"));
    let only_sub = jwt(json!({ "sub": "1234-uuid" }));
    assert_eq!(token_username(&only_sub).as_deref(), Some("1234-uuid"));
    assert_eq!(token_username(&jwt(json!({ "preferred_username": "  " }))), None);
    assert_eq!(token_username("not-a-jwt"), None);
    assert_eq!(decode_jwt_claims("a.!!!.c"), None);
}

#[test]
fn pasted_tokens_are_cleaned_and_checked() {
    let exp = Utc::now().timestamp() + 300;
    let token = jwt(json!({ "preferred_username": "alice.martin", "exp": exp }));

    for pasted in [token.clone(), format!("Bearer {token}"), format!("\"{token}\""), format!("  bearer {token} ")] {
        let client = AurigaClient::with_access_token(&pasted).expect("valid token");
        assert_eq!(client.username, "alice.martin");
        assert_eq!(client.access_token.as_deref(), Some(token.as_str()));
    }

    let expired = jwt(json!({ "preferred_username": "alice.martin", "exp": Utc::now().timestamp() - 1 }));
    assert!(AurigaClient::with_access_token(&expired).err().unwrap().contains("expiré"));
    assert!(AurigaClient::with_access_token("eyJhbGc…").err().unwrap().contains("tronqué"));
    assert!(AurigaClient::with_access_token("eyJhbGc...").is_err());
}

#[test]
fn date_format_validation() {
    assert!(is_valid_date_format("2026-09-01"));
    for bad in ["2026-9-1", "2026-13-01", "01-09-2026", "2026-09-01&x=1", ""] {
        assert!(!is_valid_date_format(bad), "{bad}");
    }
    let (start, end) = get_default_school_year_range();
    assert!(is_valid_date_format(&start) && is_valid_date_format(&end));
    assert!(start < end);
}

#[tokio::test]
async fn planning_request_rejects_bad_dates_before_any_network_call() {
    let client = AurigaClient::new("user", "pass");
    let err = client.get_planning("2026-01-01&evil=1", "2026-02-01").await.unwrap_err();
    assert!(err.contains("Format de date invalide"));
}

#[test]
fn normalize_course_maps_auriga_fields() {
    let raw = json!({
        "id": 22555,
        "startDateTime": "2026-10-07T09:00:00Z",
        "endDateTime": "2026-10-07T10:00:00Z",
        "interventionPedagogicalUnits": [{ "pedagogicalUnit": { "code": "PSAC", "caption": { "fr": "Programmation système" } } }],
        "activityType": { "code": "CME", "caption": { "fr": "Cours magistral" }, "isExam": false },
        "interventionStatus": { "code": "ANNULE" },
        "interventionInstructors": [{ "person": { "currentFirstName": "Matthieu", "currentLastName": "RONDEAU" } }],
        "interventionResources": [{ "resource": { "code": "KB-301", "caption": { "fr": " Salle 301 " } } }],
        "interventionPopulations": [
            { "population": { "code": "S3-L3", "caption": { "fr": "2627 - S3 Lyon L3" } } },
            { "population": { "code": "S3-L2" } }
        ]
    });
    let c = normalize_course(&raw);
    assert_eq!(c.id, "22555");
    assert_eq!(c.course_name, "Programmation système");
    assert_eq!(c.course_code, "PSAC");
    assert_eq!(c.activity_type, "CME");
    assert!(c.is_cancelled);
    assert_eq!(c.teachers, ["Matthieu RONDEAU"]);
    assert_eq!(c.rooms, ["Salle 301"]);
    assert_eq!(c.groups, ["2627 - S3 Lyon L3", "S3-L2"], "caption, else code");
    assert_eq!(c.start, "2026-10-07T09:00:00Z");
    assert!(c.searchable_text.contains("Salle 301") && c.searchable_text.contains("S3 Lyon L3"));
}

#[test]
fn normalize_course_reads_older_room_and_group_shapes() {
    let raw = json!({
        "id": 1,
        "interventionResources": [{ "resource": { "name": "Amphi 310" } }, { "resource": {} }],
        "interventionPromotions": [{ "promotion": { "name": "SUP Lyon" } }]
    });
    let c = normalize_course(&raw);
    assert_eq!(c.rooms, ["Amphi 310"]);
    assert_eq!(c.groups, ["SUP Lyon"]);
}

#[test]
fn normalize_course_tolerates_missing_fields() {
    let c = normalize_course(&json!({}));
    assert_eq!(c.id, "0");
    assert_eq!(c.course_name, "Cours");
    assert_eq!(c.activity_type, "AUTRE");
    assert!(!c.is_cancelled);
    assert!(c.teachers.is_empty() && c.rooms.is_empty());
}
