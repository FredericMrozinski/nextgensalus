#[allow(dead_code)]
mod frontend_communication_relay;
mod workspace;

use dioxus::prelude::*;
use workspace::Workspace;


fn main() {
    #[cfg(feature = "server")] {
        api::run_server(App);
    }
    #[cfg(feature = "web")]  {
        console_error_panic_hook::set_once();
    }
    console_error_panic_hook::set_once();
    #[cfg(not(feature = "server"))] {
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
