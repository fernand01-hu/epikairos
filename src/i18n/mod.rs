//! French / English. French is the source language: the English dictionary maps
//! each French text to its translation, and a missing entry falls back to French.
//! The same dictionary is sent to the browser for texts built in JavaScript.

use std::collections::HashMap;
use std::sync::OnceLock;
use tower_cookies::Cookies;

const EN_JSON: &str = include_str!("en.json");

fn english() -> &'static HashMap<String, String> {
    static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
    EN.get_or_init(|| serde_json::from_str(EN_JSON).expect("src/i18n/en.json must be a JSON object"))
}

/// The English dictionary as JSON, safe to embed in a `<script type="application/json">`
/// for the JavaScript `t()` helper ("</" is escaped so no text can close the tag).
pub fn english_json() -> &'static str {
    static SAFE: OnceLock<String> = OnceLock::new();
    SAFE.get_or_init(|| EN_JSON.replace("</", "<\\/"))
}

/// "en" when the `lang` cookie asks for it, "fr" otherwise.
pub fn current_lang(cookies: &Cookies) -> &'static str {
    match cookies.get("lang").map(|c| c.value().to_string()).as_deref() {
        Some("en") => "en",
        _ => "fr",
    }
}

/// `text` (French) in the requested language.
pub fn tr(lang: &str, text: &str) -> String {
    if lang == "en" {
        if let Some(t) = english().get(text) {
            return t.clone();
        }
    }
    text.to_string()
}

/// Every French text the dictionary translates (used by the completeness test).
pub fn english_keys() -> Vec<&'static str> {
    english().keys().map(|k| k.as_str()).collect()
}
