use super::state::{PanelId, WorkspaceState};
use super::tab_bar::TabBar;
use super::tab_content::TabContentView;
use dioxus::prelude::*;

/// One of the workspace's four fixed slots: a tab bar plus the content of its tabs. Identical for
/// every slot — only which `PanelId` it reads from context differs.
///
/// Every tab's content stays mounted and only the active one is visible. Plugin pages keep their
/// state, stay reachable for messages from other components and keep their size while hidden
/// (so viewers do not have to re-measure); switching tabs must never reload a plugin.
#[component]
pub fn Panel(panel_id: PanelId) -> Element {
    let workspace = use_context::<WorkspaceState>();
    let tabs = workspace.tabs(panel_id);

    rsx! {
        div { class: "panel",
            TabBar { panel_id }
            div { class: "panel-content",
                if tabs.is_empty() {
                    div { class: "panel-empty", "No tabs open" }
                }
                for tab in tabs {
                    div {
                        key: "{tab.id.raw()}",
                        class: if workspace.is_active(panel_id, tab.id) { "panel-tab" } else { "panel-tab panel-tab-hidden" },
                        TabContentView { content: tab.content }
                    }
                }
            }
        }
    }
}
