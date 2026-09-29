mod panel;
mod plugin_picker;
mod resize_handle;
mod state;
mod tab_bar;
mod tab_content;

pub use state::WorkspaceState;

use dioxus::prelude::*;
use panel::Panel;
use plugin_picker::PluginPicker;
use resize_handle::{ResizeAxis, ResizeHandle};
use state::PanelId;

/// Root of the IDE-like shell: one center panel, two side panels and a
/// bottom panel, each independently resizable by dragging the handles
/// between them.
#[component]
pub fn Workspace() -> Element {
    let workspace = use_context_provider(WorkspaceState::new);
    use_hook(|| workspace.seed_demo_tabs());

    let left_width = workspace.left_width();
    let right_width = workspace.right_width();
    let bottom_height = workspace.bottom_height();

    rsx! {
        document::Stylesheet { href: asset!("/assets/workspace.css") }
        div { class: "workspace-root",
            div { class: "workspace-main-row",
                div {
                    class: "panel-slot panel-slot-left",
                    style: "width: {left_width}px",
                    Panel { panel_id: PanelId::Left }
                }
                ResizeHandle { axis: ResizeAxis::Horizontal, on_resize: move |dx| workspace.resize_left(dx) }
                div { class: "panel-slot panel-slot-center",
                    Panel { panel_id: PanelId::Center }
                }
                ResizeHandle { axis: ResizeAxis::Horizontal, on_resize: move |dx| workspace.resize_right(dx) }
                div {
                    class: "panel-slot panel-slot-right",
                    style: "width: {right_width}px",
                    Panel { panel_id: PanelId::Right }
                }
            }
            ResizeHandle { axis: ResizeAxis::Vertical, on_resize: move |dy| workspace.resize_bottom(dy) }
            div {
                class: "panel-slot panel-slot-bottom",
                style: "height: {bottom_height}px",
                Panel { panel_id: PanelId::Bottom }
            }
        }
        if workspace.plugin_picker_target().is_some() {
            PluginPicker {}
        }
    }
}
