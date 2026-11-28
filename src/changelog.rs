//! "Mises à jour" page: the site's updates, from `src/changelog.json`
//! (newest first, French and English for every line).

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const CHANGELOG_JSON: &str = include_str!("changelog.json");

#[derive(Debug, Clone, Deserialize)]
pub struct Text {
    pub fr: String,
    pub en: String,
}

impl Text {
    fn pick(&self, lang: &str) -> String {
        if lang == "en" { self.en.clone() } else { self.fr.clone() }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    /// YYYY-MM-DD
    pub date: String,
    pub title: Text,
    #[serde(default)]
    pub new: Vec<Text>,
    #[serde(default)]
    pub fixes: Vec<Text>,
}

/// One entry in the page's language.
#[derive(Debug, Clone, Serialize)]
pub struct EntryView {
    pub date: String,
    pub date_label: String,
    pub title: String,
    pub new: Vec<String>,
    pub fixes: Vec<String>,
}

pub fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| serde_json::from_str(CHANGELOG_JSON).expect("src/changelog.json must be a list of entries"))
}

fn date_label(date: &str, lang: &str) -> String {
    const FR: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];
    const EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let Ok(d) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") else { return date.to_string() };
    use chrono::Datelike;
    let month = if lang == "en" { EN } else { FR }[d.month0() as usize];
    format!("{} {} {}", d.day(), month, d.year())
}

pub fn view(lang: &str) -> Vec<EntryView> {
    entries()
        .iter()
        .map(|e| EntryView {
            date: e.date.clone(),
            date_label: date_label(&e.date, lang),
            title: e.title.pick(lang),
            new: e.new.iter().map(|t| t.pick(lang)).collect(),
            fixes: e.fixes.iter().map(|t| t.pick(lang)).collect(),
        })
        .collect()
}
