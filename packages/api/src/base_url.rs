//! The URL prefix Salus is served under (for example behind JupyterHub: `/user/me/salus`).
//!
//! There is no Salus setting for it: this reads Dioxus' own base path (`dx serve --base-path`,
//! `[web.app] base_path`), the same one Dioxus uses to nest the router, to prefix `asset!()` URLs and
//! to build server function URLs. Both server and browser see the same value, so any URL written
//! by hand goes through [`with_base`] and stays consistent with those.

/// The prefix without trailing slash, e.g. `/user/me/salus`, or `""` when served at the root.
pub fn base_path() -> String {
    match dioxus::cli_config::base_path() {
        Some(base) => {
            let base = base.trim_matches('/');
            if base.is_empty() { String::new() } else { format!("/{base}") }
        }
        None => String::new(),
    }
}

/// An absolute path of this application (starting with `/`) with the prefix put in front.
pub fn with_base(path: &str) -> String {
    format!("{}/{}", base_path(), path.trim_start_matches('/'))
}
