use crate::models::{Plugin, User};
use axum_extra::extract::CookieJar;
use dioxus::fullstack::ServerFnError::ServerError;
#[cfg(feature = "server")]
use crate::plugin_library;
#[cfg(feature = "server")]
use crate::plugin_process_manager;
#[cfg(feature = "server")]
use crate::session_manager;
use dioxus::prelude::*;

// TODO all of these functions should take a session token. Remember, that they
// are exposed to anyone.
#[server]
pub async fn get_plugin_from_id(plugin_id: u32) -> Result<Option<Plugin>, ServerFnError> {
    let jar: CookieJar = FullstackContext::extract().await?;
    if !session_manager::session_valid(jar).await {
        return Err(ServerFnError::new("invalid or missing session"));
    }

    Ok(plugin_library::get_plugin_from_id(&plugin_id))
}

#[server]
pub async fn spawn_frontend_plugin_process(plugin_id: u32, user: User) -> Result<u32, ServerFnError> {
    let jar: CookieJar = FullstackContext::extract().await?;
    if !session_manager::session_valid(jar).await {
        return Err(ServerFnError::new("invalid or missing session"));
    }

    Ok(plugin_process_manager::spawn_frontend_plugin_process(plugin_id, &user))
}

