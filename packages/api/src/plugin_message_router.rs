use std::io::Write;
use crate::plugin_process_manager::{self};
use crate::plugin_backend_executor::BackendPluginSocketHandle;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;
use tokio::io::{ReadHalf, AsyncReadExt, WriteHalf, AsyncWriteExt, AsyncRead, AsyncWrite};
use std::collections::HashMap;
use dioxus::logger::tracing::field::debug;
use dioxus::prelude::ReadableOptionExt;
use tokio::task::JoinHandle;
use crate::message_frame::{PluginMessageFrame, ALL_FRONTENDS, FLAG_MORE_FRAGMENTS, MAX_FRAME_SIZE};
use crate::{browser_connections, http_gateway, salus_control};
use std::sync::atomic::{AtomicU32, Ordering};
use dioxus::prelude::*;
use tokio::net::UnixStream;

// Cap for reassembled fragmented messages.
const MAX_MESSAGE_SIZE: usize = 1024 * 1024 * 1024;

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

// TODO replace all "dyn" by compile time traits
fn get_stream_writers() -> &'static Mutex<HashMap<u32, Arc<Mutex<WriteHalf<UnixStream>>>>> {
    static WRITERS_FOR_PLUGIN: OnceLock<Mutex<HashMap<u32, Arc<Mutex<WriteHalf<UnixStream>>>>>> = OnceLock::new();
    WRITERS_FOR_PLUGIN.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn get_write_stream_for(backend_pid: u32) -> Option<Arc<Mutex<WriteHalf<UnixStream>>>> {
    let write_stream = get_stream_writers().lock().await.get(&backend_pid)?.clone();

    Some(write_stream)
}

fn connected_since() -> &'static std::sync::Mutex<HashMap<u32, std::time::Instant>> {
    static CONNECTED_SINCE: OnceLock<std::sync::Mutex<HashMap<u32, std::time::Instant>>> = OnceLock::new();
    CONNECTED_SINCE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// When the backend's socket connection was established; `None` while it is not connected.
pub fn backend_connected_since(be_pid: u32) -> Option<std::time::Instant> {
    connected_since().lock().unwrap().get(&be_pid).copied()
}

fn next_message_id() -> u32 {
    static NEXT_MESSAGE_ID: AtomicU32 = AtomicU32::new(1);
    NEXT_MESSAGE_ID.fetch_add(1, Ordering::SeqCst)
}

pub fn establish_backend_streams(
    be_pid: u32,
    socket_handle: BackendPluginSocketHandle
) -> JoinHandle<BackendPluginSocketHandle> {
    let task = async move {
        info!("Waiting for plugin backend [{}] to connect to socket... (1/3)", be_pid); // TODO add socket details

        let stream = socket_handle.accept().await.unwrap();

        info!("Plugin backend [{}] connected to socket. Establishing stream... (2/3)", be_pid);

        let (reader, writer) = tokio::io::split(stream);

        let arc_mutex_writer = Arc::new(Mutex::new(writer));

        get_stream_writers().lock().await.insert(be_pid, arc_mutex_writer);
        connected_since().lock().unwrap().insert(be_pid, std::time::Instant::now());

        loop_backend_plugin_stream_read(be_pid, reader);

        // The SDK learns about bound frontends solely from these events.
        for fe_pid in plugin_process_manager::get_frontends_for_backend(be_pid) {
            salus_control::send_frontend_attached(be_pid, fe_pid).await;
        }

        info!("Established backend streams for backend plugin [{}]. (3/3)", be_pid);

        socket_handle
    };

    let handle = tokio::spawn(task);
    handle
}

/// Tells a (possibly not yet connected) backend about a newly bound frontend. If the backend
/// is not connected yet, the event is sent once it connects.
pub fn notify_frontend_attached(be_pid: u32, fe_pid: u32) {
    tokio::spawn(async move {
        if get_write_stream_for(be_pid).await.is_some() {
            salus_control::send_frontend_attached(be_pid, fe_pid).await;
        }
    });
}

/// Tells a connected backend that a frontend went away.
pub async fn notify_frontend_detached(be_pid: u32, fe_pid: u32, component: &str) {
    if get_write_stream_for(be_pid).await.is_some() {
        salus_control::send_frontend_detached(be_pid, fe_pid, component).await;
    }
}

/// Closes our end of a backend's connection. The SDK takes that as the signal to shut down.
pub async fn close_backend_connection(be_pid: u32) {
    if let Some(writer) = get_write_stream_for(be_pid).await {
        let _ = writer.lock().await.shutdown().await;
    }
}

fn invalid_data(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg.to_string())
}

/// Writes one complete frame (length prefix + body) while holding the stream lock, so frames
/// of different sources never interleave byte-wise.
async fn write_frame(be_pid: u32, frame: &PluginMessageFrame) -> std::io::Result<()> {
    let enc_frame = frame.encode()?;

    let stream_mutex = get_write_stream_for(be_pid).await.ok_or(
        std::io::Error::new(std::io::ErrorKind::NotFound, "invalid backend process id."))?;

    let mut out = Vec::with_capacity(4 + enc_frame.len());
    out.extend_from_slice(&(enc_frame.len() as u32).to_le_bytes());
    out.extend_from_slice(&enc_frame);

    let mut stream = stream_mutex.lock().await;
    stream.write_all(&out).await
}

/// Sends a framework-originated message to a backend. Fails with `InvalidData` if it does not
/// fit into one frame (fragmenting towards the backend is not implemented yet).
pub async fn send_to_backend(be_pid: u32, frontend_process_id: u32, channel: &str, payload: Vec<u8>) -> std::io::Result<()> {
    write_frame(be_pid, &PluginMessageFrame {
        frontend_process_id,
        message_id: next_message_id(),
        flags: 0,
        logical_channel: channel.to_string(),
        payload,
    }).await
}

struct PartialMessage {
    channel: String,
    data: Vec<u8>,
    poisoned: bool,
}

type PartialMessages = HashMap<(u32, u32), PartialMessage>;

/// Returns the complete message once its last fragment arrived.
fn reassemble(partial: &mut PartialMessages, frame: PluginMessageFrame) -> Option<PluginMessageFrame> {
    let key = (frame.frontend_process_id, frame.message_id);
    let more = frame.flags & FLAG_MORE_FRAGMENTS != 0;

    if !more && !partial.contains_key(&key) {
        return Some(frame);
    }

    let entry = partial.entry(key).or_insert_with(|| PartialMessage {
        channel: frame.logical_channel.clone(),
        data: Vec::new(),
        poisoned: false,
    });

    if entry.channel != frame.logical_channel {
        entry.poisoned = true;
    }
    if !entry.poisoned {
        entry.data.extend_from_slice(&frame.payload);
        if entry.data.len() > MAX_MESSAGE_SIZE {
            entry.poisoned = true;
            entry.data = Vec::new();
        }
    }

    if more {
        return None;
    }

    let entry = partial.remove(&key)?;
    if entry.poisoned {
        warn!("Dropping fragmented message {:?}: channel mismatch or size cap exceeded", key);
        return None;
    }
    Some(PluginMessageFrame { flags: 0, payload: entry.data, ..frame })
}

async fn read_backend_frames(be_pid: u32, read_half: &mut ReadHalf<UnixStream>) -> std::io::Result<()> {
    let mut partial = PartialMessages::new();

    loop {
        let mut len_bytes = [0u8; 4];
        read_half.read_exact(&mut len_bytes).await?;
        let len = u32::from_le_bytes(len_bytes) as usize;
        if len > MAX_FRAME_SIZE {
            return Err(invalid_data("frame exceeds 64 MiB"));
        }

        let mut body = vec![0u8; len];
        read_half.read_exact(&mut body).await?;

        let frame = match PluginMessageFrame::decode(&body) {
            Ok(frame) => frame,
            Err(e) => {
                warn!("Backend [{}]: dropping invalid frame: {}", be_pid, e);
                continue;
            }
        };

        dispatch_backend_frame(be_pid, frame, &mut partial).await;
    }
}

async fn dispatch_backend_frame(be_pid: u32, frame: PluginMessageFrame, partial: &mut PartialMessages) {
    let channel = frame.logical_channel.as_str();

    if channel.starts_with(salus_control::SALUS_PREFIX) {
        if let Some(message) = reassemble(partial, frame) {
            // Own task so a slow reply never stalls reading from the backend.
            tokio::spawn(salus_control::handle_backend_request(be_pid, message));
        }
    } else if channel.starts_with("http://") {
        if let Some(message) = reassemble(partial, frame) {
            http_gateway::handle_backend_frame(be_pid, message);
        }
    } else {
        forward_to_frontends(be_pid, frame).await;
    }
}

/// `ws://` and unknown channels are forwarded unchanged to the target frontend (or all frontends
/// bound to this backend for `ALL_FRONTENDS`).
async fn forward_to_frontends(be_pid: u32, frame: PluginMessageFrame) {
    if frame.frontend_process_id == ALL_FRONTENDS {
        for fe_pid in plugin_process_manager::get_frontends_for_backend(be_pid) {
            browser_connections::deliver_to_frontend(&PluginMessageFrame { frontend_process_id: fe_pid, ..frame.clone() });
        }
        return;
    }

    if plugin_process_manager::get_be_pid_for_fe_pid(frame.frontend_process_id) != Some(be_pid) {
        warn!("Backend [{}]: dropping frame for frontend {} that is not bound to it", be_pid, frame.frontend_process_id);
        return;
    }
    browser_connections::deliver_to_frontend(&frame);
}

fn loop_backend_plugin_stream_read(be_pid: u32, mut read_half: ReadHalf<UnixStream>) {
    let read_loop = async move {
        match read_backend_frames(be_pid, &mut read_half).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                info!("Backend [{}] closed its connection.", be_pid);
            }
            Err(e) => warn!("Backend [{}] read error: {}", be_pid, e),
        }

        get_stream_writers().lock().await.remove(&be_pid);
        connected_since().lock().unwrap().remove(&be_pid);
        http_gateway::backend_disconnected(be_pid);
    };

    tokio::spawn(read_loop);
}

pub async fn forward_message_to_backend(message: PluginMessageFrame) -> std::io::Result<()> {

    debug!("Sending message frame to backend: {}", message.frontend_process_id);

    let backend_pid = plugin_process_manager::get_be_pid_for_fe_pid(message.frontend_process_id).ok_or(
        std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no be_pid found for fe_pid")
    )?;

    write_frame(backend_pid, &message).await
}

// pub fn register_plugin_backend_receiver()