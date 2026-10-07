//! This crate contains all shared fullstack server functions.

#[cfg(feature = "server")]
mod config_loader;

#[cfg(feature = "server")]
mod plugin_loader;
pub mod models;

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

        let router = dioxus::server::router(app)
            .route("/salus/theme.css", routing::get(theme_css))
            .nest("/plugins", protected_pfe_asset_fetcher)
            .merge(plugin_api)
            .route("/plugin-stream", routing::get(browser_connections::plugin_stream))
            .route("/dev/login", routing::get(session_manager::dev_login)); // TODO remove the dev/login

        Ok(router)
    });
}

/// The theme stylesheet Salus injects into plugin frontends (and uses for its own workspace).
#[cfg(feature = "server")]
async fn theme_css() -> impl axum::response::IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (axum::http::header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("theme.css"),
    )
}
