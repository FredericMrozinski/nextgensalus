use dioxus::prelude::*;
use dioxus::document;
use serde::{Serialize, Deserialize};
use wasm_bindgen::prelude::*;
use web_sys::MessageEvent;
use dioxus::logger::tracing;
use gloo_net::websocket::{futures::WebSocket, Message};
use futures::{StreamExt, SinkExt};
use futures::channel::mpsc::unbounded;
use postcard::{from_bytes, to_allocvec};
use web_sys::HtmlIFrameElement;
use api::message_frame::PluginMessageFrame;

// === Message definition
pub fn init_plugin_bridge() {
    let (tx, rx) = unbounded::<PluginMessageFrame>();
    build_message_from_plugin_receiver(tx);
    run_socket_to_backend(rx);
}

// === Communication between plugin and Salus FE
fn build_message_from_plugin_receiver(plugin_salus_stream:
                                      UnboundedSender<PluginMessageFrame>) {
    let closure = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {

        // TODO Check origin
        let origin = event.origin();
        debug!("{}", &origin);

        // Get message data
        let data = event.data();
        let frame: PluginMessageFrame = match serde_wasm_bindgen::from_value(data.clone()) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to deserialize plugin message: {e:?}, raw data: {:?}", data);
                return;
            }
        };

        let _ = plugin_salus_stream.unbounded_send(frame);
    });

    web_sys::window().unwrap().add_event_listener_with_callback(
        "message", closure.as_ref().unchecked_ref()).unwrap();
    closure.forget();
}

async fn send_message_to_plugin(message_frame: PluginMessageFrame) {

    let ser_message = serde_wasm_bindgen::to_value(&message_frame).unwrap();
    let document = web_sys::window().unwrap().document().unwrap();

    let Some(element) = document.get_element_by_id(&message_frame.frontend_process_id.to_string()) else {
        warn!("no element with id={}", message_frame.frontend_process_id);
        return;
    };

    let iframe: HtmlIFrameElement = element.dyn_into().unwrap();

    let Some(content_window) = iframe.content_window() else {
        return;
    };

    if let Err(e) = content_window.post_message(&ser_message, "*") {
        error!("post_message failed: {e:?}");
    }
}

// === Forwarding to backend and directly
fn run_socket_to_backend(mut plugin_salus_stream: UnboundedReceiver<PluginMessageFrame>) {
    let ws = WebSocket::open("/plugin-stream").unwrap();
    let (mut write, mut read) = ws.split();

    // Encodes message from JS to byte-stream and forwards to backend
    let send_task = async move {
        while let Some(frame) = plugin_salus_stream.next().await {
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

// --- REST TODO