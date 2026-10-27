//! `salus://` control protocol: meta container codec, backend requests
//! (`http/open`, `http/close`) and framework -> backend events.

use crate::http_gateway;
use crate::message_frame::{PluginMessageFrame, NO_FRONTEND};
use crate::plugin_message_router;
use crate::plugin_process_manager;
use dioxus::logger::tracing::warn;
use serde_json::json;

pub const SALUS_PREFIX: &str = "salus://";

pub use crate::message_frame::{pack_meta, unpack_meta, Meta};

/// Handles a (reassembled) `salus://<topic>` request from a backend and always replies
/// as long as the request carries an `id`.
pub async fn handle_backend_request(be_pid: u32, frame: PluginMessageFrame) {
    let topic = frame
        .logical_channel
        .strip_prefix(SALUS_PREFIX)
        .unwrap_or_default()
        .to_string();

    let meta = match unpack_meta(&frame.payload) {
        Ok((meta, _body)) => meta,
        Err(e) => {
            warn!("Backend [{be_pid}]: malformed salus:// request on '{}': {e}", frame.logical_channel);
            return;
        }
    };

    let Some(id) = meta.get("id").filter(|id| id.is_number()).cloned() else {
        warn!("Backend [{be_pid}]: salus:// request without numeric id on '{}'", frame.logical_channel);
        return;
    };

    let result: Result<Option<serde_json::Value>, String> = match topic.as_str() {
        "http/open" => http_gateway::open_route(be_pid, &meta).map(|()| None),
        "http/close" => http_gateway::close_route(be_pid, &meta).map(|()| None),
        "config/get" => config_request(be_pid, None).await.map(Some),
        "config/set" => config_request(be_pid, Some(&meta)).await.map(Some),
        _ => Err("unknown topic".to_string()),
    };

    let reply = match result {
        Ok(None) => json!({ "id": id, "ok": true }),
        Ok(Some(data)) => json!({ "id": id, "ok": true, "data": data }),
        Err(error) => json!({ "id": id, "ok": false, "error": error }),
    };

    if let Err(e) = plugin_message_router::send_to_backend(
        be_pid,
        NO_FRONTEND,
        &frame.logical_channel,
        pack_meta(&reply, &[]),
    )
    .await
    {
        warn!("Backend [{be_pid}]: failed to send reply for '{}': {e}", frame.logical_channel);
    }
}

/// `config/get` and `config/set`: the backend's own settings. `set` takes the changes as `values` in the meta
/// (a value sets the setting, `null` resets it). Both reply with the values in effect.
async fn config_request(be_pid: u32, set: Option<&Meta>) -> Result<serde_json::Value, String> {
    let plugin = plugin_process_manager::get_plugin_id_for_be_pid(be_pid)
        .and_then(|plugin_id| crate::plugin_library::get_plugin_from_id(&plugin_id))
        .ok_or("unknown plugin")?;
    let updates = match set {
        None => None,
        Some(meta) => match meta.get("values") {
            Some(serde_json::Value::Object(values)) => Some(values.clone()),
            _ => return Err("'values' must be an object".to_string()),
        },
    };
    let result = tokio::task::spawn_blocking(move || match updates {
        None => crate::config_loader::plugin_values(&plugin),
        Some(updates) => crate::config_loader::update_plugin_values(&plugin, &updates),
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map(|values| json!({ "values": values })).map_err(|e| e.to_string())
}

pub async fn send_frontend_attached(be_pid: u32, fe_pid: u32) {
    let channel = format!("{SALUS_PREFIX}frontend/attached");
    if let Err(e) = plugin_message_router::send_to_backend(
        be_pid,
        fe_pid,
        &channel,
        // The backend learns which of the plugin's frontend components attached.
        pack_meta(&json!({ "component": plugin_process_manager::get_frontend_component(fe_pid).unwrap_or_default() }), &[]),
    )
    .await
    {
        warn!("Backend [{be_pid}]: failed to send attached event for frontend {fe_pid}: {e}");
    }
}

pub async fn send_frontend_detached(be_pid: u32, fe_pid: u32, component: &str) {
    let channel = format!("{SALUS_PREFIX}frontend/detached");
    if let Err(e) = plugin_message_router::send_to_backend(
        be_pid,
        fe_pid,
        &channel,
        pack_meta(&json!({ "component": component }), &[]),
    )
    .await
    {
        warn!("Backend [{be_pid}]: failed to send detached event for frontend {fe_pid}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_meta_is_two_bytes_of_json() {
        assert_eq!(pack_meta(&json!({}), &[]), vec![2, 0, 0, 0, b'{', b'}']);
    }

    #[test]
    fn meta_roundtrip_keeps_body() {
        let packed = pack_meta(&json!({"id": 7}), b"hello");
        let (meta, body) = unpack_meta(&packed).unwrap();
        assert_eq!(meta["id"], 7);
        assert_eq!(body, b"hello");
    }

    #[test]
    fn malformed_meta_is_rejected() {
        assert!(unpack_meta(&[1, 0]).is_err());
        assert!(unpack_meta(&[9, 0, 0, 0, b'{']).is_err());
        assert!(unpack_meta(&pack_meta(&json!([1, 2]), &[])).is_err());
    }
}