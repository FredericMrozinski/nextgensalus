use crate::models::{FileViewer, Plugin, PluginDescription, SpawnedComponent};
use axum_extra::extract::CookieJar;
#[cfg(feature = "server")]
use dioxus::fullstack::FullstackContext;
#[cfg(feature = "server")]
use crate::models::User;
#[cfg(feature = "server")]
use crate::plugin_library;
#[cfg(feature = "server")]
use crate::plugin_process_manager::{self, ComponentTarget};
#[cfg(feature = "server")]
use crate::session_manager;
use dioxus::prelude::*;

// TODO all of these functions should take a session token. Remember, that they
// are exposed to anyone.

/// The user of the request's session cookie.
#[cfg(feature = "server")]
async fn session_user() -> Result<User, ServerFnError> {
    let jar: CookieJar = FullstackContext::extract().await?;
    match session_manager::user_id_from_jar(&jar) {
        Some(user_id) => Ok(User { user_id }),
        None => Err(ServerFnError::new("invalid or missing session")),
    }
}

#[server]
pub async fn get_plugin_from_id(plugin_id: u32) -> Result<Option<Plugin>, ServerFnError> {
    session_user().await?;
    Ok(plugin_library::get_plugin_from_id(&plugin_id))
}

/// The plugins the plugin picker lists: those that name an entry component.
#[server]
// TODO, at some point (as with everything) a session token needs to be passed here so that
// only the user allowed plugins will be returned
pub async fn get_available_plugins() -> Result<Vec<(u32, PluginDescription)>, ServerFnError> {
    let mut res: Vec<(u32, PluginDescription)> = plugin_library::get_plugins().iter()
        .filter(|(_, plugin)| plugin.entry_component().is_some())
        .map(|(id, plugin)| (*id, plugin.manifest.description.clone()))
        .collect();
    res.sort_by_key(|(id, _)| *id);

    Ok(res)
}

/// Opens a plugin from the plugin picker: spawns a frontend process for its entry component.
/// `page_id` identifies the browser page; its frontends are closed when the page's connection ends.
#[server]
pub async fn open_plugin(plugin_id: u32, page_id: Option<String>) -> Result<SpawnedComponent, ServerFnError> {
    let user = session_user().await?;
    plugin_process_manager::open_plugin_entry(plugin_id, &user, page_id).map_err(ServerFnError::new)
}

/// Opens a component on behalf of a running frontend, which becomes its parent.
/// `plugin = None` opens a component of the requester's own plugin (then `component` is required);
/// otherwise `plugin` must be listed under `[dependencies]` of the requester's plugin, and
/// `component = None` means that plugin's entry component.
/// With `reuse`, an existing process of that component is returned instead of a new one.
/// `params` is JSON text handed to the component.
#[server]
pub async fn open_frontend_component(
    requester_frontend_process_id: u32,
    plugin: Option<String>,
    component: Option<String>,
    params: Option<String>,
    reuse: bool,
    page_id: Option<String>,
) -> Result<SpawnedComponent, ServerFnError> {
    let user = session_user().await?;
    let target = match (plugin, component) {
        (None, Some(component)) => ComponentTarget::Own { component },
        (None, None) => return Err(ServerFnError::new("a component name is required")),
        (Some(plugin), component) => ComponentTarget::Dependency { plugin, component },
    };
    plugin_process_manager::open_component(requester_frontend_process_id, target, params, reuse, &user, page_id)
        .map_err(ServerFnError::new)
}

/// Components that can display files of the given type (extension, any case, no dot).
#[server]
pub async fn get_file_viewers(extension: String) -> Result<Vec<FileViewer>, ServerFnError> {
    session_user().await?;
    Ok(plugin_library::get_file_viewers(&extension))
}

/// Opens `file` in the given viewer component; the viewer receives it as `params.file`.
#[server]
pub async fn open_file_viewer(plugin_id: u32, component: String, file: String, page_id: Option<String>) -> Result<SpawnedComponent, ServerFnError> {
    let user = session_user().await?;
    plugin_process_manager::open_file_viewer(plugin_id, &component, &file, &user, page_id).map_err(ServerFnError::new)
}

/// Closes a frontend (its tab was closed). The plugin's backend is told, and a `panel` backend ends with its last frontend.
#[server]
pub async fn close_frontend_process(frontend_process_id: u32) -> Result<(), ServerFnError> {
    let user = session_user().await?;
    plugin_process_manager::close_frontend_of_user(frontend_process_id, &user).await.map_err(ServerFnError::new)
}
