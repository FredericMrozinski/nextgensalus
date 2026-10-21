#[allow(dead_code)]
mod frontend_communication_relay;
mod workspace;

use dioxus::prelude::*;
use workspace::Workspace;


fn main() {
    #[cfg(feature = "server")] {
        api::run_server(App);
    }

    #[cfg(feature = "web")]
    init_server_url();

    #[cfg(not(feature = "server"))] {
        dioxus::launch(App);
    }
}

#[cfg(feature = "web")]
fn init_server_url() {
    let origin = web_sys::window().unwrap().location().origin().unwrap();
    let url = format!("{origin}/user/fmrozins/salus/salus"); // same as base_path, with leading slash
    dioxus_fullstack::set_server_url(Box::leak(url.into_boxed_str()));
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
