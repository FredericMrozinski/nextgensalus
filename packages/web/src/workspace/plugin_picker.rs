use super::state::{TabContent, WorkspaceState};
use api::framework_web_api;
use dioxus::prelude::*;

/// Shown when a panel's "+" button is pressed. Lists the plugins that name an entry component
/// (`get_available_plugins`); picking one opens that component in the panel that was clicked.
#[component]
pub fn PluginPicker() -> Element {
    let workspace = use_context::<WorkspaceState>();
    let available_plugins = use_resource(|| async move { framework_web_api::get_available_plugins().await });

    rsx! {
        div {
            class: "plugin-picker-overlay",
            onclick: move |_| workspace.close_plugin_picker(),
            div {
                class: "plugin-picker",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "plugin-picker-header",
                    span { "Available Plugins" }
                    button {
                        class: "plugin-picker-close",
                        title: "Close",
                        onclick: move |_| workspace.close_plugin_picker(),
                        "×"
                    }
                }
                div { class: "plugin-picker-list",
                    match &*available_plugins.read() {
                        None => rsx! {
                            div { class: "plugin-picker-status", "Loading..." }
                        },
                        Some(Err(_)) => rsx! {
                            div { class: "plugin-picker-status", "Failed to load plugins." }
                        },
                        Some(Ok(plugins)) if plugins.is_empty() => rsx! {
                            div { class: "plugin-picker-status", "No plugins available." }
                        },
                        Some(Ok(plugins)) => rsx! {
                            for (id , description) in plugins.clone() {
                                div {
                                    key: "{id}",
                                    class: "plugin-picker-item",
                                    onclick: move |_| {
                                        let Some(panel) = workspace.plugin_picker_target() else { return };
                                        spawn(async move {
                                            match framework_web_api::open_plugin(id, Some(crate::frontend_communication_relay::page_id())).await {
                                                Ok(component) => {
                                                    workspace.open_tab(panel, TabContent::Component(component));
                                                    workspace.close_plugin_picker();
                                                }
                                                Err(err) => error!("Could not open the plugin: {err}"),
                                            }
                                        });
                                    },
                                    "{description.name}"
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}
