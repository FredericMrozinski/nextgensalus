mod panel;
mod plugin_picker;
mod resize_handle;
mod state;
mod tab_bar;
mod tab_content;
mod theme;
mod viewer_picker;

pub use state::WorkspaceState;

use dioxus::prelude::*;
use futures::StreamExt;
use std::cell::RefCell;
use std::rc::Rc;
use panel::Panel;
use plugin_picker::PluginPicker;
use viewer_picker::ViewerPicker;
use resize_handle::{ResizeAxis, ResizeHandle};
use state::{PanelId, TabContent, Theme, ViewerPickerRequest};

/// Root of the IDE-like shell: one center panel, two side panels and a
/// bottom panel, each independently resizable by dragging the handles
/// between them.
#[component]
pub fn Workspace() -> Element {
    let workspace = use_context_provider(WorkspaceState::new);
    use_hook(|| workspace.seed_demo_tabs());
    // Components Salus opens (requested by plugins) arrive here. They come over a channel so the tab is
    // opened from a task owned by this component. A component that already runs is only focused.
    let component_requests = use_hook(|| {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        crate::frontend_communication_relay::set_component_tab_opener(move |component| {
            let _ = tx.unbounded_send(component);
        });
        Rc::new(RefCell::new(Some(rx)))
    });
    use_future(move || {
        let component_requests = component_requests.clone();
        async move {
            let Some(mut requests) = component_requests.borrow_mut().take() else {
                return;
            };
            while let Some(component) = requests.next().await {
                if component.reused && workspace.focus_frontend(component.frontend_process_id) {
                    continue;
                }
                workspace.open_tab(component.target_panel.into(), TabContent::Component(component));
            }
        }
    });

    // Files that several viewers can open: the user chooses (a plugin only asks to open the file).
    let viewer_requests = use_hook(|| {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        crate::frontend_communication_relay::set_viewer_picker(move |file, viewers, reply| {
            let _ = tx.unbounded_send(ViewerPickerRequest::new(file, viewers, reply));
        });
        Rc::new(RefCell::new(Some(rx)))
    });
    use_future(move || {
        let viewer_requests = viewer_requests.clone();
        async move {
            let Some(mut requests) = viewer_requests.borrow_mut().take() else {
                return;
            };
            while let Some(request) = requests.next().await {
                workspace.show_viewer_picker(request);
            }
        }
    });

    // Theme: restore the user's choice, then keep the page and all plugin pages in sync with it.
    use_effect(move || {
        if let Some(stored) = theme::load_stored_theme() {
            workspace.set_theme(stored);
        }
    });
    use_effect(move || {
        let current = workspace.theme();
        theme::apply_page_theme(current);
        crate::frontend_communication_relay::apply_theme_to_all_frames(current.as_str());
    });

    let left_width = workspace.left_width();
    let right_width = workspace.right_width();
    let bottom_height = workspace.bottom_height();

    rsx! {
        document::Stylesheet { href: "/salus/theme.css" }
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
        button {
            class: "theme-toggle",
            title: "Switch between light and dark theme",
            onclick: move |_| workspace.set_theme(workspace.theme().toggled()),
            if workspace.theme() == Theme::Dark { "☀" } else { "☾" }
        }
        if workspace.plugin_picker_target().is_some() {
            PluginPicker {}
        }
        ViewerPicker {}
    }
}
