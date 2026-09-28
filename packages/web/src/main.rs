mod plugin_frame;
mod frontend_communication_relay;

use dioxus::prelude::*;
use plugin_frame::PluginFrame;


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
        PluginFrame { plugin_id: 10000 }
    }
}
