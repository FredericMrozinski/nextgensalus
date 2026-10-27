//! HTTP access of plugin frontends to their plugin's settings (`/plugin-config/{fe_pid}`).
//!
//! `GET` returns the values in effect as a JSON object (stored values, else the defaults of the plugin's
//! configurable). `PUT` takes a JSON object of changes: a value sets the setting, `null` resets it to its
//! default. The reply is the values in effect afterwards. A frontend only ever reaches the settings of its own
//! plugin, and only for its owner's session.

use crate::config_loader::{self, ConfigError};
use crate::models::Plugin;
use crate::{plugin_library, plugin_process_manager, session_manager};
use axum::body::Bytes;
use axum::extract::Path;
use axum::http::{header, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use dioxus::logger::tracing::warn;
use serde_json::{Map, Value};

/// The plugin a frontend process belongs to, if this session may use it.
pub fn plugin_of_frontend(fe_pid: u32, jar: &CookieJar) -> Result<Plugin, StatusCode> {
    let user_id = session_manager::user_id_from_jar(jar).ok_or(StatusCode::UNAUTHORIZED)?;
    let owner = plugin_process_manager::get_frontend_owner(fe_pid).ok_or(StatusCode::NOT_FOUND)?;
    if owner.user_id != user_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let plugin_id = plugin_process_manager::get_plugin_id_for_fe_pid(fe_pid).ok_or(StatusCode::NOT_FOUND)?;
    plugin_library::get_plugin_from_id(&plugin_id).ok_or(StatusCode::NOT_FOUND)
}

pub fn json_response(status: StatusCode, value: &Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-store")],
        value.to_string(),
    )
        .into_response()
}

pub fn error_response(error: ConfigError) -> Response {
    match error {
        ConfigError::Invalid(message) => (StatusCode::BAD_REQUEST, message).into_response(),
        ConfigError::Corrupt(_) => (StatusCode::CONFLICT, error.to_string()).into_response(),
        ConfigError::Io(e) => {
            warn!("Config access failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// The JSON object in a request body.
pub fn parse_updates(body: &[u8]) -> Result<Map<String, Value>, Response> {
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err((StatusCode::BAD_REQUEST, "the body must be a JSON object of settings").into_response()),
    }
}

pub async fn handle(Path(fe_pid): Path<u32>, jar: CookieJar, method: Method, body: Bytes) -> Response {
    let plugin = match plugin_of_frontend(fe_pid, &jar) {
        Ok(plugin) => plugin,
        Err(status) => return status.into_response(),
    };
    let updates = match method {
        Method::GET => None,
        Method::PUT => match parse_updates(&body) {
            Ok(updates) => Some(updates),
            Err(response) => return response,
        },
        _ => return StatusCode::METHOD_NOT_ALLOWED.into_response(),
    };

    // File access blocks; keep it off the async workers.
    let result = tokio::task::spawn_blocking(move || match updates {
        None => config_loader::plugin_values(&plugin),
        Some(updates) => config_loader::update_plugin_values(&plugin, &updates),
    })
    .await;
    match result {
        Ok(Ok(values)) => json_response(StatusCode::OK, &Value::Object(values)),
        Ok(Err(error)) => error_response(error),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
