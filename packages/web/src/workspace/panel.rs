use super::state::{PanelId, WorkspaceState};
use super::tab_bar::TabBar;
use super::tab_content::TabContentView;
use dioxus::prelude::*;

/// One of the workspace's four fixed slots: a tab bar plus the active tab's
/// content. Identical for every slot — only which `PanelId` it reads from
/// context differs.
#[component]
pub fn Panel(panel_id: PanelId) -> Element {
    let workspace = use_context::<WorkspaceState>();
    let active = workspace.active_tab(panel_id);

    rsx! {
        div { class: "panel",
            TabBar { panel_id }
            div { class: "panel-content",
                match active {
                    Some(tab) => rsx! { TabContentView { content: tab.content } },
                    None => rsx! { div { class: "panel-empty", "No tabs open" } },
                }
            }
        }
    }
}
