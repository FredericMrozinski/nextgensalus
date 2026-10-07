use super::state::WorkspaceState;
use dioxus::prelude::*;

/// Shown when a plugin asked Salus to open a file that several viewers can display.
/// Plugins never see this: they only ask to open the file, Salus asks the user.
#[component]
pub fn ViewerPicker() -> Element {
    let workspace = use_context::<WorkspaceState>();
    let Some(request) = workspace.viewer_picker() else {
        return rsx! {};
    };
    let file_name = request.file.rsplit('/').next().unwrap_or(&request.file).to_string();

    rsx! {
        div {
            class: "plugin-picker-overlay",
            onclick: move |_| workspace.resolve_viewer_picker(None),
            div {
                class: "plugin-picker",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "plugin-picker-header",
                    span { "Open {file_name} with" }
                    button {
                        class: "plugin-picker-close",
                        title: "Cancel",
                        onclick: move |_| workspace.resolve_viewer_picker(None),
                        "×"
                    }
                }
                div { class: "plugin-picker-list",
                    for viewer in request.viewers.clone() {
                        div {
                            key: "{viewer.plugin_id}-{viewer.component_name}",
                            class: "plugin-picker-item",
                            onclick: {
                                let viewer = viewer.clone();
                                move |_| workspace.resolve_viewer_picker(Some(viewer.clone()))
                            },
                            "{viewer.title}"
                            if viewer.title != viewer.plugin_name {
                                span { class: "plugin-picker-hint", " ({viewer.plugin_name})" }
                            }
                        }
                    }
                }
            }
        }
    }
}
