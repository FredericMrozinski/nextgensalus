//! HTTP access of plugin frontends to their plugin's config directory (`/plugin-config/{fe_pid}/…`).
//!
//! `GET path` reads a file, `GET path?list` (or the bare root) lists a directory, `PUT path` writes a
//! file and `DELETE path` removes a file or directory. A frontend only ever reaches the directory of its
//! own plugin, and only for its owner's session.

use crate::config_loader::{self, ConfigError};
use crate::{plugin_library, plugin_process_manager, session_manager};
use axum::body::Bytes;
use axum::extract::{Path, RawQuery};
use axum::http::{header, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use dioxus::logger::tracing::warn;
use std::path::PathBuf;

pub async fn handle_root(
    Path(fe_pid): Path<u32>,
    jar: CookieJar,
    method: Method,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    handle_request(fe_pid, String::new(), jar, method, query, body).await
}

pub async fn handle(
    Path((fe_pid, rest)): Path<(u32, String)>,
    jar: CookieJar,
    method: Method,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    handle_request(fe_pid, rest, jar, method, query, body).await
}

/// The config directory of the plugin a frontend process belongs to, if this session may use it.
fn directory_for(fe_pid: u32, jar: &CookieJar) -> Result<PathBuf, StatusCode> {
    let user_id = session_manager::user_id_from_jar(jar).ok_or(StatusCode::UNAUTHORIZED)?;
    let owner = plugin_process_manager::get_frontend_owner(fe_pid).ok_or(StatusCode::NOT_FOUND)?;
    if owner.user_id != user_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let plugin_id = plugin_process_manager::get_plugin_id_for_fe_pid(fe_pid).ok_or(StatusCode::NOT_FOUND)?;
    let plugin = plugin_library::get_plugin_from_id(&plugin_id).ok_or(StatusCode::NOT_FOUND)?;
    config_loader::plugin_config_dir(&plugin.identifier()).map_err(|e| {
        warn!("Cannot open the config directory of plugin {plugin_id}: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

async fn handle_request(fe_pid: u32, rest: String, jar: CookieJar, method: Method, query: Option<String>, body: Bytes) -> Response {
    let dir = match directory_for(fe_pid, &jar) {
        Ok(dir) => dir,
        Err(status) => return status.into_response(),
    };
    let wants_list = rest.is_empty() || rest.ends_with('/') || query.as_deref().is_some_and(|q| q.split('&').any(|p| p == "list"));
    let relative = rest.trim_end_matches('/').to_string();
    let max_bytes = config_loader::framework_config().plugin_config_max_file_bytes;

    // File access blocks; keep it off the async workers.
    let result = tokio::task::spawn_blocking(move || -> Result<Response, ConfigError> {
        match method {
            Method::GET if wants_list => {
                let entries = config_loader::list(&dir, &relative)?;
                Ok(([(header::CONTENT_TYPE, "application/json")], serde_json::to_string(&entries).unwrap()).into_response())
            }
            Method::GET => {
                let data = config_loader::read(&dir, &relative)?;
                // Always an opaque download: a plugin's data must never run as a page on the Salus origin.
                Ok((
                    [
                        (header::CONTENT_TYPE, "application/octet-stream"),
                        (header::CACHE_CONTROL, "no-store"),
                        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                    ],
                    data,
                )
                    .into_response())
            }
            Method::PUT => {
                config_loader::write(&dir, &relative, &body, max_bytes)?;
                Ok(StatusCode::NO_CONTENT.into_response())
            }
            Method::DELETE => {
                config_loader::delete(&dir, &relative)?;
                Ok(StatusCode::NO_CONTENT.into_response())
            }
            _ => Ok(StatusCode::METHOD_NOT_ALLOWED.into_response()),
        }
    })
    .await;

    match result {
        Ok(Ok(response)) => response,
        Ok(Err(ConfigError::NotFound)) => StatusCode::NOT_FOUND.into_response(),
        Ok(Err(e @ ConfigError::InvalidPath(_))) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        Ok(Err(e @ ConfigError::TooLarge(_))) => (StatusCode::PAYLOAD_TOO_LARGE, e.to_string()).into_response(),
        Ok(Err(ConfigError::Io(e))) => {
            warn!("Plugin config access failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
