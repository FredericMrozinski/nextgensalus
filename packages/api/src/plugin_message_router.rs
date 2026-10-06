use std::io::Write;
use crate::plugin_process_manager::{self};
use crate::plugin_backend_executor::BackendPluginSocketHandle;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;
use axum::extract::ws::{Message, WebSocketUpgrade};
use axum::response::IntoResponse;
use tokio::io::{ReadHalf, AsyncReadExt, WriteHalf, AsyncWriteExt, AsyncRead, AsyncWrite};
use std::collections::HashMap;
use dioxus::logger::tracing::field::debug;
use dioxus::prelude::ReadableOptionExt;
use futures_util::{sink::SinkExt, stream::StreamExt};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use crate::message_frame::PluginMessageFrame;
use dioxus::prelude::*;
use tokio::net::UnixStream;

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

// TODO replace my own Stream type by tokio's Stream type
static AUXILIARY_SENDER: OnceLock<Sender<Vec<u8>>> = OnceLock::new();
static AUXILIARY_RECEIVER: OnceLock<Mutex<Receiver<Vec<u8>>>> = OnceLock::new();

fn init_auxiliary_channel() -> &'static Sender<Vec<u8>> {
    AUXILIARY_SENDER.get_or_init(|| {
        let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(32);
        AUXILIARY_RECEIVER.set(Mutex::new(rx))
            .unwrap_or_else(|_| panic!("channel already initialized"));
        tx
    })
}

pub fn auxiliary_sender() -> &'static Sender<Vec<u8>> {
    init_auxiliary_channel()
}

pub fn auxiliary_receiver() -> &'static Mutex<Receiver<Vec<u8>>> {
    // ensure init has run, then hand back the receiver lock
    init_auxiliary_channel();
    AUXILIARY_RECEIVER.get().expect("channel initialized")
}

// TODO replace all "dyn" by compile time traits
fn get_stream_writers() -> &'static Mutex<HashMap<u32, Arc<Mutex<WriteHalf<UnixStream>>>>> {
    static WRITERS_FOR_PLUGIN: OnceLock<Mutex<HashMap<u32, Arc<Mutex<WriteHalf<UnixStream>>>>>> = OnceLock::new();
    WRITERS_FOR_PLUGIN.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn get_write_stream_for(backend_pid: u32) -> Option<Arc<Mutex<WriteHalf<UnixStream>>>> {
    let write_stream = get_stream_writers().lock().await.get(&backend_pid)?.clone();

    Some(write_stream)
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

        loop_backend_plugin_stream_read(reader);

        info!("Established backend streams for backend plugin [{}]. (3/3)", be_pid);

        socket_handle
    };

    let handle = tokio::spawn(task);
    handle
}

fn loop_backend_plugin_stream_read(mut read_half: ReadHalf<UnixStream>) {

    let read_loop = async move {
        let mut message_frame_bytes: Vec<u8> = Vec::new();
        let mut message_frame_size: usize = 0;
        let mut buf = vec![0u8; 4096];

        loop {
            let tx = auxiliary_sender();

            match read_half.read(&mut buf).await {
                Ok(0) => {
                    // EOF, peer closed the connection
                    break;
                }
                Ok(n) => {
                    let mut received_message_frame_bytes = &buf[..];
                    // First read has the message frame length suffix which needs to be read
                    if message_frame_bytes.len() == 0 {
                        let size_bytes: [u8; 4] = buf[..4].try_into().unwrap();
                        message_frame_size = u32::from_le_bytes(size_bytes) as usize;

                        received_message_frame_bytes = &buf[4..];
                    }

                    let num_bytes_to_collect = message_frame_size - message_frame_bytes.len();

                    if num_bytes_to_collect < received_message_frame_bytes.len() {
                        received_message_frame_bytes = &received_message_frame_bytes[..num_bytes_to_collect];
                    }

                    message_frame_bytes.extend(received_message_frame_bytes);

                    if message_frame_bytes.len() == message_frame_size {

                        tx.send(message_frame_bytes).await;

                        message_frame_bytes = Vec::new();
                        message_frame_size = 0;
                    }
                }
                Err(e) => {
                    eprintln!("read error: {e}");
                    break;
                }
            }
        }
    };

    tokio::spawn(read_loop);
}

// TODO rename this function
pub async fn plugin_stream(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|mut socket| async move {

        let (mut writer, mut reader) = socket.split();

        let read_task = tokio::spawn(async move {
            while let Some(Ok(Message::Binary(frame))) = reader.next().await {
                let Ok(dec_frame) = PluginMessageFrame::decode(&frame) else {
                    continue;
                };

                forward_message_to_backend(dec_frame).await;
            }
        });

        let mut rx = auxiliary_receiver().lock().await;
        let write_task = tokio::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                debug!("Received bytes to forward: {:?}", bytes);

                if writer.send(Message::Binary(bytes.into())).await.is_err() {
                    break;
                }
            }
        });
    })
}

pub async fn forward_message_to_backend(message: PluginMessageFrame) -> std::io::Result<()> {

    debug!("Sending message frame to backend: {}", message.frontend_process_id);

    let backend_pid = plugin_process_manager::get_be_pid_for_fe_pid(message.frontend_process_id).ok_or(
        std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no be_pid found for fe_pid")
    )?;

    let enc_frame = message.encode()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    let stream_mutex = get_write_stream_for(backend_pid).await.ok_or(
        std::io::Error::new(std::io::ErrorKind::NotFound, "invalid backend process id."))?;
    let mut stream = stream_mutex.lock().await;

    let len = enc_frame.len() as u32;
    stream.write_all(&len.to_le_bytes()).await?;
    stream.write_all(&enc_frame).await?;

    Ok(())
}

// pub fn register_plugin_backend_receiver()