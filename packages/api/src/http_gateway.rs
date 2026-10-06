//! HTTP side of the plugin protocol: per-backend route tables opened via
//! `salus://http/open`, forwarding of frontend HTTP requests to the backend over its
//! socket, and completion of the HTTP response from the backend's `http://response`.

use crate::message_frame::PluginMessageFrame;
use crate::plugin_message_router;
use crate::plugin_process_manager;
use crate::salus_control::{self, Meta};
use crate::session_manager;
use axum::body::{Body, Bytes};
use axum::extract::{Path, RawQuery};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use dioxus::logger::tracing::warn;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;

const REQUEST_CHANNEL: &str = "http://request";
const RESPONSE_CHANNEL: &str = "http://response";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const ALLOWED_METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

// Never forward cookies / authorization: plugin backends must not see the session.
const FORWARDED_REQUEST_HEADERS: [&str; 8] = [
    "content-type",
    "accept",
    "accept-language",
    "if-match",
    "if-none-match",
    "if-modified-since",
    "range",
    "x-requested-with",
];

const BLOCKED_RESPONSE_HEADERS: [&str; 9] = [
    "set-cookie",
    "connection",
    "keep-alive",
    "transfer-encoding",
    "upgrade",
    "te",
    "trailer",
    "content-length",
    "proxy-authenticate",
];

// ============================================================================
// Route table (per backend instance)
// ============================================================================

#[derive(Default)]
struct MethodRoutes {
    paths: Vec<String>,
    router: matchit::Router<String>,
}

type RouteTable = HashMap<String, MethodRoutes>;

fn route_tables() -> &'static Mutex<HashMap<u32, RouteTable>> {
    static ROUTES: OnceLock<Mutex<HashMap<u32, RouteTable>>> = OnceLock::new();
    ROUTES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn build_router(paths: &[String]) -> Result<matchit::Router<String>, String> {
    let mut router = matchit::Router::new();
    for path in paths {
        router
            .insert(path.clone(), path.clone())
            .map_err(|e| format!("invalid or conflicting route '{path}': {e}"))?;
    }
    Ok(router)
}

fn parse_method_and_path(meta: &Meta) -> Result<(String, String), String> {
    let method = meta
        .get("method")
        .and_then(|v| v.as_str())
        .ok_or("missing 'method'")?;
    if !ALLOWED_METHODS.contains(&method) {
        return Err(format!("unsupported method '{method}'"));
    }
    let path = meta
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("missing 'path'")?;
    if !path.starts_with('/') {
        return Err("path must start with '/'".into());
    }
    Ok((method.to_string(), path.to_string()))
}

pub fn open_route(be_pid: u32, meta: &Meta) -> Result<(), String> {
    let (method, path) = parse_method_and_path(meta)?;

    let mut tables = route_tables().lock().unwrap();
    let routes = tables.entry(be_pid).or_default().entry(method).or_default();

    if routes.paths.contains(&path) {
        return Err("route already open".into());
    }
    let mut paths = routes.paths.clone();
    paths.push(path);
    routes.router = build_router(&paths)?;
    routes.paths = paths;
    Ok(())
}

pub fn close_route(be_pid: u32, meta: &Meta) -> Result<(), String> {
    let (method, path) = parse_method_and_path(meta)?;

    let mut tables = route_tables().lock().unwrap();
    let routes = tables
        .get_mut(&be_pid)
        .and_then(|table| table.get_mut(&method))
        .ok_or("route not open")?;

    let Some(pos) = routes.paths.iter().position(|p| *p == path) else {
        return Err("route not open".into());
    };
    routes.paths.remove(pos);
    // matchit cannot remove a route, so rebuild from the remaining ones.
    routes.router = build_router(&routes.paths)?;
    Ok(())
}

/// Returns the matched route pattern and its captured parameters.
fn match_route(be_pid: u32, method: &str, path: &str) -> Option<(String, Vec<(String, String)>)> {
    let tables = route_tables().lock().unwrap();
    let routes = tables.get(&be_pid)?.get(method)?;
    let matched = routes.router.at(path).ok()?;
    let params = matched
        .params
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Some((matched.value.clone(), params))
}

// ============================================================================
// Pending requests
// ============================================================================

pub struct HttpReply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

type PendingKey = (u32, u32); // (backend pid, request id)

fn pending_requests() -> &'static Mutex<HashMap<PendingKey, oneshot::Sender<HttpReply>>> {
    static PENDING: OnceLock<Mutex<HashMap<PendingKey, oneshot::Sender<HttpReply>>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_request_id() -> u32 {
    static NEXT_ID: AtomicU32 = AtomicU32::new(1);
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

// Removes the pending entry when the request future ends or is cancelled
// (e.g. the browser closed the connection).
struct PendingGuard(PendingKey);

impl Drop for PendingGuard {
    fn drop(&mut self) {
        pending_requests().lock().unwrap().remove(&self.0);
    }
}

/// Called by the message router for every reassembled `http://` frame from a backend.
pub fn handle_backend_frame(be_pid: u32, frame: PluginMessageFrame) {
    if frame.logical_channel != RESPONSE_CHANNEL {
        warn!("Backend [{be_pid}]: ignoring unexpected http:// channel '{}'", frame.logical_channel);
        return;
    }

    let (meta, body) = match salus_control::unpack_meta(&frame.payload) {
        Ok(parsed) => parsed,
        Err(e) => {
            warn!("Backend [{be_pid}]: malformed http response: {e}");
            return;
        }
    };

    let Some(id) = meta.get("id").and_then(|v| v.as_u64()).and_then(|v| u32::try_from(v).ok()) else {
        warn!("Backend [{be_pid}]: http response without valid id");
        return;
    };
    let Some(status) = meta
        .get("status")
        .and_then(|v| v.as_u64())
        .filter(|s| (100..=599).contains(s))
    else {
        warn!("Backend [{be_pid}]: http response {id} without valid status");
        return;
    };

    let headers = meta
        .get("headers")
        .and_then(|v| v.as_array())
        .map(|pairs| {
            pairs
                .iter()
                .filter_map(|pair| {
                    let pair = pair.as_array()?;
                    Some((pair.first()?.as_str()?.to_string(), pair.get(1)?.as_str()?.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();

    let sender = pending_requests().lock().unwrap().remove(&(be_pid, id));
    match sender {
        Some(sender) => {
            let _ = sender.send(HttpReply { status: status as u16, headers, body: body.to_vec() });
        }
        None => warn!("Backend [{be_pid}]: response for unknown or completed request {id}"),
    }
}

/// Drops all routes of the backend and fails its pending requests (their receivers see 502).
pub fn backend_disconnected(be_pid: u32) {
    route_tables().lock().unwrap().remove(&be_pid);
    pending_requests().lock().unwrap().retain(|(pid, _), _| *pid != be_pid);
}

// ============================================================================
// Forwarding
// ============================================================================

pub async fn request_via_backend(
    be_pid: u32,
    fe_pid: u32,
    method: &str,
    path: &str,
    query_string: &str,
    headers: Vec<(String, String)>,
    body: &[u8],
) -> Result<HttpReply, StatusCode> {
    let (route, params) = match_route(be_pid, method, path).ok_or(StatusCode::NOT_FOUND)?;

    let id = next_request_id();
    let key = (be_pid, id);
    let (tx, rx) = oneshot::channel();
    pending_requests().lock().unwrap().insert(key, tx);
    let _guard = PendingGuard(key);

    let params: HashMap<String, String> = params.into_iter().collect();
    let headers: Vec<[String; 2]> = headers.into_iter().map(|(k, v)| [k, v]).collect();
    let meta = json!({
        "id": id,
        "method": method,
        "path": path,
        "route": route,
        "params": params,
        "query_string": query_string,
        "headers": headers,
    });
    let payload = salus_control::pack_meta(&meta, body);

    plugin_message_router::send_to_backend(be_pid, fe_pid, REQUEST_CHANNEL, payload)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::InvalidData => StatusCode::PAYLOAD_TOO_LARGE,
            _ => StatusCode::BAD_GATEWAY,
        })?;

    match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
        Ok(Ok(reply)) => Ok(reply),
        Ok(Err(_)) => Err(StatusCode::BAD_GATEWAY),
        Err(_) => Err(StatusCode::GATEWAY_TIMEOUT),
    }
}

// ============================================================================
// Axum handlers: /plugin-api/{fe_pid}/{*path}
// ============================================================================

pub async fn handle_root(
    Path(fe_pid): Path<u32>,
    jar: CookieJar,
    method: Method,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    handle_request(fe_pid, String::new(), jar, method, query, headers, body).await
}

pub async fn handle(
    Path((fe_pid, rest)): Path<(u32, String)>,
    jar: CookieJar,
    method: Method,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    handle_request(fe_pid, rest, jar, method, query, headers, body).await
}

async fn handle_request(
    fe_pid: u32,
    rest: String,
    jar: CookieJar,
    method: Method,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Same session check as the asset server, plus: the frontend must belong to this user.
    let Some(session_id) = jar.get("session_id").and_then(|c| c.value().parse::<u32>().ok()) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(user_id) = session_manager::get_user_id_from_session(session_id) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(owner) = plugin_process_manager::get_frontend_owner(fe_pid) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if owner.user_id != user_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(be_pid) = plugin_process_manager::get_be_pid_for_fe_pid(fe_pid) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let path = format!("/{rest}");
    let forwarded_headers: Vec<(String, String)> = headers
        .iter()
        .filter(|(name, _)| FORWARDED_REQUEST_HEADERS.contains(&name.as_str()))
        .filter_map(|(name, value)| Some((name.as_str().to_string(), value.to_str().ok()?.to_string())))
        .collect();

    match request_via_backend(
        be_pid,
        fe_pid,
        method.as_str(),
        &path,
        query.as_deref().unwrap_or(""),
        forwarded_headers,
        &body,
    )
    .await
    {
        Ok(reply) => build_response(reply),
        Err(status) => status.into_response(),
    }
}

fn build_response(reply: HttpReply) -> Response {
    let mut builder = Response::builder()
        .status(reply.status)
        .header("x-content-type-options", "nosniff");

    for (name, value) in &reply.headers {
        if BLOCKED_RESPONSE_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
            continue;
        }
        if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(name.as_bytes()), HeaderValue::from_str(value)) {
            builder = builder.header(name, value);
        }
    }

    builder
        .body(Body::from(reply.body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn meta(method: &str, path: &str) -> Meta {
        match json!({ "method": method, "path": path }) {
            Value::Object(m) => m,
            _ => unreachable!(),
        }
    }

    #[test]
    fn open_match_close() {
        let be = 900_001;
        open_route(be, &meta("GET", "/doc/{id}")).unwrap();

        let (route, params) = match_route(be, "GET", "/doc/42").unwrap();
        assert_eq!(route, "/doc/{id}");
        assert_eq!(params, vec![("id".to_string(), "42".to_string())]);
        assert!(match_route(be, "POST", "/doc/42").is_none());
        assert!(match_route(be, "GET", "/doc").is_none());

        close_route(be, &meta("GET", "/doc/{id}")).unwrap();
        assert!(match_route(be, "GET", "/doc/42").is_none());
        backend_disconnected(be);
    }

    #[test]
    fn literal_beats_parameter() {
        let be = 900_002;
        open_route(be, &meta("GET", "/doc/{id}")).unwrap();
        open_route(be, &meta("GET", "/doc/new")).unwrap();
        assert_eq!(match_route(be, "GET", "/doc/new").unwrap().0, "/doc/new");
        assert_eq!(match_route(be, "GET", "/doc/7").unwrap().0, "/doc/{id}");
        backend_disconnected(be);
    }

    #[test]
    fn duplicates_conflicts_and_bad_input_are_rejected() {
        let be = 900_003;
        open_route(be, &meta("GET", "/doc/{id}")).unwrap();
        assert!(open_route(be, &meta("GET", "/doc/{id}")).is_err());
        assert!(open_route(be, &meta("GET", "/doc/{name}")).is_err());
        assert!(open_route(be, &meta("GET", "no-slash")).is_err());
        assert!(open_route(be, &meta("TRACE", "/x")).is_err());
        assert!(close_route(be, &meta("GET", "/never-opened")).is_err());
        // same pattern for another method is fine
        open_route(be, &meta("POST", "/doc/{id}")).unwrap();
        backend_disconnected(be);
    }

    #[test]
    fn disconnect_drops_routes() {
        let be = 900_004;
        open_route(be, &meta("GET", "/a")).unwrap();
        backend_disconnected(be);
        assert!(match_route(be, "GET", "/a").is_none());
    }

    use crate::message_frame::PluginMessageFrame;
    use crate::plugin_backend_executor::BackendPluginSocketHandle;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    async fn write_frame(stream: &mut UnixStream, fe_pid: u32, channel: &str, payload: Vec<u8>) {
        let body = PluginMessageFrame {
            frontend_process_id: fe_pid,
            message_id: 1,
            flags: 0,
            logical_channel: channel.to_string(),
            payload,
        }
        .encode()
        .unwrap();
        stream.write_all(&(body.len() as u32).to_le_bytes()).await.unwrap();
        stream.write_all(&body).await.unwrap();
    }

    async fn read_frame(stream: &mut UnixStream) -> PluginMessageFrame {
        let mut len = [0u8; 4];
        stream.read_exact(&mut len).await.unwrap();
        let mut body = vec![0u8; u32::from_le_bytes(len) as usize];
        stream.read_exact(&mut body).await.unwrap();
        PluginMessageFrame::decode(&body).unwrap()
    }

    #[tokio::test]
    async fn end_to_end_with_fake_backend() {
        let be_pid = 900_100;
        let fe_pid = 30_001;
        let path = std::env::temp_dir().join("salus-test-900100.sock");
        let handle = BackendPluginSocketHandle::bind(path.clone()).unwrap();
        let _accept = plugin_message_router::establish_backend_streams(be_pid, handle);
        let mut backend = UnixStream::connect(&path).await.unwrap();

        // Backend opens a route and gets an ok reply on the same channel.
        let open = json!({"id": 1, "method": "POST", "path": "/doc/{id}"});
        write_frame(&mut backend, 0, "salus://http/open", salus_control::pack_meta(&open, b"")).await;
        let reply = read_frame(&mut backend).await;
        assert_eq!(reply.logical_channel, "salus://http/open");
        assert_eq!(reply.frontend_process_id, 0);
        let (reply_meta, _) = salus_control::unpack_meta(&reply.payload).unwrap();
        assert_eq!(reply_meta["id"], 1);
        assert_eq!(reply_meta["ok"], true);

        // Unknown topic is answered with ok:false.
        write_frame(&mut backend, 0, "salus://nope", salus_control::pack_meta(&json!({"id": 2}), b"")).await;
        let reply = read_frame(&mut backend).await;
        let (reply_meta, _) = salus_control::unpack_meta(&reply.payload).unwrap();
        assert_eq!(reply_meta["ok"], false);

        // Unopened route -> 404 without touching the backend.
        let not_found = request_via_backend(be_pid, fe_pid, "GET", "/doc/1", "", vec![], b"").await;
        assert_eq!(not_found.err(), Some(StatusCode::NOT_FOUND));

        // Frontend request is forwarded, backend answers, HTTP response completes.
        let request = tokio::spawn(request_via_backend(
            be_pid,
            fe_pid,
            "POST",
            "/doc/42",
            "a=1",
            vec![("content-type".to_string(), "text/plain".to_string())],
            b"hello",
        ));
        let forwarded = read_frame(&mut backend).await;
        assert!(forwarded.logical_channel.starts_with("http://"));
        assert_eq!(forwarded.frontend_process_id, fe_pid);
        let (meta, body) = salus_control::unpack_meta(&forwarded.payload).unwrap();
        assert_eq!(body, b"hello");
        assert_eq!(meta["method"], "POST");
        assert_eq!(meta["path"], "/doc/42");
        assert_eq!(meta["route"], "/doc/{id}");
        assert_eq!(meta["params"]["id"], "42");
        assert_eq!(meta["query_string"], "a=1");
        assert_eq!(meta["headers"][0][0], "content-type");

        let response = json!({"id": meta["id"], "status": 201, "headers": [["content-type", "text/plain"]]});
        write_frame(&mut backend, fe_pid, "http://response", salus_control::pack_meta(&response, b"ok")).await;
        let reply = request.await.unwrap().unwrap();
        assert_eq!(reply.status, 201);
        assert_eq!(reply.body, b"ok");
        assert_eq!(reply.headers, vec![("content-type".to_string(), "text/plain".to_string())]);

        // Backend disappears while a request is pending -> 502, and routes are dropped.
        let pending = tokio::spawn(request_via_backend(be_pid, fe_pid, "POST", "/doc/1", "", vec![], b""));
        let _ = read_frame(&mut backend).await;
        drop(backend);
        assert_eq!(pending.await.unwrap().err(), Some(StatusCode::BAD_GATEWAY));
        assert!(match_route(be_pid, "POST", "/doc/1").is_none());
        let _ = std::fs::remove_file(&path);
    }
}
