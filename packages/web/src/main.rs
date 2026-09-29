#[allow(dead_code)]
mod plugin_frame;
mod frontend_communication_relay;
mod workspace;

use dioxus::prelude::*;
use workspace::Workspace;


fn main() {
    #[cfg(feature = "server")]
    {
        api::run_server(App);
    }
    #[cfg(not(feature = "server"))]
    {
        dioxus::launch(App);
    }
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
