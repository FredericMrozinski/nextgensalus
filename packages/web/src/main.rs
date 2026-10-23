#[allow(dead_code)]
mod frontend_communication_relay;
mod workspace;

use dioxus::prelude::*;
use workspace::Workspace;


fn main() {
    #[cfg(feature = "server")]
    api::run_server(App);

    #[cfg(all(feature = "web", not(feature = "server")))]
    {
        dioxus::logger::initialize_default();
        init_server_url();
        dioxus::web::launch::launch_cfg(App, dioxus::web::Config::new().hydrate(true));
    }

    #[cfg(not(any(feature = "server", feature = "web")))]
    dioxus::launch(App);
}

#[cfg(feature = "web")]
fn init_server_url() {
    let origin = web_sys::window().unwrap().location().origin().unwrap();
    let base = dioxus::cli_config::base_path()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let base = base.trim_matches('/');
    let url = if base.is_empty() { origin } else { format!("{origin}/{base}") };
    dioxus::fullstack::set_server_url(Box::leak(url.into_boxed_str()));
}

#[component]
fn App() -> Element {
    use_effect(|| {
        frontend_communication_relay::init_plugin_bridge();
    });

    rsx! {
        Workspace {}
    }
}
