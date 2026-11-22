//! French / English: every text marked for translation has its English version,
//! and the language cookie switches the pages.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use std::collections::HashSet;

/// Texts marked in templates ({{ _("...") }}) and scripts (t("...")).
fn marked_texts() -> Vec<(String, String)> {
    let tpl = regex::Regex::new(r#"_\(\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)')\s*\)"#).unwrap();
    let js = regex::Regex::new(r#"\bt\(\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)')"#).unwrap();
    let mut out = Vec::new();
    for (dir, ext, re) in [("templates", "html", &tpl), ("static/js", "js", &js)] {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some(ext) {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            for cap in re.captures_iter(&src) {
                let text = cap.get(1).or(cap.get(2)).unwrap().as_str().replace("\\\"", "\"").replace("\\'", "'");
                out.push((path.display().to_string(), text));
            }
        }
    }
    out
}

#[test]
fn every_marked_text_has_an_english_translation() {
    let known: HashSet<&str> = epikairos::i18n::english_keys().into_iter().collect();
    let marked = marked_texts();
    assert!(marked.len() > 150, "found only {} marked texts", marked.len());
    let missing: Vec<_> = marked.iter().filter(|(_, t)| !known.contains(t.as_str())).collect();
    assert!(missing.is_empty(), "add these to src/i18n/en.json: {missing:#?}");
}

#[test]
fn placeholders_match_between_languages() {
    let re = regex::Regex::new(r"\{[a-z]+\}").unwrap();
    for fr in epikairos::i18n::english_keys() {
        let en = epikairos::i18n::tr("en", fr);
        let a: HashSet<_> = re.find_iter(fr).map(|m| m.as_str()).collect();
        let b: HashSet<_> = re.find_iter(&en).map(|m| m.as_str()).collect();
        assert_eq!(a, b, "{fr:?} -> {en:?}");
    }
}

#[test]
fn unknown_texts_and_french_stay_as_written() {
    assert_eq!(epikairos::i18n::tr("en", "Mon planning"), "My timetable");
    assert_eq!(epikairos::i18n::tr("fr", "Mon planning"), "Mon planning");
    assert_eq!(epikairos::i18n::tr("en", "Texte inconnu"), "Texte inconnu");
}

async fn get_with_lang(app: &TestApp, path: &str, lang: Option<&str>) -> String {
    let mut cookie = session_cookie("alice.martin");
    if let Some(l) = lang {
        cookie = format!("{cookie}; lang={l}");
    }
    let req = Request::builder().uri(path).header(header::HOST, "localhost:5000").header(header::COOKIE, cookie);
    let res = app.send(req.body(Body::empty()).unwrap()).await;
    assert_eq!(res.status, StatusCode::OK, "{path}");
    res.body
}

#[tokio::test]
async fn pages_follow_the_language_cookie() {
    let app = TestApp::new();
    for path in ["/personal", "/planning", "/rooms", "/login"] {
        let fr = get_with_lang(&app, path, None).await;
        assert!(fr.contains(r#"<html lang="fr""#), "{path}");
        assert!(fr.contains("Mon planning") && fr.contains(r#"data-lang="en""#) && fr.contains(">EN</button>"), "{path}");
        assert!(!fr.contains("i18nData"), "French pages carry no dictionary");

        let en = get_with_lang(&app, path, Some("en")).await;
        assert!(en.contains(r#"<html lang="en""#), "{path}");
        assert!(en.contains("My timetable") && en.contains(r#"data-lang="fr""#) && en.contains(">FR</button>"), "{path}");
        assert!(en.contains(r#"id="i18nData""#), "{path}: dictionary for scripts");
        let visible = &en[..en.find(r#"id="i18nData""#).unwrap()];
        assert!(!visible.contains("Se déconnecter") && !visible.contains("Salles libres"), "{path}");
    }
    // Anything else falls back to French
    assert!(get_with_lang(&app, "/rooms", Some("de")).await.contains(r#"<html lang="fr""#));
}

#[tokio::test]
async fn embedded_dictionary_cannot_close_its_script_tag() {
    let app = TestApp::new();
    let page = get_with_lang(&app, "/login", Some("en")).await;
    let start = page.find(r#"id="i18nData">"#).unwrap();
    let block = &page[start..start + page[start..].find("</script>").unwrap()];
    assert!(block.contains("<button") && !block.contains("</button"), "'</' is escaped inside the JSON");
    let json: serde_json::Value = serde_json::from_str(&block[r#"id="i18nData">"#.len()..]).unwrap();
    assert_eq!(json["Copier"], "Copy");
}
