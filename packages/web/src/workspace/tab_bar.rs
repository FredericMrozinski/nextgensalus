use super::state::{PanelId, TabId, WorkspaceState};
use api::framework_web_api;
use dioxus::core::spawn_forever;
use dioxus::prelude::*;

/// Row of tabs for one panel, plus the "new tab" button. Mirrors a browser's
/// tab strip: click a tab to switch to it, click its close button to remove
/// it, click "+" to open the plugin picker for this panel.
#[component]
pub fn TabBar(panel_id: PanelId) -> Element {
    let workspace = use_context::<WorkspaceState>();
    let tabs = workspace.tabs(panel_id);

    rsx! {
        div { class: "tab-bar",
            div { class: "tab-list",
                for tab in tabs {
                    TabButton {
                        key: "{tab.id.raw()}",
                        panel_id,
                        tab_id: tab.id,
                        title: tab.title.clone(),
                        active: workspace.is_active(panel_id, tab.id),
                    }
                }
            }
            button {
                class: "new-tab-button",
                title: "New tab",
                onclick: move |_| workspace.open_plugin_picker(panel_id),
                "+"
            }
        }
    }
}

#[component]
fn TabButton(panel_id: PanelId, tab_id: TabId, title: String, active: bool) -> Element {
    let workspace = use_context::<WorkspaceState>();
    let class = if active { "tab tab-active" } else { "tab" };

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| workspace.activate_tab(panel_id, tab_id),
            span { class: "tab-title", "{title}" }
            button {
                class: "tab-close",
                title: "Close tab",
                onclick: move |evt| {
                    evt.stop_propagation();
                    if let Some(frontend_process_id) = workspace.close_tab(panel_id, tab_id) {
                        // The plugin page is gone: let the server close its process (and a `panel` backend with the last one).
                        // Not tied to this button: it disappears together with the tab, which would cancel the request.
                        spawn_forever(async move {
                            let _ = framework_web_api::close_frontend_process(frontend_process_id).await;
                        });
                    }
                },
                "×"
            }
        }
    }
}
