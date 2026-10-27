//! This crate contains all shared fullstack server functions.

#[cfg(feature = "server")]
mod config_loader;

#[cfg(feature = "server")]
mod plugin_loader;
pub mod models;
pub mod configurable;
pub mod base_url;

#[cfg(feature = "server")]
mod plugin_library;

#[cfg(feature = "server")]
pub mod plugin_process_manager;

#[cfg(feature = "server")]
mod asset_server;

#[cfg(feature = "server")]
mod session_manager;

#[cfg(feature = "server")]
mod user_manager;

#[cfg(feature = "server")]
mod plugin_message_router;

#[cfg(feature = "server")]
mod salus_control;

#[cfg(feature = "server")]
mod browser_connections;

#[cfg(feature = "server")]
mod http_gateway;

#[cfg(feature = "server")]
mod plugin_config_api;

#[cfg(feature = "server")]
mod settings_api;

pub mod framework_web_api;

#[cfg(feature = "server")]
mod plugin_backend_executor;
pub mod message_frame;

use dioxus::prelude::*;

#[cfg(feature = "server")]
use axum::routing;
#[cfg(feature = "server")]
use dioxus::server::DioxusRouterExt;

#[cfg(feature = "server")]
pub fn run_server(app: fn() -> Element) {
    dioxus::serve(move || async move {
        config_loader::init();
        plugin_library::init();

        // Build protected plugin frontend asset fetching
        let protected_pfe_asset_fetcher = axum::Router::new().route("/{*path}", routing::get(asset_server::serve_plugin_frontend_assets));

        // Plugin frontends reach their backend's dynamically opened HTTP routes through here.
        let plugin_api = axum::Router::new()
            .route("/plugin-api/{fe_pid}", routing::any(http_gateway::handle_root))
            .route("/plugin-api/{fe_pid}/{*path}", routing::any(http_gateway::handle))
            .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024));

        // Plugin frontends read and change their plugin's settings through here; the settings plugin reaches everyone's.
        let plugin_config = axum::Router::new()
            .route("/plugin-config/{fe_pid}", routing::any(plugin_config_api::handle))
            .route("/settings-api/{fe_pid}/{*path}", routing::any(settings_api::handle))
            .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024));

        // Our routes are added to the Dioxus application *before* it is put under the base path, so that
        // they live under the same prefix as the assets and the server functions.
        let app_router = axum::Router::new()
            .route("/assets/theme.css", routing::get(theme_css))
            .route("/assets/icon.svg", routing::get(|| async { icon("image/svg+xml", include_bytes!("../../web/assets/icons/icon.svg")) }))
            .route("/assets/icon-32.png", routing::get(|| async { icon("image/png", include_bytes!("../../web/assets/icons/icon-32.png")) }))
            .route("/assets/icon-180.png", routing::get(|| async { icon("image/png", include_bytes!("../../web/assets/icons/icon-180.png")) }))
            .route("/assets/icon-512.png", routing::get(|| async { icon("image/png", include_bytes!("../../web/assets/icons/icon-512.png")) }))
            .nest("/plugins", protected_pfe_asset_fetcher)
            .merge(plugin_api)
            .merge(plugin_config)
            .route("/plugin-stream", routing::get(browser_connections::plugin_stream))
            .route("/dev/login", routing::get(session_manager::dev_login)) // TODO remove the dev/login
            .serve_dioxus_application(dioxus::server::ServeConfig::new(), app)
            // Whoever opens Salus without a session goes to the login page first.
            .layer(axum::middleware::from_fn(session_manager::require_session));
        let router = under_base_path(app_router);

        Ok(router)
    });
}

/// Serves the router under Dioxus' base path (`dx serve --base-path`), the way `dioxus::server::router`
/// does for the application alone.
#[cfg(feature = "server")]
fn under_base_path(router: axum::Router) -> axum::Router {
    let base = base_url::base_path();
    if base.is_empty() {
        return router;
    }
    // Axum serves a nested router at `/base/` only, so `/base` redirects there (relative, so a proxy prefix is kept).
    let last = base.rsplit('/').next().unwrap_or_default().to_string();
    axum::Router::new()
        .nest(&format!("{base}/"), router)
        .route(&base, routing::get(move || async move { axum::response::Redirect::to(&format!("{last}/")) }))
}

/// The app icon, in the sizes the page and the login page link to. Served by the server itself (not bundled by `dx`), so the URLs are
/// fixed and the login page, which is not a Dioxus page, can use them too.
#[cfg(feature = "server")]
fn icon(content_type: &'static str, bytes: &'static [u8]) -> impl axum::response::IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, content_type),
            (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    )
}

/// The theme stylesheet Salus injects into plugin frontends (and uses for its own workspace).
#[cfg(feature = "server")]
async fn theme_css() -> impl axum::response::IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (axum::http::header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("../../web/assets/theme.css"),
    )
}
