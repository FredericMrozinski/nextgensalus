use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU32, Ordering};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};
use crate::models::{Plugin, User, BackendLifetime};
use crate::plugin_library;
use crate::plugin_backend_executor::{self, BackendPluginSocketHandle};
use std::process;
use std::sync::Arc;
use dioxus::html::KeyCode::P;
use dioxus::prelude::debug;
use tokio::task::JoinHandle;
use tokio::net::UnixListener;
use crate::plugin_message_router::{self, Stream};

pub struct PluginFrontendProcess {
    pub plugin_id: u32,
    pub process_id: u32,
    pub backend_process_id: u32,
    pub owner: User,
}

pub struct PluginBackendProcess {
    pub plugin_id: u32,
    pub process_id: u32,
    pub backend_lifetime: BackendLifetime,
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

fn get_connectable_backend_process_id(plugin_id: u32, user: &User) -> Option<u32> {
    let b_processes = backend_processes().lock().unwrap();

    for (_pid, _process) in b_processes.iter() {
        let _other = plugin_library::get_plugin_from_id(_pid).unwrap();

        if _process.plugin_id == plugin_id {
            return match _process.backend_lifetime {
                BackendLifetime::Panel => {
                    None
                }
                BackendLifetime::Session(_) => {
                    None // TODO implement
                }
                BackendLifetime::System(_) => {
                    Some(*_pid)
                }
            }
        }
    }

    None
}

fn spawn_backend_plugin_process(plugin_id: u32) -> u32 {
    debug!("Attempting to spawn plugin backend-process for plugin {}.", plugin_id);

    let be_pid = get_next_backend_pid();

    // TODO go through the entire code and replace those unwraps. They might be unsafe

    let Ok((child_process, socket_handle)) = plugin_backend_executor::execute_plugin_backend_process(
        &plugin_library::get_plugin_from_id(&plugin_id).unwrap(),
        be_pid,
    ) else {
        debug!("Error while starting backend process for plugin {}.", plugin_id);
        return 0; // TODO return an error instead
    };

    let socket_join_handle = plugin_message_router::establish_backend_streams(be_pid, socket_handle);

    let mut plugin_backend_process = PluginBackendProcess {
        plugin_id,
        process_id: be_pid,
        backend_lifetime: BackendLifetime::System(true), // TODO change and adapt!!
        process: child_process,
        socket_join_handle
    };

    backend_processes().lock().unwrap().insert(be_pid, plugin_backend_process);

    debug!("Backend plugin process {} spawned for plugin {}.", be_pid, plugin_id);

    be_pid
}

pub fn spawn_frontend_plugin_process(plugin_id: u32, user: &User) -> u32 {
    debug!("Attempting to spawn plugin frontend-process for plugin {}.", plugin_id);

    // TODO this "match" should be moved into spawn_backend_plugin_process
    let backend_pid = match get_connectable_backend_process_id(plugin_id, user) {
        Some(backend_pid) => backend_pid,
        None => spawn_backend_plugin_process(plugin_id),
    };

    let fe_pid = get_next_frontend_pid();

    let frontend_process = PluginFrontendProcess {
        plugin_id,
        process_id: fe_pid,
        backend_process_id: backend_pid,
        owner: user.clone(), // requires `User: Clone` — add #[derive(Clone)] if it's missing
    };

    frontend_processes().lock().unwrap().insert(fe_pid, frontend_process);

    plugin_message_router::notify_frontend_attached(backend_pid, fe_pid);

    debug!("Frontend plugin process {} spawned for plugin {}.", fe_pid, plugin_id);

    fe_pid
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
