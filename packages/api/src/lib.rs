//! This crate contains all shared fullstack server functions.

#[cfg(feature = "server")]
mod config_loader;

#[cfg(feature = "server")]
mod plugin_loader;
pub mod models;
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
        plugin_library::init();

        // Build protected plugin frontend asset fetching
        let protected_pfe_asset_fetcher = axum::Router::new().route("/{*path}", routing::get(asset_server::serve_plugin_frontend_assets));

        // Plugin frontends reach their backend's dynamically opened HTTP routes through here.
        let plugin_api = axum::Router::new()
            .route("/plugin-api/{fe_pid}", routing::any(http_gateway::handle_root))
            .route("/plugin-api/{fe_pid}/{*path}", routing::any(http_gateway::handle))
            .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024));

        // Our routes are added to the Dioxus application *before* it is put under the base path, so that
        // they live under the same prefix as the assets and the server functions.
        let app_router = axum::Router::new()
            .route("/assets/theme.css", routing::get(theme_css))
            .nest("/plugins", protected_pfe_asset_fetcher)
            .merge(plugin_api)
            .route("/plugin-stream", routing::get(browser_connections::plugin_stream))
            .route("/dev/login", routing::get(session_manager::dev_login)) // TODO remove the dev/login
            .serve_dioxus_application(dioxus::server::ServeConfig::new(), app);
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
