//! The Salus page's side of plugin communication: relays messages between plugin iframes and
//! the server, answers `salus://` control requests from plugin frontends, routes messages
//! between components of one plugin without involving the server, and applies the theme.

use api::framework_web_api;
use api::message_frame::{pack_meta, parse_peer_channel, unpack_meta, PluginMessageFrame, PEER_PREFIX};
use api::models::{FileViewer, SpawnedComponent};
use dioxus::logger::tracing;
use dioxus::prelude::*;
use futures::channel::mpsc::unbounded;
use futures::channel::oneshot;
use futures::{SinkExt, StreamExt};
use gloo_net::websocket::{futures::WebSocket, Message};
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use wasm_bindgen::prelude::*;
use web_sys::{HtmlIFrameElement, MessageEvent};

const SALUS_PREFIX: &str = "salus://";

type ComponentTabOpener = Rc<dyn Fn(SpawnedComponent)>;
type ViewerPicker = Rc<dyn Fn(String, Vec<FileViewer>, oneshot::Sender<Option<FileViewer>>)>;

thread_local! {
    // Set by the workspace: shows (or focuses) the tab of a component Salus opened.
    static COMPONENT_TAB_OPENER: RefCell<Option<ComponentTabOpener>> = const { RefCell::new(None) };
    // Set by the workspace: lets the user choose between several viewers for a file.
    static VIEWER_PICKER: RefCell<Option<ViewerPicker>> = const { RefCell::new(None) };
    // The plugin frontends currently shown in this page: frontend id -> (plugin id, component name).
    static FRONTENDS: RefCell<HashMap<u32, (u32, String)>> = RefCell::new(HashMap::new());
    // Identifies this browser page towards the server (see `page_id`).
    static PAGE_ID: String = new_page_id();
}

fn new_page_id() -> String {
    use web_sys::js_sys::{Date, Math};
    format!("{:x}{:x}", Date::now() as u64, (Math::random() * 1e15) as u64)
}

/// Random id of this page load. It is sent with the websocket and with every request to open a plugin frontend,
/// so the server can close the frontends (and end `panel` backends) when the page goes away.
pub fn page_id() -> String {
    PAGE_ID.with(|id| id.clone())
}

pub fn set_component_tab_opener(opener: impl Fn(SpawnedComponent) + 'static) {
    COMPONENT_TAB_OPENER.with(|slot| *slot.borrow_mut() = Some(Rc::new(opener)));
}

pub fn set_viewer_picker(picker: impl Fn(String, Vec<FileViewer>, oneshot::Sender<Option<FileViewer>>) + 'static) {
    VIEWER_PICKER.with(|slot| *slot.borrow_mut() = Some(Rc::new(picker)));
}

/// Called when a plugin iframe is created, so the page knows which plugin and component it is.
pub fn register_frontend(frontend_process_id: u32, plugin_id: u32, component: String) {
    FRONTENDS.with(|map| map.borrow_mut().insert(frontend_process_id, (plugin_id, component)));
}

pub fn unregister_frontend(frontend_process_id: u32) {
    FRONTENDS.with(|map| map.borrow_mut().remove(&frontend_process_id));
}

// === Message definition
pub fn init_plugin_bridge() {
    let (tx, rx) = unbounded::<PluginMessageFrame>();
    build_message_from_plugin_receiver(tx);
    run_socket_to_backend(rx);
}

// === Communication between plugin and Salus FE
fn build_message_from_plugin_receiver(plugin_salus_stream: UnboundedSender<PluginMessageFrame>) {
    let closure = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        // Get message data
        let data = event.data();
        let frame: PluginMessageFrame = match serde_wasm_bindgen::from_value(data.clone()) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to deserialize plugin message: {e:?}, raw data: {:?}", data);
                return;
            }
        };

        // A frame may only come from the iframe of the frontend it claims to be from.
        if !event_comes_from_frontend(&event, frame.frontend_process_id) {
            warn!("Dropping message claiming to be frontend {} from a different window", frame.frontend_process_id);
            return;
        }

        let _ = plugin_salus_stream.unbounded_send(frame);
    });

    web_sys::window().unwrap().add_event_listener_with_callback(
        "message", closure.as_ref().unchecked_ref()).unwrap();
    closure.forget();
}

fn plugin_iframe(frontend_process_id: u32) -> Option<HtmlIFrameElement> {
    let document = web_sys::window()?.document()?;
    document.get_element_by_id(&frontend_process_id.to_string())?.dyn_into::<HtmlIFrameElement>().ok()
}

fn event_comes_from_frontend(event: &MessageEvent, frontend_process_id: u32) -> bool {
    let (Some(source), Some(iframe)) = (event.source(), plugin_iframe(frontend_process_id)) else {
        return false;
    };
    let Some(content_window) = iframe.content_window() else {
        return false;
    };
    JsValue::from(content_window) == JsValue::from(source)
}

async fn send_message_to_plugin(message_frame: PluginMessageFrame) {
    let ser_message = serde_wasm_bindgen::to_value(&message_frame).unwrap();

    let Some(iframe) = plugin_iframe(message_frame.frontend_process_id) else {
        warn!("no element with id={}", message_frame.frontend_process_id);
        return;
    };
    let Some(content_window) = iframe.content_window() else {
        return;
    };

    if let Err(e) = content_window.post_message(&ser_message, "*") {
        error!("post_message failed: {e:?}");
    }
}

/// Messages between two components of the same plugin never leave the browser: the page delivers
/// them itself. Returns the frame back if it has to go to the server (anything else).
fn route_between_components(frame: PluginMessageFrame) -> Option<PluginMessageFrame> {
    let sender = frame.frontend_process_id;
    let Some((target, name)) = parse_peer_channel(&frame.logical_channel) else {
        return Some(frame);
    };
    let name = name.to_string();

    let same_plugin = FRONTENDS.with(|map| {
        let map = map.borrow();
        match (map.get(&sender), map.get(&target)) {
            (Some((a, _)), Some((b, _))) => a == b && sender != target,
            _ => false,
        }
    });
    if !same_plugin {
        return Some(frame);
    }

    spawn(send_message_to_plugin(PluginMessageFrame {
        frontend_process_id: target,
        logical_channel: format!("{PEER_PREFIX}{sender}/{name}"),
        ..frame
    }));
    None
}

// === Forwarding to backend and directly
fn run_socket_to_backend(mut plugin_salus_stream: UnboundedReceiver<PluginMessageFrame>) {
    let ws = WebSocket::open(&format!("/plugin-stream?page={}", page_id())).unwrap();
    let (mut write, mut read) = ws.split();

    // Encodes message from JS to byte-stream and forwards to backend
    let send_task = async move {
        while let Some(frame) = plugin_salus_stream.next().await {
            if frame.logical_channel.starts_with(SALUS_PREFIX) {
                spawn(handle_control_request(frame));
                continue;
            }
            let Some(frame) = route_between_components(frame) else {
                continue;
            };

            let enc_frame = frame.encode().unwrap();
            write.send(Message::Bytes(enc_frame)).await.unwrap();
        }
    };
    spawn(send_task);

    // Receives a message from backend and sends it to plugin frontend
    let recv_task = async move {
        while let Some(Ok(Message::Bytes(frame))) = read.next().await {
            let dec_frame = PluginMessageFrame::decode(&frame).unwrap();
            spawn(send_message_to_plugin(dec_frame));
        }
    };
    spawn(recv_task);
}

// === Control requests (salus://...) from plugin frontends to the framework
fn next_message_id() -> u32 {
    static NEXT_MESSAGE_ID: AtomicU32 = AtomicU32::new(1);
    NEXT_MESSAGE_ID.fetch_add(1, Ordering::SeqCst)
}

/// Answers a `salus://<topic>` request on the same channel with `{id, ok, error?, ...}`.
async fn handle_control_request(frame: PluginMessageFrame) {
    let requester = frame.frontend_process_id;

    let meta = match unpack_meta(&frame.payload) {
        Ok((meta, _body)) => meta,
        Err(e) => {
            warn!("Malformed control request from frontend {requester}: {e}");
            return;
        }
    };
    let Some(id) = meta.get("id").filter(|id| id.is_number()).cloned() else {
        warn!("Control request from frontend {requester} without numeric id");
        return;
    };

    let topic = frame.logical_channel.strip_prefix(SALUS_PREFIX).unwrap_or_default();
    let result = match topic {
        "dependency/open" => open_component(requester, &meta, true).await,
        "component/open" => open_component(requester, &meta, false).await,
        "component/list" => Ok(list_components(requester, &meta)),
        "file/open" => open_file(&meta).await,
        _ => Err("unknown topic".to_string()),
    };

    let reply = match result {
        Ok(mut fields) => {
            fields.insert("id".to_string(), id);
            fields.insert("ok".to_string(), Value::Bool(true));
            Value::Object(fields)
        }
        Err(error) => json!({ "id": id, "ok": false, "error": error }),
    };

    send_message_to_plugin(PluginMessageFrame {
        frontend_process_id: requester,
        message_id: next_message_id(),
        flags: 0,
        logical_channel: frame.logical_channel.clone(),
        payload: pack_meta(&reply, &[]),
    })
    .await;
}

/// Shows (or focuses) the tab of a component and describes it for the requester.
fn present(spawned: SpawnedComponent) -> Result<Map<String, Value>, String> {
    let opener = COMPONENT_TAB_OPENER
        .with(|slot| slot.borrow().clone())
        .ok_or("the workspace cannot show plugins")?;
    opener(spawned.clone());

    let mut fields = Map::new();
    fields.insert("frontend_id".to_string(), json!(spawned.frontend_process_id));
    fields.insert("plugin_id".to_string(), json!(spawned.plugin_id));
    fields.insert("component".to_string(), json!(spawned.component_name));
    fields.insert("reused".to_string(), json!(spawned.reused));
    Ok(fields)
}

/// `salus://dependency/open {plugin, component?, params?, reuse?}` and
/// `salus://component/open {component, params?, reuse?}`.
async fn open_component(requester: u32, meta: &Map<String, Value>, dependency: bool) -> Result<Map<String, Value>, String> {
    let text = |key: &str| meta.get(key).and_then(Value::as_str).map(str::to_string);
    let plugin = if dependency { Some(text("plugin").ok_or("missing 'plugin'")?) } else { None };
    let component = text("component");
    if !dependency && component.is_none() {
        return Err("missing 'component'".to_string());
    }
    let params = match meta.get("params") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.to_string()),
    };
    let reuse = meta.get("reuse").and_then(Value::as_bool).unwrap_or(false);

    let spawned = framework_web_api::open_frontend_component(requester, plugin, component, params, reuse, Some(page_id()))
        .await
        .map_err(|e| e.to_string())?;
    present(spawned)
}

/// `salus://component/list {component?}`: the other components of the requester's plugin that are open.
fn list_components(requester: u32, meta: &Map<String, Value>) -> Map<String, Value> {
    let wanted = meta.get("component").and_then(Value::as_str).map(str::to_string);
    let components: Vec<Value> = FRONTENDS.with(|map| {
        let map = map.borrow();
        let Some((plugin_id, _)) = map.get(&requester) else {
            return Vec::new();
        };
        let mut found: Vec<(u32, String)> = map.iter()
            .filter(|(id, (plugin, component))| **id != requester && plugin == plugin_id && wanted.as_ref().is_none_or(|w| w == component))
            .map(|(id, (_, component))| (*id, component.clone()))
            .collect();
        found.sort();
        found.into_iter().map(|(id, component)| json!({ "frontend_id": id, "component": component })).collect()
    });

    let mut fields = Map::new();
    fields.insert("components".to_string(), Value::Array(components));
    fields
}

/// `salus://file/open {file}`: opens the file in a viewer; asks the user if there are several.
async fn open_file(meta: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    let file = meta.get("file").and_then(Value::as_str).ok_or("missing 'file'")?.to_string();
    let extension = file.rsplit_once('.').map(|(_, ext)| ext.to_lowercase()).unwrap_or_default();

    let mut viewers = framework_web_api::get_file_viewers(extension.clone()).await.map_err(|e| e.to_string())?;
    let viewer = match viewers.len() {
        0 => return Err(format!("no viewer is installed for '.{extension}' files")),
        1 => viewers.remove(0),
        _ => {
            let picker = VIEWER_PICKER.with(|slot| slot.borrow().clone()).ok_or("the workspace cannot ask which viewer to use")?;
            let (reply, choice) = oneshot::channel();
            picker(file.clone(), viewers, reply);
            choice.await.ok().flatten().ok_or("cancelled")?
        }
    };

    let spawned = framework_web_api::open_file_viewer(viewer.plugin_id, viewer.component_name, file, Some(page_id()))
        .await
        .map_err(|e| e.to_string())?;
    present(spawned)
}

// === Theme
fn apply_theme_to_frame(frontend_process_id: u32, theme: &str) {
    let Some(frame_document) = plugin_iframe(frontend_process_id).and_then(|iframe| iframe.content_document()) else {
        return;
    };
    if let Some(root) = frame_document.document_element() {
        let _ = root.set_attribute("data-theme", theme);
    }
    if frame_document.get_element_by_id("salus-theme-link").is_none() {
        if let (Ok(link), Some(head)) = (frame_document.create_element("link"), frame_document.head()) {
            link.set_id("salus-theme-link");
            let _ = link.set_attribute("rel", "stylesheet");
            let _ = link.set_attribute("href", "/salus/theme.css");
            let _ = head.append_child(&link);
        }
    }
}

/// Injects the theme stylesheet into one plugin page and sets its theme (when it has loaded).
pub fn apply_theme(frontend_process_id: u32, theme: &str) {
    apply_theme_to_frame(frontend_process_id, theme);
}

/// Switches the theme of every plugin page currently shown.
pub fn apply_theme_to_all_frames(theme: &str) {
    let ids: Vec<u32> = FRONTENDS.with(|map| map.borrow().keys().copied().collect());
    for id in ids {
        apply_theme_to_frame(id, theme);
    }
}
