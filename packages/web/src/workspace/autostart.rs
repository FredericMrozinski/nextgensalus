//! Opening the plugins of the `autostart` setting after a login (browser only; call from effects).
//!
//! The login page sends the browser to `?from=login` (see `session_manager` in the api crate). Only that page load opens
//! the autostart plugins: the marker is taken out of the address first, so reloading the page does not open them again.

const LOGIN_MARKER: &str = "from=login";

/// `search` (`?a=1&from=login`) without the login marker, and whether it had one.
fn without_login_marker(search: &str) -> (String, bool) {
    let query = search.strip_prefix('?').unwrap_or(search);
    let pairs: Vec<&str> = query.split('&').filter(|pair| !pair.is_empty()).collect();
    let rest: Vec<&str> = pairs.iter().copied().filter(|pair| *pair != LOGIN_MARKER).collect();
    let had = rest.len() != pairs.len();
    let search = if rest.is_empty() { String::new() } else { format!("?{}", rest.join("&")) };
    (search, had)
}

/// Whether this page load comes straight from the login page. Takes the marker out of the address (without a reload),
/// so the answer is `true` once per login.
pub fn take_login_marker() -> bool {
    let Some(window) = web_sys::window() else { return false };
    let location = window.location();
    let (search, had) = without_login_marker(&location.search().unwrap_or_default());
    if had {
        let url = format!("{}{}{}", location.pathname().unwrap_or_default(), search, location.hash().unwrap_or_default());
        if let Ok(history) = window.history() {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
        }
    }
    had
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_login_marker_is_found_and_taken_out() {
        assert_eq!(without_login_marker("?from=login"), (String::new(), true));
        assert_eq!(without_login_marker("?a=1&from=login&b=2"), ("?a=1&b=2".to_string(), true));
        assert_eq!(without_login_marker("?a=1"), ("?a=1".to_string(), false));
        assert_eq!(without_login_marker(""), (String::new(), false));
        assert_eq!(without_login_marker("?from=loginx"), ("?from=loginx".to_string(), false));
    }
}
