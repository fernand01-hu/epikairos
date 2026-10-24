use minijinja::value::{Object, Value};
use minijinja::Environment;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SessionObject {
    pub data: HashMap<String, Value>,
}

impl Object for SessionObject {
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

    // Custom url_for helper
    env.add_function(
        "url_for",
        |endpoint: String, kwargs: minijinja::value::Kwargs| -> Result<String, minijinja::Error> {
            if endpoint == "static" {
                let filename: Option<String> = kwargs.get("filename")?;
                return Ok(format!("/static/{}", filename.unwrap_or_default()));
            }

            match endpoint.as_str() {
                "dashboard" => Ok("/dashboard".to_string()),
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
                "custom_calendar" => Ok("/custom-calendar".to_string()),
                "rooms_page" => Ok("/rooms".to_string()),
                "old_zeus" => Ok("/old-zeus".to_string()),
                _ => Ok(format!("/{}", endpoint)),
            }
        },
    );

    env
}

pub fn create_session_value(logged_in: bool, username: Option<&str>, role: Option<&str>) -> Value {
    let mut map = HashMap::new();
    map.insert("logged_in".to_string(), Value::from(logged_in));
    if let Some(u) = username {
        map.insert("username".to_string(), Value::from(u));
    }
    if let Some(r) = role {
        map.insert("role".to_string(), Value::from(r));
    }
    Value::from_object(SessionObject { data: map })
}
