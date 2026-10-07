//! Persistence and application of the page theme (browser only; call from effects).

use super::state::Theme;

const STORAGE_KEY: &str = "salus-theme";

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// The theme the user chose earlier, if any.
pub fn load_stored_theme() -> Option<Theme> {
    Theme::parse(&storage()?.get_item(STORAGE_KEY).ok().flatten()?)
}

/// Sets `data-theme` on the page's `<html>` and remembers the choice.
pub fn apply_page_theme(theme: Theme) {
    if let Some(root) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) {
        let _ = root.set_attribute("data-theme", theme.as_str());
    }
    if let Some(storage) = storage() {
        let _ = storage.set_item(STORAGE_KEY, theme.as_str());
    }
}
