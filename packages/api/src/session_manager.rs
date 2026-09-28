use axum::http::{StatusCode};
use axum::response::Redirect;
use axum_extra::extract::cookie::{Cookie, CookieJar};

pub fn get_user_id_from_session(session_id: u32) -> Result<u32, StatusCode> {
    if session_id == 0 { // TODO remove
        Ok(0)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub async fn dev_login(cookie_jar: CookieJar) -> (CookieJar, Redirect) {
    let cookie = Cookie::build(("session_id", "0"))
        .path("/") // makes it visible to every route, not just /plugins
        .build();

    (cookie_jar.add(cookie), Redirect::to("/"))
}

pub async fn session_valid(cookie_jar: CookieJar) -> bool {
    let session_cookie = cookie_jar.get("session_id").map(|c| c.value().to_string());
    
    if session_cookie.is_some() && session_cookie.unwrap() == "0" {
        return true;
    }
    false   
}