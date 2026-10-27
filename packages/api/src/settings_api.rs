//! The framework API of the settings plugin (`/settings-api/{fe_pid}/…`).
//!
//! `org.salus.settings` shows and changes the settings of Salus and of every plugin. It does not collect them
//! itself: it asks the framework, through these routes. They are not part of the plugin SDK. Only frontends of
//! the plugin named in `SETTINGS_PLUGIN` may use them (until the settings become a system plugin of Salus).
//!
//! ```text
//! GET /configurables              Salus and every plugin with its settings (the configurable)
//! GET /config/salus               {"values": …, "stored": […]} of salus.toml
//! PUT /config/salus               changes (a JSON object; null resets a setting), replies like GET
//! GET /config/plugin/{identifier} the same for one plugin
//! PUT /config/plugin/{identifier}
//! ```
//!
//! `values` are the values in effect, `stored` lists the settings that are set explicitly (the others show their default).

use crate::config_loader;
use crate::configurable::Configurable;
use crate::plugin_config_api::{error_response, json_response, parse_updates, plugin_of_frontend};
use crate::plugin_library;
use axum::body::Bytes;
use axum::extract::Path;
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

/// Identifier (folder name) of the only plugin that may use this API.
pub const SETTINGS_PLUGIN: &str = "org.salus.settings";

enum Target {
    Salus,
    Plugin(String),
}

fn target_of(path: &str) -> Option<Target> {
    match path.split('/').collect::<Vec<_>>().as_slice() {
        ["config", "salus"] => Some(Target::Salus),
        ["config", "plugin", identifier] if !identifier.is_empty() => Some(Target::Plugin(identifier.to_string())),
        _ => None,
    }
}

pub async fn handle(Path((fe_pid, rest)): Path<(u32, String)>, jar: CookieJar, method: Method, body: Bytes) -> Response {
    match plugin_of_frontend(fe_pid, &jar) {
        Ok(plugin) if plugin.identifier() == SETTINGS_PLUGIN => {}
        Ok(_) => return StatusCode::FORBIDDEN.into_response(),
        Err(status) => return status.into_response(),
    }
    let rest = rest.trim_end_matches('/').to_string();

    if rest == "configurables" {
        return match method {
            Method::GET => json_response(StatusCode::OK, &configurables()),
            _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
        };
    }
    let Some(target) = target_of(&rest) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let updates = match method {
        Method::GET => None,
        Method::PUT => match parse_updates(&body) {
            Ok(updates) => Some(updates),
            Err(response) => return response,
        },
        _ => return StatusCode::METHOD_NOT_ALLOWED.into_response(),
    };

    let changes_salus = matches!(target, Target::Salus) && updates.is_some();

    // File access blocks; keep it off the async workers.
    match tokio::task::spawn_blocking(move || config_of(target, updates)).await {
        Ok(Ok(Some(config))) => {
            if changes_salus {
                // The theme applies at once, in every open page.
                crate::browser_connections::broadcast_theme(&config_loader::theme());
            }
            json_response(StatusCode::OK, &config)
        }
        Ok(Ok(None)) => StatusCode::NOT_FOUND.into_response(),
        Ok(Err(error)) => error_response(error),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn configurables() -> Value {
    let mut plugins: Vec<(String, String, Configurable)> = plugin_library::get_plugins()
        .into_iter()
        .map(|(_, plugin)| {
            (plugin.identifier(), plugin.manifest.description.name.clone(), plugin.configurable.clone().unwrap_or_default())
        })
        .collect();
    plugins.sort_by_key(|(_, name, _)| name.to_lowercase());

    let mut all = vec![json!({
        "id": "salus", "kind": "salus", "name": "Salus",
        "settings": config_loader::salus_configurable().to_json(),
    })];
    all.extend(plugins.into_iter().map(|(identifier, name, configurable)| {
        json!({ "id": identifier, "kind": "plugin", "name": name, "settings": configurable.to_json() })
    }));
    Value::Array(all)
}

/// Reads or (with `updates`) changes the config of a target. `None` means there is no such plugin.
fn config_of(target: Target, updates: Option<Map<String, Value>>) -> Result<Option<Value>, config_loader::ConfigError> {
    let (path, configurable): (PathBuf, Configurable) = match target {
        Target::Salus => (config_loader::framework_config_path(), config_loader::salus_configurable().clone()),
        Target::Plugin(identifier) => {
            let Some((_, plugin)) = plugin_library::get_plugin_by_identifier(&identifier) else {
                return Ok(None);
            };
            (config_loader::plugin_config_path(&identifier)?, plugin.configurable.unwrap_or_default())
        }
    };
    let values = match &updates {
        None => config_loader::load_effective(&path, &configurable),
        Some(updates) => config_loader::update(&path, &configurable, updates)?,
    };
    Ok(Some(json!({ "values": values, "stored": config_loader::stored_keys(&path) })))
}
