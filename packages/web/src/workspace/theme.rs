//! Persistence and application of the page theme (browser only; call from effects).
//!
//! The theme itself is a setting of Salus (the server decides). The last one is remembered in the browser only so
//! that the next page load starts with the right colors instead of flashing the default.

const STORAGE_KEY: &str = "salus-theme";

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// The theme shown on the previous visit, if any.
pub fn load_stored_theme() -> Option<String> {
    storage()?.get_item(STORAGE_KEY).ok().flatten().filter(|theme| !theme.is_empty())
}

/// Sets `data-theme` on the page's `<html>` and remembers the theme.
pub fn apply_page_theme(theme: &str) {
    if let Some(root) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) {
        let _ = root.set_attribute("data-theme", theme);
    }
    if let Some(storage) = storage() {
        let _ = storage.set_item(STORAGE_KEY, theme);
    }
}
