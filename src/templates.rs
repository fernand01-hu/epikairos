use minijinja::value::{Object, Value};
use minijinja::Environment;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SessionObject {
    pub data: HashMap<String, Value>,
}

impl Object for SessionObject {
    // Lets Rust read fields (the `_` function needs `session.lang`)
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        key.as_str().and_then(|k| self.data.get(k).cloned())
    }

    fn call_method(
        self: &Arc<Self>,
        _state: &minijinja::State,
        name: &str,
        args: &[Value],
    ) -> Result<Value, minijinja::Error> {
        if name == "get" {
            let key = args.first().and_then(|v| v.as_str()).unwrap_or("");
            let default = args.get(1).cloned().unwrap_or(Value::UNDEFINED);
            Ok(self.data.get(key).cloned().unwrap_or(default))
        } else {
            Err(minijinja::Error::new(
                minijinja::ErrorKind::UnknownMethod,
                format!("method {} not found on session", name),
            ))
        }
    }
}

pub fn create_template_env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_loader(minijinja::path_loader("templates"));

    // Translation: {{ _("Texte en français") }} follows the page's language
    env.add_function("_", |state: &minijinja::State, text: String| -> String {
        let lang = state
            .lookup("session")
            .and_then(|s| s.get_attr("lang").ok())
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "fr".to_string());
        crate::i18n::tr(&lang, &text)
    });
    // Absolute site address for link previews (og:image must be a full URL)
    let site_url = std::env::var("SERVER_DOMAIN")
        .ok()
        .map(|d| d.trim().trim_end_matches('/').to_string())
        .filter(|d| !d.is_empty())
        .map(|d| if d.starts_with("http") { d } else { format!("https://{d}") })
        .unwrap_or_default();
    env.add_global("site_url", site_url);
    env.add_global("i18n_en", Value::from_safe_string(crate::i18n::english_json().to_string()));

    // Custom url_for helper
    env.add_function(
        "url_for",
        |endpoint: String, kwargs: minijinja::value::Kwargs| -> Result<String, minijinja::Error> {
            if endpoint == "static" {
                let filename: Option<String> = kwargs.get("filename")?;
                return Ok(format!("/static/{}", filename.unwrap_or_default()));
            }

            match endpoint.as_str() {
                "dashboard" => Ok("/personal".to_string()),
                "login_page" => {
                    let logged_out: Option<String> = kwargs.get("logged_out").ok().flatten();
                    if let Some(lo) = logged_out {
                        Ok(format!("/login?logged_out={}", lo))
                    } else {
                        Ok("/login".to_string())
                    }
                }
                "logout" => Ok("/logout".to_string()),
                "personal_calendar" => Ok("/personal".to_string()),
                "custom_calendar" => Ok("/planning".to_string()),
                "rooms_page" => Ok("/rooms".to_string()),
                "old_zeus" => Ok("/planning".to_string()),
                _ => Ok(format!("/{}", endpoint)),
            }
        },
    );

    env
}

pub fn create_session_value(logged_in: bool, username: Option<&str>, role: Option<&str>, lang: &str) -> Value {
    let mut map = HashMap::new();
    map.insert("lang".to_string(), Value::from(lang));
    map.insert("logged_in".to_string(), Value::from(logged_in));
    if let Some(u) = username {
        map.insert("username".to_string(), Value::from(u));
    }
    if let Some(r) = role {
        map.insert("role".to_string(), Value::from(r));
    }
    Value::from_object(SessionObject { data: map })
}
