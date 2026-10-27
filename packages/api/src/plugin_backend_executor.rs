use crate::models::Plugin;
use crate::plugin_message_router::{Stream};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::fd::AsRawFd;
use tokio::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Arc;
use dioxus::prelude::*;
use log::error;
// TODO reimplement by hand. Now it's AI generated but I want to learn :)


pub struct BackendPluginSocketHandle {
    socket_listener: UnixListener,
    socket_path: PathBuf,
}

impl BackendPluginSocketHandle {
    pub fn bind(path: PathBuf) -> io::Result<Self> {
        // Best-effort cleanup of a stale socket file left behind by a
        // previous crash (SIGKILL/abort give us no chance to run Drop).
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&path)?;
        Ok(Self {
            socket_listener: listener,
            socket_path: path
        })
    }

    pub async fn accept(&self) -> io::Result<UnixStream> {
        let (stream, _addr) = self.socket_listener.accept().await.unwrap();
        Ok(stream)
    }

    pub fn drop(&mut self, path_buf: PathBuf) {
        fs::remove_file(path_buf);
    }
}


pub fn execute_plugin_backend_process(
    plugin: &Plugin,
    be_process_id: u32,
) -> io::Result<(Child, BackendPluginSocketHandle)> {
    let tmp = PathBuf::from_iter([
        &plugin.plugin_folder_name,
        &plugin.manifest.backend_specs.entry_point_file_path,
    ]);
    let entrypoint = tmp.to_str().unwrap();

    let path = socket_path(be_process_id);

    // TODO can bind just take a reference instead so we can remove clone?
    let handle = match BackendPluginSocketHandle::bind(path.clone()) {
        Ok(h) => h,
        Err(e) => {
            // TODO in this case, more should happen, like the plugin should display an error message
            info!("Could not bind Unix listener to path {}: {}", &path.display(), e.to_string());
            return Err(e);
        }
    };

    let mut cmd = Command::new(entrypoint);
    cmd.arg(&path); // tell the spawned process where to connect

    let child = spawn_tied(&mut cmd)?;

    Ok((child, handle))
}

// ============================================================================
// Unix socket Listener implementation
// ============================================================================

pub fn remove_socket_file(be_process_id: u32) {
    let _ = fs::remove_file(socket_path(be_process_id));
}

fn socket_path(be_process_id: u32) -> PathBuf {
    std::env::temp_dir().join(format!("plugin-{be_process_id}.sock"))
}

// ============================================================================
// spawn_tied (macOS + Linux only)
// ============================================================================

/// Spawn `cmd` so that the child dies when the current process dies.
///
/// Linux: PR_SET_PDEATHSIG (SIGKILL). Tied to the *thread* that calls this,
///        so call it from a long-lived thread (e.g. main).
/// macOS: the program is launched via `/bin/sh`, which watches an inherited
///        pipe (fd 3) and kills the program on EOF, i.e. when this process
///        is gone. Because the shell `exec`s the real program, the returned
///        `Child` has the program's real pid, so kill()/wait()/id() behave
///        normally.
pub fn spawn_tied(cmd: &mut Command) -> io::Result<Child> {
    spawn_impl(cmd)
}

#[cfg(target_os = "linux")]
fn spawn_impl(cmd: &mut Command) -> io::Result<Child> {
    unsafe {
        cmd.pre_exec(|| {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(io::Error::last_os_error());
            }
            // Parent may have died between fork() and prctl().
            if libc::getppid() == 1 {
                libc::_exit(1);
            }
            Ok(())
        });
    }
    cmd.spawn()
}

#[cfg(target_os = "macos")]
fn spawn_impl(cmd: &mut Command) -> io::Result<Child> {
    // The shell script:
    //   * starts a background subshell that blocks on fd 3 until EOF, then
    //     SIGKILLs $$ (which, after `exec`, is the real program's pid);
    //   * `exec`s the real program with fd 3 closed, so the program itself
    //     never sees the pipe and keeps the shell's pid.
    const SCRIPT: &str =
        r#"( cat <&3 >/dev/null 2>&1; kill -9 "$$" 2>/dev/null ) & exec "$0" "$@" 3<&-"#;

    let (reader, writer) = io::pipe()?;
    std::mem::forget(writer); // intentionally leaked: closes only at process death
    let read_fd = reader.as_raw_fd();

    let program: OsString = cmd.get_program().to_os_string();
    let args: Vec<OsString> = cmd.get_args().map(|a| a.to_os_string()).collect();

    let mut sh = Command::new("/bin/sh");
    sh.arg("-c").arg(SCRIPT).arg(&program).args(&args);

    if let Some(dir) = cmd.get_current_dir() {
        sh.current_dir(dir);
    }
    for (k, v) in cmd.get_envs() {
        match v {
            Some(v) => { sh.env(k, v); }
            None => { sh.env_remove(k); }
        }
    }

    unsafe {
        sh.pre_exec(move || {
            if libc::dup2(read_fd, 3) < 0 {
                return Err(io::Error::last_os_error());
            }
            // If read_fd happened to already be 3, dup2 was a no-op and the
            // CLOEXEC flag is still set; clear it.
            if libc::fcntl(3, libc::F_SETFD, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = sh.spawn();
    drop(reader);
    child
}
