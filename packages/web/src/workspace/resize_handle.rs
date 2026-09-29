use dioxus::prelude::*;

/// Which direction a handle drags along. `Horizontal` handles are vertical
/// bars that resize width (dragged left/right); `Vertical` handles are
/// horizontal bars that resize height (dragged up/down).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeAxis {
    Horizontal,
    Vertical,
}

/// A thin draggable divider between two panels. Reports pixel deltas along
/// its axis via `on_resize`; interpreting the delta (growing which panel,
/// clamping to min/max) is left to the caller.
///
/// Dragging is tracked without any JS interop: on mousedown a full-viewport
/// transparent overlay is rendered so mouse-move/mouse-up keep firing even
/// once the cursor leaves the thin handle itself.
#[component]
pub fn ResizeHandle(axis: ResizeAxis, on_resize: EventHandler<f64>) -> Element {
    let mut dragging = use_signal(|| false);
    let mut last_pos = use_signal(|| 0.0_f64);

    let handle_class = match axis {
        ResizeAxis::Horizontal => "resize-handle resize-handle-horizontal",
        ResizeAxis::Vertical => "resize-handle resize-handle-vertical",
    };
    let overlay_class = match axis {
        ResizeAxis::Horizontal => "resize-overlay resize-overlay-horizontal",
        ResizeAxis::Vertical => "resize-overlay resize-overlay-vertical",
    };

    let axis_pos = move |evt: &MouseEvent| {
        let coords = evt.client_coordinates();
        match axis {
            ResizeAxis::Horizontal => coords.x,
            ResizeAxis::Vertical => coords.y,
        }
    };

    rsx! {
        div {
            class: "{handle_class}",
            onmousedown: move |evt| {
                evt.stop_propagation();
                last_pos.set(axis_pos(&evt));
                dragging.set(true);
            },
        }
        if dragging() {
            div {
                class: "{overlay_class}",
                onmousemove: move |evt| {
                    let pos = axis_pos(&evt);
                    let delta = pos - last_pos();
                    last_pos.set(pos);
                    on_resize.call(delta);
                },
                onmouseup: move |_| dragging.set(false),
                onmouseleave: move |_| dragging.set(false),
            }
        }
    }
}
