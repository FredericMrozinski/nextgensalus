use super::state::WorkspaceState;
use api::framework_web_api;
use dioxus::prelude::*;
use crate::workspace::state::PanelId::Center;
use crate::workspace::state::TabContent::Plugin;

/// Shown when a panel's "+" button is pressed. Lists whatever
/// `get_available_plugins` currently reports — nothing is clickable yet,
/// this only surfaces what's there. Rendered centered on screen for now;
/// where it actually belongs in the workspace is still undecided.
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
                                    onclick: move |_| workspace.open_tab(Center, Plugin(id, description.clone())),
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
