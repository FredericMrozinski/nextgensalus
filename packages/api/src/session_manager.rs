use axum::extract::Request;
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, CookieJar};

pub fn get_user_id_from_session(session_id: u32) -> Result<u32, StatusCode> {
    if session_id == 0 { // TODO remove
        Ok(0)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// The login page. For now there is nothing to enter: it logs in as the development user (sets the session cookie), says
/// so, and sends the browser back to Salus. Its stylesheet and the way back are relative URLs, so it works behind any
/// prefix of a proxy in front (JupyterHub).
pub async fn dev_login(cookie_jar: CookieJar) -> (CookieJar, Response) {
    let cookie = Cookie::build(("session_id", "0"))
        // The proxy in front (JupyterHub) may add a prefix the server cannot see, so a path derived from
        // our own base path would not match the browser's URL. "/" works behind any prefix.
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .build();

    let page = ([(header::CACHE_CONTROL, "no-store")], Html(login_page(&crate::config_loader::theme()))).into_response();
    (cookie_jar.add(cookie), page)
}

/// Where the login page sends the browser: Salus' root, marked so that a browser that does not keep the cookie does not loop.
const AFTER_LOGIN: &str = "../?from=login";

fn login_page(theme: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en" data-theme="{theme}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Salus</title>
<link rel="icon" type="image/svg+xml" href="../assets/icon.svg">
<link rel="icon" type="image/png" sizes="32x32" href="../assets/icon-32.png">
<link rel="apple-touch-icon" href="../assets/icon-180.png">
<link rel="stylesheet" href="../assets/theme.css">
<meta http-equiv="refresh" content="1;url={AFTER_LOGIN}">
<style>
  body {{ min-height: 100vh; display: flex; align-items: center; justify-content: center; }}
  main {{ display: flex; flex-direction: column; align-items: center; gap: 20px; }}
  h1 {{ margin: 0; font-size: 15px; font-weight: 500; color: var(--salus-fg-muted); letter-spacing: 0.02em; }}
  .spinner {{ width: 28px; height: 28px; border-radius: 50%; border: 2px solid var(--salus-border);
             border-top-color: var(--salus-accent); animation: spin 0.9s linear infinite; }}
  @keyframes spin {{ to {{ transform: rotate(360deg); }} }}
  @media (prefers-reduced-motion: reduce) {{ .spinner {{ animation: none; border-color: var(--salus-accent); }} }}
</style>
</head>
<body>
<main>
  <div class="spinner" role="progressbar" aria-label="Logging in"></div>
  <h1>Logging in…</h1>
</main>
<script>setTimeout(function () {{ location.replace("{AFTER_LOGIN}"); }}, 600);</script>
</body>
</html>
"#
    )
}

pub async fn session_valid(cookie_jar: CookieJar) -> bool {
    let session_cookie = cookie_jar.get("session_id").map(|c| c.value().to_string());

    if session_cookie.is_some() && session_cookie.unwrap() == "0" {
        return true;
    }
    false
}

/// The user a request's session cookie belongs to, if the session is valid.
pub fn user_id_from_jar(cookie_jar: &CookieJar) -> Option<u32> {
    let session_id = cookie_jar.get("session_id")?.value().parse::<u32>().ok()?;
    get_user_id_from_session(session_id).ok()
}

// ============================================================================
// Sending visitors without a session to the login page
// ============================================================================

/// Paths (below the base path) that belong to something other than the application page; they check the session themselves.
const NOT_THE_APP: [&str; 8] =
    ["/assets/", "/plugins/", "/plugin-api/", "/plugin-config/", "/settings-api/", "/plugin-stream", "/dev/login", "/api/"];

fn is_application_page(path: &str) -> bool {
    !NOT_THE_APP.iter().any(|prefix| path.starts_with(prefix) || path == prefix.trim_end_matches('/'))
}

fn wants_html(headers: &HeaderMap) -> bool {
    headers.get(header::ACCEPT).and_then(|value| value.to_str().ok()).is_some_and(|accept| accept.contains("text/html"))
}

/// The login page as a relative URL from `path` (below the base path), so a proxy's prefix is kept:
/// `/` -> `dev/login`, `/a/b` -> `../dev/login`, `/a/b/` -> `../../dev/login`.
fn login_url_from(path: &str) -> String {
    let segments = path.split('/').filter(|segment| !segment.is_empty()).count();
    let up = if path.ends_with('/') { segments } else { segments.saturating_sub(1) };
    format!("{}dev/login", "../".repeat(up))
}

/// Middleware for the application page: a browser that opens Salus without a valid session cookie is sent to the login page,
/// which sets one and sends it back. A browser that comes back from the login page still without a session (it blocks
/// cookies) gets an explanation instead of another round trip.
pub async fn require_session(cookie_jar: CookieJar, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let guarded = request.method() == Method::GET && wants_html(request.headers()) && is_application_page(path);
    if guarded && user_id_from_jar(&cookie_jar).is_none() {
        if request.uri().query().is_some_and(|query| query.split('&').any(|pair| pair == "from=login")) {
            return (
                StatusCode::FORBIDDEN,
                Html("<!doctype html><meta charset=\"utf-8\"><title>Salus</title><p style=\"font-family: system-ui; margin: 3em auto; max-width: 30em\">\
                      Logging in did not work: your browser did not keep the session cookie. Allow cookies for this site and reload.</p>"),
            )
                .into_response();
        }
        return Redirect::to(&login_url_from(path)).into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[test]
    fn the_login_url_is_relative_to_the_page() {
        assert_eq!(login_url_from("/"), "dev/login");
        assert_eq!(login_url_from(""), "dev/login");
        assert_eq!(login_url_from("/a"), "dev/login");
        assert_eq!(login_url_from("/a/b"), "../dev/login");
        assert_eq!(login_url_from("/a/b/"), "../../dev/login");
    }

    #[test]
    fn only_the_application_page_is_guarded() {
        assert!(is_application_page("/"));
        assert!(is_application_page("/anything"));
        for other in ["/assets/web.js", "/plugins/x/index.html", "/plugin-api/1/x", "/plugin-config/1", "/settings-api/1/configurables", "/plugin-stream", "/dev/login", "/api/get_theme"] {
            assert!(!is_application_page(other), "{other}");
        }
    }

    fn app() -> Router {
        Router::new()
            .route("/", get(|| async { "the app" }))
            .route("/assets/x.js", get(|| async { "script" }))
            .fallback(|| async { "fallback page" })
            .layer(axum::middleware::from_fn(require_session))
    }

    async fn call(uri: &str, accept: &str, cookie: Option<&str>) -> (StatusCode, Option<String>, String) {
        let mut request = axum::http::Request::builder().uri(uri).header(header::ACCEPT, accept);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let response = app().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let location = response.headers().get(header::LOCATION).map(|v| v.to_str().unwrap().to_string());
        let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        (status, location, String::from_utf8_lossy(&body).into_owned())
    }

    const HTML: &str = "text/html,application/xhtml+xml";

    #[tokio::test]
    async fn visitors_without_a_session_go_to_the_login_page() {
        let (status, location, _) = call("/", HTML, None).await;
        assert_eq!((status, location.as_deref()), (StatusCode::SEE_OTHER, Some("dev/login")));
        let (_, location, _) = call("/some/deep/page", HTML, Some("session_id=7")).await;
        assert_eq!(location.as_deref(), Some("../../dev/login"), "an unknown session counts as none");
        let (_, location, _) = call("/some/deep/page/", HTML, None).await;
        assert_eq!(location.as_deref(), Some("../../../dev/login"));
    }

    #[tokio::test]
    async fn a_valid_session_and_everything_that_is_not_the_page_pass() {
        assert_eq!(call("/", HTML, Some("session_id=0")).await.2, "the app");
        assert_eq!(call("/other", HTML, Some("session_id=0")).await.2, "fallback page");
        assert_eq!(call("/assets/x.js", "*/*", None).await.2, "script");
        assert_eq!(call("/", "application/json", None).await.2, "the app", "only browser navigations are redirected");
    }

    #[tokio::test]
    async fn a_browser_that_drops_the_cookie_is_told_instead_of_looping() {
        let (status, location, body) = call("/?from=login", HTML, None).await;
        assert_eq!((status, location), (StatusCode::FORBIDDEN, None));
        assert!(body.contains("did not keep the session cookie"));
    }

    #[test]
    fn the_login_page_says_so_and_goes_back_by_a_relative_url() {
        let page = login_page("light");
        assert!(page.contains("Logging in…") && page.contains(r#"data-theme="light""#));
        assert!(page.contains(r#"href="../assets/theme.css""#) && page.contains(AFTER_LOGIN));
        assert!(page.contains(r#"href="../assets/icon.svg""#) && page.contains(r#"href="../assets/icon-180.png""#));
    }
}
