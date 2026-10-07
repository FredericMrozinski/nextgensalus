//! The websocket connections of browser pages (`/plugin-stream`).
//!
//! Every page registers its connection under the session's user. A frame for a frontend is
//! delivered to the connections of the user that owns the frontend; the page's relay then picks
//! the iframe by id. Also routes `peer://` messages between frontends that are related as
//! parent and dependency.

use crate::message_frame::{parse_peer_channel, PluginMessageFrame, PEER_PREFIX};
use crate::plugin_message_router;
use crate::plugin_process_manager;
use crate::session_manager;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use dioxus::logger::tracing::{debug, warn};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

struct BrowserConnection {
    user_id: u32,
    tx: UnboundedSender<Vec<u8>>,
}

fn connections() -> &'static Mutex<HashMap<u64, BrowserConnection>> {
    static CONNECTIONS: OnceLock<Mutex<HashMap<u64, BrowserConnection>>> = OnceLock::new();
    CONNECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register(user_id: u32) -> (u64, UnboundedReceiver<Vec<u8>>) {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = unbounded_channel();
    connections().lock().unwrap().insert(id, BrowserConnection { user_id, tx });
    (id, rx)
}

fn unregister(id: u64) {
    connections().lock().unwrap().remove(&id);
}

/// Sends an encoded frame to every connection of `user_id`. Returns whether any took it.
fn deliver_bytes(user_id: u32, bytes: Vec<u8>) -> bool {
    let connections = connections().lock().unwrap();
    let mut delivered = false;
    for connection in connections.values().filter(|c| c.user_id == user_id) {
        delivered |= connection.tx.send(bytes.clone()).is_ok();
    }
    delivered
}

/// Delivers a frame (addressed by its `frontend_process_id`) to the browser pages of the
/// frontend's owner.
pub fn deliver_to_frontend(frame: &PluginMessageFrame) -> bool {
    let Some(owner) = plugin_process_manager::get_frontend_owner(frame.frontend_process_id) else {
        warn!("Dropping frame for unknown frontend {}", frame.frontend_process_id);
        return false;
    };
    let bytes = match frame.encode() {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!("Dropping frame for frontend {}: {e}", frame.frontend_process_id);
            return false;
        }
    };
    let delivered = deliver_bytes(owner.user_id, bytes);
    if !delivered {
        debug!("No open browser page for frontend {}; frame dropped", frame.frontend_process_id);
    }
    delivered
}

/// Forwards a frontend's `peer://<target>/<name>` message to the target, rewritten to
/// `peer://<sender>/<name>` so the receiver sees who it came from.
fn route_peer_message(frame: PluginMessageFrame) -> Result<(), String> {
    let sender = frame.frontend_process_id;
    let (target, name) = parse_peer_channel(&frame.logical_channel)
        .ok_or_else(|| format!("malformed peer channel '{}'", frame.logical_channel))?;
    if !plugin_process_manager::are_related_frontends(sender, target) {
        return Err(format!("frontend {sender} may not message frontend {target}"));
    }

    let delivered = PluginMessageFrame {
        frontend_process_id: target,
        logical_channel: format!("{PEER_PREFIX}{sender}/{name}"),
        ..frame
    };
    deliver_to_frontend(&delivered);
    Ok(())
}

async fn handle_browser_frame(user_id: u32, bytes: &[u8]) {
    let frame = match PluginMessageFrame::decode(bytes) {
        Ok(frame) => frame,
        Err(e) => {
            warn!("Dropping invalid frame from browser: {e}");
            return;
        }
    };

    // A page may only speak for the frontends of its own user.
    match plugin_process_manager::get_frontend_owner(frame.frontend_process_id) {
        Some(owner) if owner.user_id == user_id => {}
        _ => {
            warn!("Dropping frame from browser for foreign or unknown frontend {}", frame.frontend_process_id);
            return;
        }
    }

    if frame.logical_channel.starts_with(PEER_PREFIX) {
        if let Err(e) = route_peer_message(frame) {
            warn!("Dropping peer message: {e}");
        }
    } else if frame.logical_channel.starts_with("salus://") {
        // Control requests are answered by the page's relay; they never reach the backend.
        warn!("Dropping salus:// frame that reached the server: '{}'", frame.logical_channel);
    } else if let Err(e) = plugin_message_router::forward_message_to_backend(frame).await {
        warn!("Could not forward frame to backend: {e}");
    }
}

#[derive(serde::Deserialize)]
pub struct PageQuery {
    /// Identifies the browser page; the frontends it opened are closed when this connection ends.
    page: Option<String>,
}

pub async fn plugin_stream(jar: CookieJar, Query(query): Query<PageQuery>, ws: WebSocketUpgrade) -> Response {
    let Some(user_id) = session_manager::user_id_from_jar(&jar) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    ws.on_upgrade(move |socket| serve_socket(user_id, query.page, socket))
}

async fn serve_socket(user_id: u32, page_id: Option<String>, socket: WebSocket) {
    let (mut writer, mut reader) = socket.split();
    let (connection_id, mut outbox) = register(user_id);

    let write_task = tokio::spawn(async move {
        while let Some(bytes) = outbox.recv().await {
            if writer.send(Message::Binary(bytes.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = reader.next().await {
        match message {
            Message::Binary(bytes) => handle_browser_frame(user_id, &bytes).await,
            Message::Close(_) => break,
            _ => {}
        }
    }

    unregister(connection_id);
    write_task.abort();

    // The page is gone, so are the plugin frontends it showed: close them (which may end `panel` backends).
    if let Some(page_id) = page_id {
        plugin_process_manager::close_page_frontends(&page_id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_process_manager::insert_test_frontend;

    #[test]
    fn peer_channels_parse() {
        assert_eq!(parse_peer_channel("peer://30001/view"), Some((30001, "view")));
        assert_eq!(parse_peer_channel("peer://30001/a/b"), Some((30001, "a/b")));
        assert_eq!(parse_peer_channel("peer://30001/"), None);
        assert_eq!(parse_peer_channel("peer://abc/view"), None);
        assert_eq!(parse_peer_channel("peer://30001"), None);
        assert_eq!(parse_peer_channel("ws://view"), None);
    }

    #[test]
    fn frames_reach_only_the_owners_connections() {
        insert_test_frontend(991_001, 1, 1, 501, None);
        let (mine, mut mine_rx) = register(501);
        let (_other, mut other_rx) = register(502);

        let frame = PluginMessageFrame {
            frontend_process_id: 991_001,
            message_id: 1,
            flags: 0,
            logical_channel: "ws://x".into(),
            payload: vec![1],
        };
        assert!(deliver_to_frontend(&frame));
        assert_eq!(PluginMessageFrame::decode(&mine_rx.try_recv().unwrap()).unwrap(), frame);
        assert!(other_rx.try_recv().is_err());

        unregister(mine);
        assert!(!deliver_to_frontend(&frame));
    }

    #[test]
    fn peer_messages_are_rewritten_and_only_routed_between_related_frontends() {
        insert_test_frontend(991_101, 1, 1, 503, None);
        insert_test_frontend(991_102, 2, 2, 503, Some(991_101));
        insert_test_frontend(991_103, 2, 2, 503, None);
        let (conn, mut rx) = register(503);

        let frame = |sender: u32, channel: &str| PluginMessageFrame {
            frontend_process_id: sender,
            message_id: 4,
            flags: 0,
            logical_channel: channel.into(),
            payload: b"zoom".to_vec(),
        };

        // parent -> child
        route_peer_message(frame(991_101, "peer://991102/view")).unwrap();
        let delivered = PluginMessageFrame::decode(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(delivered.frontend_process_id, 991_102);
        assert_eq!(delivered.logical_channel, "peer://991101/view");
        assert_eq!(delivered.payload, b"zoom");
        assert_eq!(delivered.message_id, 4);

        // child -> parent
        route_peer_message(frame(991_102, "peer://991101/view")).unwrap();
        assert_eq!(PluginMessageFrame::decode(&rx.try_recv().unwrap()).unwrap().frontend_process_id, 991_101);

        // unrelated frontends and malformed channels are refused
        assert!(route_peer_message(frame(991_103, "peer://991101/view")).is_err());
        assert!(route_peer_message(frame(991_101, "peer://991102")).is_err());
        assert!(rx.try_recv().is_err());

        unregister(conn);
    }
}
