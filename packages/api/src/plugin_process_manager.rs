use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU32, Ordering};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};
use crate::models::{FrontendComponent, Plugin, SpawnedComponent, User, BackendLifetime};
use crate::plugin_library;
use crate::plugin_backend_executor::{self, BackendPluginSocketHandle};
use std::process;
use std::sync::Arc;
use std::time::{Duration, Instant};
use dioxus::prelude::debug;
use tokio::task::JoinHandle;
use tokio::net::UnixListener;
use crate::plugin_message_router::{self, Stream};

pub struct PluginFrontendProcess {
    pub plugin_id: u32,
    /// Which of the plugin's frontend components this process runs.
    pub component_name: String,
    pub process_id: u32,
    pub backend_process_id: u32,
    pub owner: User,
    /// The frontend that asked for this one to be opened as its dependency.
    pub parent_process_id: Option<u32>,
    /// The browser page that opened it. When that page goes away, the process is closed.
    pub page_id: Option<String>,
}

pub struct PluginBackendProcess {
    pub plugin_id: u32,
    pub process_id: u32,
    pub backend_lifetime: BackendLifetime,
    pub owner: User,
    pub process: process::Child,
    pub socket_join_handle: JoinHandle<BackendPluginSocketHandle>,
}

fn get_next_frontend_pid() -> u32 {
    static NEXT_FREE_FE_PID: AtomicU32 = AtomicU32::new(30000);

    NEXT_FREE_FE_PID.fetch_add(1, Ordering::SeqCst)
}

fn get_next_backend_pid() -> u32 {
    static NEXT_FREE_BE_PID: AtomicU32 = AtomicU32::new(20000);

    NEXT_FREE_BE_PID.fetch_add(1, Ordering::SeqCst)
}

fn frontend_processes() -> &'static Mutex<HashMap<u32, PluginFrontendProcess>> {
    static FRONTEND_PROCESSES: OnceLock<Mutex<HashMap<u32, PluginFrontendProcess>>> = OnceLock::new();
    FRONTEND_PROCESSES.get_or_init(|| Mutex::new(HashMap::new()))
}

// TODO Okay now the UnixListener is hardcoded which we tried to avoid. fix that
fn backend_processes() -> &'static Mutex<HashMap<u32, PluginBackendProcess>> {
    static BACKEND_PROCESSES: OnceLock<Mutex<HashMap<u32, PluginBackendProcess>>> = OnceLock::new();
    BACKEND_PROCESSES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A running backend, as far as the choice of a backend for a new frontend is concerned.
struct BackendCandidate {
    id: u32,
    plugin_id: u32,
    owner_user_id: u32,
}

/// Which running backend a new frontend of `plugin_id` connects to; `None` means a new one is started.
///
/// - A frontend opened by a frontend **of the same plugin** (a component opening another component)
///   always shares its parent's backend: all components of a plugin instance talk to one backend.
/// - Otherwise the manifest's `lifetime` decides: `panel` always starts a new backend (and the backend ends
///   with its last frontend); `session` reuses the user's running backend of the plugin; `system` reuses
///   any user's running backend of the plugin.
fn choose_backend(
    plugin_id: u32,
    lifetime: &BackendLifetime,
    user_id: u32,
    parent_backend: Option<u32>,
    running: &[BackendCandidate],
) -> Option<u32> {
    if parent_backend.is_some() {
        return parent_backend;
    }
    let same_plugin = running.iter().filter(|backend| backend.plugin_id == plugin_id);
    match lifetime {
        BackendLifetime::Panel => None,
        BackendLifetime::Session(_) => same_plugin.filter(|backend| backend.owner_user_id == user_id).map(|backend| backend.id).min(),
        BackendLifetime::System(_) => same_plugin.map(|backend| backend.id).min(),
    }
}

fn spawn_backend_plugin_process(plugin_id: u32, plugin: &Plugin, user: &User) -> Result<u32, String> {
    debug!("Attempting to spawn plugin backend-process for plugin {}.", plugin_id);

    let be_pid = get_next_backend_pid();

    let (child_process, socket_handle) = plugin_backend_executor::execute_plugin_backend_process(plugin, be_pid)
        .map_err(|e| format!("could not start the backend of plugin '{}': {e}", plugin.identifier()))?;

    let socket_join_handle = plugin_message_router::establish_backend_streams(be_pid, socket_handle);

    let plugin_backend_process = PluginBackendProcess {
        plugin_id,
        process_id: be_pid,
        backend_lifetime: plugin.manifest.backend_specs.lifetime.clone(),
        owner: user.clone(),
        process: child_process,
        socket_join_handle
    };

    backend_processes().lock().unwrap().insert(be_pid, plugin_backend_process);

    debug!("Backend plugin process {} spawned for plugin {}.", be_pid, plugin_id);

    Ok(be_pid)
}

/// Frontend ids are sent by browsers; keep them harmless.
fn validate_page_id(page_id: &Option<String>) -> Result<(), String> {
    match page_id {
        Some(id) if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') => {
            Err("invalid page id".to_string())
        }
        _ => Ok(()),
    }
}

fn spawn_frontend(
    plugin_id: u32,
    component_name: &str,
    user: &User,
    parent_process_id: Option<u32>,
    page_id: Option<String>,
) -> Result<u32, String> {
    debug!("Attempting to spawn plugin frontend-process for plugin {} component {}.", plugin_id, component_name);

    let plugin = plugin_library::get_plugin_from_id(&plugin_id).ok_or("unknown plugin")?;

    // Choosing a backend and recording the new frontend must not interleave with another spawn,
    // or two frontends could each start a backend that one of them should have shared.
    static SPAWN_LOCK: Mutex<()> = Mutex::new(());
    let _guard = SPAWN_LOCK.lock().unwrap();

    let parent_backend = parent_process_id.and_then(|parent| {
        frontend_processes().lock().unwrap().get(&parent)
            .filter(|frontend| frontend.plugin_id == plugin_id)
            .map(|frontend| frontend.backend_process_id)
    });
    let running: Vec<BackendCandidate> = backend_processes().lock().unwrap().values()
        .map(|backend| BackendCandidate { id: backend.process_id, plugin_id: backend.plugin_id, owner_user_id: backend.owner.user_id })
        .collect();

    let backend_pid = match choose_backend(plugin_id, &plugin.manifest.backend_specs.lifetime, user.user_id, parent_backend, &running) {
        Some(backend_pid) => backend_pid,
        None => spawn_backend_plugin_process(plugin_id, &plugin, user)?,
    };

    let fe_pid = get_next_frontend_pid();

    let frontend_process = PluginFrontendProcess {
        plugin_id,
        component_name: component_name.to_string(),
        process_id: fe_pid,
        backend_process_id: backend_pid,
        owner: user.clone(),
        parent_process_id,
        page_id,
    };

    frontend_processes().lock().unwrap().insert(fe_pid, frontend_process);

    plugin_message_router::notify_frontend_attached(backend_pid, fe_pid);

    debug!("Frontend plugin process {} spawned for plugin {}.", fe_pid, plugin_id);

    Ok(fe_pid)
}

#[cfg(not(test))]
const BACKEND_SHUTDOWN_GRACE: Duration = Duration::from_secs(6);
#[cfg(test)]
const BACKEND_SHUTDOWN_GRACE: Duration = Duration::from_millis(300);

/// Closes a frontend process (its tab was closed, or its page went away). The backend is told, and a
/// backend with `lifetime = "panel"` is shut down once its last frontend is gone. `session` and
/// `system` backends keep running.
pub async fn close_frontend(fe_pid: u32) {
    let removed = frontend_processes().lock().unwrap().remove(&fe_pid);
    let Some(frontend) = removed else {
        return;
    };
    let backend_pid = frontend.backend_process_id;
    debug!("Frontend plugin process {} (component {}) closed.", fe_pid, frontend.component_name);

    plugin_message_router::notify_frontend_detached(backend_pid, fe_pid, &frontend.component_name).await;

    let ends_with_last_frontend = backend_processes().lock().unwrap().get(&backend_pid)
        .is_some_and(|backend| matches!(backend.backend_lifetime, BackendLifetime::Panel));
    if ends_with_last_frontend && get_frontends_for_backend(backend_pid).is_empty() {
        terminate_backend(backend_pid).await;
    }
}

/// Closes a frontend on behalf of its owner.
pub async fn close_frontend_of_user(fe_pid: u32, user: &User) -> Result<(), String> {
    match get_frontend_owner(fe_pid) {
        None => Ok(()), // already gone
        Some(owner) if owner.user_id == user.user_id => {
            close_frontend(fe_pid).await;
            Ok(())
        }
        Some(_) => Err("frontend process belongs to another user".to_string()),
    }
}

/// Closes every frontend a browser page opened; called when the page's connection ends.
pub async fn close_page_frontends(page_id: &str) {
    let ids: Vec<u32> = frontend_processes().lock().unwrap().values()
        .filter(|frontend| frontend.page_id.as_deref() == Some(page_id))
        .map(|frontend| frontend.process_id)
        .collect();
    for fe_pid in ids {
        close_frontend(fe_pid).await;
    }
}

/// Stops a backend: closing the connection is the SDK's shutdown signal (running handlers get a few seconds);
/// a process that has not exited after the grace period is killed.
async fn terminate_backend(be_pid: u32) {
    let record = backend_processes().lock().unwrap().remove(&be_pid);
    let Some(mut backend) = record else {
        return;
    };
    debug!("Shutting down backend plugin process {}.", be_pid);

    plugin_message_router::close_backend_connection(be_pid).await;

    tokio::spawn(async move {
        let deadline = Instant::now() + BACKEND_SHUTDOWN_GRACE;
        loop {
            match backend.process.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => tokio::time::sleep(Duration::from_millis(50)).await,
                _ => {
                    let _ = backend.process.kill();
                    let _ = backend.process.wait();
                    break;
                }
            }
        }
        plugin_backend_executor::remove_socket_file(be_pid);
        backend.socket_join_handle.abort();
    });
}

/// Params are handed to the component as `?params=<url-encoded JSON>`; keep them valid and small.
const MAX_PARAMS_LEN: usize = 16 * 1024;

fn validate_params(params: &Option<String>) -> Result<(), String> {
    let Some(text) = params else {
        return Ok(());
    };
    if text.len() > MAX_PARAMS_LEN {
        return Err(format!("params are larger than {MAX_PARAMS_LEN} bytes"));
    }
    serde_json::from_str::<serde_json::Value>(text).map(|_| ()).map_err(|e| format!("params are not valid JSON: {e}"))
}

/// Finds or spawns the frontend process for a component and describes it. With `reuse`, an
/// existing process of the same plugin component and owner is returned instead of a new one.
fn open_component_process(
    plugin_id: u32,
    plugin: &Plugin,
    component: &FrontendComponent,
    params: Option<String>,
    parent: Option<u32>,
    reuse: bool,
    user: &User,
    page_id: Option<String>,
) -> Result<SpawnedComponent, String> {
    validate_params(&params)?;
    validate_page_id(&page_id)?;

    let existing = if reuse {
        frontend_processes().lock().unwrap().values()
            .filter(|fe| fe.plugin_id == plugin_id && fe.component_name == component.name && fe.owner.user_id == user.user_id)
            .map(|fe| (fe.process_id, fe.parent_process_id))
            .min_by_key(|(fe_pid, _)| *fe_pid)
    } else {
        None
    };
    let (frontend_process_id, parent_frontend_process_id, reused) = match existing {
        Some((fe_pid, existing_parent)) => (fe_pid, existing_parent, true),
        None => (spawn_frontend(plugin_id, &component.name, user, parent, page_id)?, parent, false),
    };

    Ok(SpawnedComponent {
        plugin_id,
        plugin_name: plugin.manifest.description.name.clone(),
        component_name: component.name.clone(),
        title: component.title.clone().unwrap_or_else(|| plugin.manifest.description.name.clone()),
        target_panel: component.target_panel,
        frontend_process_id,
        parent_frontend_process_id,
        params,
        reused,
    })
}

/// Opens a plugin from the plugin picker: its entry component, without a parent.
pub fn open_plugin_entry(plugin_id: u32, user: &User, page_id: Option<String>) -> Result<SpawnedComponent, String> {
    let plugin = plugin_library::get_plugin_from_id(&plugin_id).ok_or("unknown plugin")?;
    let component = plugin.entry_component().ok_or("the plugin has no entry component")?.clone();
    open_component_process(plugin_id, &plugin, &component, None, None, false, user, page_id)
}

/// What a frontend asks to open.
pub enum ComponentTarget {
    /// A component of the requester's own plugin.
    Own { component: String },
    /// A component of a plugin the requester's manifest lists under `[dependencies]`; without a
    /// component name that plugin's entry component.
    Dependency { plugin: String, component: Option<String> },
}

/// Opens a component on behalf of `requester_fe_pid`, which becomes its parent.
pub fn open_component(
    requester_fe_pid: u32,
    target: ComponentTarget,
    params: Option<String>,
    reuse: bool,
    user: &User,
    page_id: Option<String>,
) -> Result<SpawnedComponent, String> {
    let (requester_plugin_id, requester_plugin) = requester_plugin(requester_fe_pid, user)?;

    let (plugin_id, plugin, component_name) = match target {
        ComponentTarget::Own { component } => (requester_plugin_id, requester_plugin, Some(component)),
        ComponentTarget::Dependency { plugin: identifier, component } => {
            if !requester_plugin.manifest.dependencies.iter().any(|declared| *declared == identifier) {
                return Err(format!(
                    "plugin '{}' does not declare '{}' under [dependencies]", requester_plugin.identifier(), identifier));
            }
            let (id, plugin) = plugin_library::get_plugin_by_identifier(&identifier)
                .ok_or_else(|| format!("dependency '{}' is not installed", identifier))?;
            (id, plugin, component)
        }
    };

    let component = match &component_name {
        Some(name) => plugin.component(name).ok_or_else(|| format!("plugin '{}' has no component '{}'", plugin.identifier(), name))?,
        None => default_component(&plugin).ok_or_else(|| format!(
            "plugin '{}' has several components and no entry component; name the component to open", plugin.identifier()))?,
    }.clone();

    open_component_process(plugin_id, &plugin, &component, params, Some(requester_fe_pid), reuse, user, page_id)
}

/// Opens a file viewer component for a file; the viewer receives `{"file": <path>}` as params.
pub fn open_file_viewer(plugin_id: u32, component_name: &str, file: &str, user: &User, page_id: Option<String>) -> Result<SpawnedComponent, String> {
    let plugin = plugin_library::get_plugin_from_id(&plugin_id).ok_or("unknown plugin")?;
    let component = plugin.component(component_name)
        .ok_or_else(|| format!("plugin '{}' has no component '{}'", plugin.identifier(), component_name))?.clone();
    let extension = file.rsplit_once('.').map(|(_, ext)| ext.to_lowercase()).unwrap_or_default();
    if !component.file_viewer_for.contains(&extension) {
        return Err(format!("component '{}' is not a viewer for '.{}' files", component_name, extension));
    }
    let params = serde_json::json!({ "file": file }).to_string();
    open_component_process(plugin_id, &plugin, &component, Some(params), None, false, user, page_id)
}

/// The component opened when none is named: the entry component, or the only component.
fn default_component(plugin: &Plugin) -> Option<&FrontendComponent> {
    plugin.entry_component().or_else(|| match plugin.manifest.frontend_components.as_slice() {
        [only] => Some(only),
        _ => None,
    })
}

fn requester_plugin(requester_fe_pid: u32, user: &User) -> Result<(u32, Plugin), String> {
    let plugin_id = {
        let frontends = frontend_processes().lock().unwrap();
        let requester = frontends.get(&requester_fe_pid).ok_or("unknown frontend process")?;
        if requester.owner.user_id != user.user_id {
            return Err("frontend process belongs to another user".to_string());
        }
        requester.plugin_id
    };
    let plugin = plugin_library::get_plugin_from_id(&plugin_id).ok_or("unknown plugin")?;
    Ok((plugin_id, plugin))
}

/// True if one of the two frontends opened the other as a dependency. Only such pairs may
/// exchange `peer://` messages.
pub fn are_related_frontends(a: u32, b: u32) -> bool {
    let frontends = frontend_processes().lock().unwrap();
    let parent_of = |fe: u32| frontends.get(&fe).and_then(|process| process.parent_process_id);
    a != b && (parent_of(a) == Some(b) || parent_of(b) == Some(a))
}

pub fn get_be_pid_for_fe_pid(fe_pid: u32) -> Option<u32> {
    let map = frontend_processes().lock().unwrap();
    let Some(frontend_process) = map.get(&fe_pid) else {
        return None;
    };
    Some(frontend_process.backend_process_id)
}

pub fn get_frontend_owner(fe_pid: u32) -> Option<User> {
    frontend_processes().lock().unwrap().get(&fe_pid).map(|fe| fe.owner.clone())
}

pub fn get_frontends_for_backend(be_pid: u32) -> Vec<u32> {
    frontend_processes()
        .lock()
        .unwrap()
        .values()
        .filter(|fe| fe.backend_process_id == be_pid)
        .map(|fe| fe.process_id)
        .collect()
}

pub fn get_frontend_component(fe_pid: u32) -> Option<String> {
    frontend_processes().lock().unwrap().get(&fe_pid).map(|fe| fe.component_name.clone())
}

#[cfg(test)]
pub fn insert_test_frontend(fe_pid: u32, plugin_id: u32, backend_pid: u32, owner_user_id: u32, parent: Option<u32>) {
    insert_test_frontend_component(fe_pid, plugin_id, "main", backend_pid, owner_user_id, parent);
}

#[cfg(test)]
pub fn insert_test_frontend_component(fe_pid: u32, plugin_id: u32, component: &str, backend_pid: u32, owner_user_id: u32, parent: Option<u32>) {
    frontend_processes().lock().unwrap().insert(fe_pid, PluginFrontendProcess {
        plugin_id,
        component_name: component.to_string(),
        process_id: fe_pid,
        backend_process_id: backend_pid,
        owner: User { user_id: owner_user_id },
        parent_process_id: parent,
        page_id: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;

    fn component(name: &str, panel: TargetPanel, viewer_for: &[&str]) -> FrontendComponent {
        FrontendComponent {
            name: name.into(),
            title: None,
            entry_point_file_path: format!("frontend/{name}/index.html"),
            target_panel: panel,
            file_viewer_for: viewer_for.iter().map(|e| e.to_string()).collect(),
        }
    }

    fn plugin(folder: &str, dependencies: &[&str], entry: Option<&str>, components: Vec<FrontendComponent>) -> Plugin {
        Plugin {
            plugin_folder_name: format!("/plugins/{folder}"),
            manifest: PluginManifest {
                meta_data: PluginMetaData { api_version: 3, entry_component: entry.map(str::to_string) },
                backend_specs: PluginBackendSpecification {
                    entry_point_file_path: "backend/plugin.py".into(),
                    lifetime: BackendLifetime::System(true),
                },
                frontend_components: components,
                description: PluginDescription {
                    name: folder.into(),
                    developer: String::new(),
                    description: String::new(),
                    version: String::new(),
                    contact: String::new(),
                },
                dependencies: dependencies.iter().map(|d| d.to_string()).collect(),
            },
        }
    }

    #[test]
    fn components_of_own_plugin_and_dependencies_are_resolved_and_checked() {
        plugin_library::insert_test_plugin(990_001, plugin("test.parent", &["test.child"], Some("main"), vec![
            component("main", TargetPanel::Center, &[]),
            component("side", TargetPanel::Right, &[]),
        ]));
        plugin_library::insert_test_plugin(990_002, plugin("test.child", &[], Some("main"), vec![
            component("main", TargetPanel::Bottom, &[]),
        ]));
        plugin_library::insert_test_plugin(990_003, plugin("test.stranger", &[], None, vec![
            component("main", TargetPanel::Center, &[]),
        ]));
        insert_test_frontend(990_101, 990_001, 1, 7, None);
        insert_test_frontend(990_102, 990_003, 1, 7, None);
        let owner = User { user_id: 7 };

        // dependency: component defaults to the entry component, panel from its manifest
        let (id, plugin, component) = {
            let (requester_id, requester) = requester_plugin(990_101, &owner).unwrap();
            assert_eq!(requester_id, 990_001);
            assert_eq!(requester.identifier(), "test.parent");
            let (id, dep) = plugin_library::get_plugin_by_identifier("test.child").unwrap();
            let c = dep.entry_component().unwrap().clone();
            (id, dep, c)
        };
        assert_eq!((id, plugin.identifier(), component.target_panel), (990_002, "test.child".to_string(), TargetPanel::Bottom));

        // errors that need no process spawning
        let open = |fe, target| open_component(fe, target, None, false, &owner, None);
        assert!(open(990_101, ComponentTarget::Dependency { plugin: "test.stranger".into(), component: None }).is_err()); // not declared
        assert!(open(990_102, ComponentTarget::Dependency { plugin: "test.child".into(), component: None }).is_err());    // requester declares nothing
        assert!(open(990_101, ComponentTarget::Own { component: "nope".into() }).is_err());                               // unknown component
        assert!(open(555, ComponentTarget::Own { component: "main".into() }).is_err());                                   // unknown frontend
        assert!(open_component(990_101, ComponentTarget::Own { component: "main".into() }, None, false, &User { user_id: 8 }, None).is_err()); // other user
        assert!(open_component(990_101, ComponentTarget::Own { component: "side".into() }, Some("{not json".into()), false, &owner, None).is_err()); // bad params
    }

    #[test]
    fn unnamed_component_defaults_to_entry_or_the_only_component() {
        let single = plugin("test.single", &[], None, vec![component("only", TargetPanel::Left, &[])]);
        assert_eq!(default_component(&single).unwrap().name, "only");
        let entry = plugin("test.entry", &[], Some("b"), vec![component("a", TargetPanel::Left, &[]), component("b", TargetPanel::Left, &[])]);
        assert_eq!(default_component(&entry).unwrap().name, "b");
        let ambiguous = plugin("test.many", &[], None, vec![component("a", TargetPanel::Left, &[]), component("b", TargetPanel::Left, &[])]);
        assert!(default_component(&ambiguous).is_none());
    }

    #[test]
    fn reuse_returns_the_existing_process_without_spawning() {
        plugin_library::insert_test_plugin(990_011, plugin("test.reuse", &[], Some("main"), vec![
            component("main", TargetPanel::Left, &[]),
            component("editor", TargetPanel::Center, &[]),
        ]));
        insert_test_frontend_component(990_111, 990_011, "main", 1, 7, None);
        insert_test_frontend_component(990_112, 990_011, "editor", 1, 7, Some(990_111));
        let owner = User { user_id: 7 };

        let reused = open_component(990_111, ComponentTarget::Own { component: "editor".into() }, None, true, &owner, None).unwrap();
        assert!(reused.reused);
        assert_eq!(reused.frontend_process_id, 990_112);
        assert_eq!(reused.target_panel, TargetPanel::Center);
        assert_eq!(reused.parent_frontend_process_id, Some(990_111));
        assert_eq!(reused.title, "test.reuse");
    }

    #[test]
    fn file_viewers_are_found_by_lowercase_extension() {
        plugin_library::insert_test_plugin(990_021, plugin("test.viewer-a", &[], None, vec![
            component("main", TargetPanel::Center, &["zzq", "zzr"]),
        ]));
        plugin_library::insert_test_plugin(990_022, plugin("test.viewer-b", &[], None, vec![
            component("view", TargetPanel::Right, &["zzq"]),
        ]));
        let viewers = plugin_library::get_file_viewers("ZZQ");
        let found: Vec<(u32, String)> = viewers.iter().map(|v| (v.plugin_id, v.component_name.clone())).collect();
        assert_eq!(found, vec![(990_021, "main".to_string()), (990_022, "view".to_string())]);
        assert_eq!(plugin_library::get_file_viewers("zzr").len(), 1);
        assert!(plugin_library::get_file_viewers("nothing").is_empty());

        // a viewer is only opened for files it declared
        assert!(open_file_viewer(990_021, "main", "/data/file.pdf", &User { user_id: 7 }, None).is_err());
        assert!(open_file_viewer(990_021, "missing", "/data/file.zzq", &User { user_id: 7 }, None).is_err());
    }

    #[test]
    fn only_parent_and_child_are_related() {
        insert_test_frontend(990_201, 1, 1, 7, None);
        insert_test_frontend(990_202, 1, 1, 7, Some(990_201));
        insert_test_frontend(990_203, 1, 1, 7, Some(990_201));
        assert!(are_related_frontends(990_201, 990_202));
        assert!(are_related_frontends(990_202, 990_201));
        assert!(!are_related_frontends(990_202, 990_203)); // siblings (the browser routes those locally)
        assert!(!are_related_frontends(990_201, 990_201));
        assert!(!are_related_frontends(990_201, 999_999));
    }

    fn candidates() -> Vec<BackendCandidate> {
        vec![
            BackendCandidate { id: 20_005, plugin_id: 1, owner_user_id: 7 },
            BackendCandidate { id: 20_002, plugin_id: 1, owner_user_id: 8 },
            BackendCandidate { id: 20_009, plugin_id: 2, owner_user_id: 7 },
        ]
    }

    #[test]
    fn backend_choice_follows_the_lifetime() {
        let panel = BackendLifetime::Panel;
        let session = BackendLifetime::Session(true);
        let system = BackendLifetime::System(true);
        let running = candidates();

        // a component opened by a component of the same plugin always shares the parent's backend
        for lifetime in [&panel, &session, &system] {
            assert_eq!(choose_backend(1, lifetime, 7, Some(20_777), &running), Some(20_777));
        }
        // panel: always a new backend
        assert_eq!(choose_backend(1, &panel, 7, None, &running), None);
        // session: the user's own running backend of that plugin
        assert_eq!(choose_backend(1, &session, 7, None, &running), Some(20_005));
        assert_eq!(choose_backend(1, &session, 8, None, &running), Some(20_002));
        assert_eq!(choose_backend(1, &session, 9, None, &running), None);
        assert_eq!(choose_backend(3, &session, 7, None, &running), None);
        // system: any user's running backend of that plugin (the oldest)
        assert_eq!(choose_backend(1, &system, 9, None, &running), Some(20_002));
        assert_eq!(choose_backend(2, &system, 9, None, &running), Some(20_009));
        assert_eq!(choose_backend(3, &system, 9, None, &running), None);
    }

    /// Registers a fake backend: a real child process that just sleeps.
    async fn insert_test_backend(be_pid: u32, lifetime: BackendLifetime) {
        let child = process::Command::new("sleep").arg("30").spawn().unwrap();
        let socket = std::env::temp_dir().join(format!("salus-test-lifecycle-{be_pid}.sock"));
        let socket_join_handle = tokio::spawn(async move { BackendPluginSocketHandle::bind(socket).unwrap() });
        backend_processes().lock().unwrap().insert(be_pid, PluginBackendProcess {
            plugin_id: 1,
            process_id: be_pid,
            backend_lifetime: lifetime,
            owner: User { user_id: 7 },
            process: child,
            socket_join_handle,
        });
    }

    async fn backend_exited(be_pid: u32) -> bool {
        // the record is removed on termination; the child is reaped by the shutdown task shortly after
        for _ in 0..60 {
            if !backend_processes().lock().unwrap().contains_key(&be_pid) {
                tokio::time::sleep(BACKEND_SHUTDOWN_GRACE + Duration::from_millis(400)).await;
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }

    #[tokio::test]
    async fn a_panel_backend_ends_with_its_last_frontend() {
        insert_test_backend(995_001, BackendLifetime::Panel).await;
        insert_test_frontend_component(995_101, 1, "main", 995_001, 7, None);
        insert_test_frontend_component(995_102, 1, "side", 995_001, 7, Some(995_101));

        close_frontend(995_101).await;
        assert!(backend_processes().lock().unwrap().contains_key(&995_001), "a frontend is still open");
        assert!(frontend_processes().lock().unwrap().get(&995_102).is_some());

        close_frontend(995_102).await;
        assert!(backend_exited(995_001).await);
        assert!(frontend_processes().lock().unwrap().get(&995_102).is_none());
    }

    #[tokio::test]
    async fn session_and_system_backends_stay_running() {
        for (be_pid, lifetime) in [(995_011, BackendLifetime::Session(true)), (995_012, BackendLifetime::System(true))] {
            insert_test_backend(be_pid, lifetime).await;
            insert_test_frontend_component(be_pid + 100, 1, "main", be_pid, 7, None);
            close_frontend(be_pid + 100).await;
            tokio::time::sleep(BACKEND_SHUTDOWN_GRACE + Duration::from_millis(300)).await;
            let mut backends = backend_processes().lock().unwrap();
            let backend = backends.get_mut(&be_pid).expect("backend must stay registered");
            assert!(backend.process.try_wait().unwrap().is_none(), "backend must keep running");
            backend.process.kill().unwrap();
            let _ = backend.process.wait();
        }
    }

    #[tokio::test]
    async fn closing_a_page_closes_its_frontends_and_only_the_owner_may_close_one() {
        insert_test_backend(995_021, BackendLifetime::Panel).await;
        insert_test_frontend_component(995_121, 1, "main", 995_021, 7, None);
        insert_test_frontend_component(995_122, 1, "side", 995_021, 7, None);
        frontend_processes().lock().unwrap().get_mut(&995_121).unwrap().page_id = Some("page-a".into());
        frontend_processes().lock().unwrap().get_mut(&995_122).unwrap().page_id = Some("page-b".into());

        assert!(close_frontend_of_user(995_121, &User { user_id: 8 }).await.is_err());
        assert!(frontend_processes().lock().unwrap().contains_key(&995_121));

        close_page_frontends("page-a").await;
        assert!(!frontend_processes().lock().unwrap().contains_key(&995_121));
        assert!(frontend_processes().lock().unwrap().contains_key(&995_122));

        close_frontend_of_user(995_122, &User { user_id: 7 }).await.unwrap();
        assert!(backend_exited(995_021).await);
        // closing something that is already gone is fine
        close_frontend_of_user(995_122, &User { user_id: 7 }).await.unwrap();
    }

    #[test]
    fn page_ids_are_validated() {
        assert!(validate_page_id(&None).is_ok());
        assert!(validate_page_id(&Some("abc-123_X".into())).is_ok());
        assert!(validate_page_id(&Some("".into())).is_err());
        assert!(validate_page_id(&Some("a b".into())).is_err());
        assert!(validate_page_id(&Some("x".repeat(65))).is_err());
    }

    #[test]
    fn params_must_be_small_valid_json() {
        assert!(validate_params(&None).is_ok());
        assert!(validate_params(&Some("{\"file\":\"/a\"}".into())).is_ok());
        assert!(validate_params(&Some("nope".into())).is_err());
        assert!(validate_params(&Some("\"".repeat(MAX_PARAMS_LEN + 1))).is_err());
    }
}
