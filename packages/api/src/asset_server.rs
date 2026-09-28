use tokio::fs;
use axum::{
    extract::Path,
    response::{IntoResponse, Response},
    http::{StatusCode, header},
};
use axum_extra::extract::cookie::CookieJar;use std::path::{Path as StdPath, PathBuf, Component};
use crate::session_manager;
use crate::user_manager;
use crate::plugin_loader;

pub async fn serve_plugin_frontend_assets(
    Path(requested): Path<String>,
    cookie_jar: CookieJar
) -> Result<Response, StatusCode> {

    // Check if session id exists, if not reject
    let session_id = cookie_jar.get("session_id").map(|c| c.value()).ok_or(StatusCode::UNAUTHORIZED)?;
    let session_id = session_id.parse::<u32>().map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Guard against any relative paths traversals
    let rel_path = StdPath::new(&requested);
    if rel_path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Check if the user has the file access rights
    let user_id = session_manager::get_user_id_from_session(session_id).map_err(|_| StatusCode::UNAUTHORIZED)?;
    let can_access = user_manager::is_user_allowed_to_read(user_id, &requested);
    if !can_access {
        return Err(StatusCode::FORBIDDEN);
    }


    // Build full path
    let abs_path = plugin_loader::get_plugins_root_path().join(rel_path);
    let bytes = fs::read(&abs_path).await.map_err(|_| StatusCode::NOT_FOUND)?;
    let mime = mime_guess::from_path(&abs_path).first_or_octet_stream();

    Ok((
        [
            (header::CONTENT_TYPE, mime.as_ref().to_string()),
            (header::CACHE_CONTROL, "no-store".to_string())
        ],
        bytes
        ).into_response())
}