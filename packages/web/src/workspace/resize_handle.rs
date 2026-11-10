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
/// Dragging uses pointer events, so a mouse, a pen and a finger (iPad) all work,
/// without any JS interop. A mouse is not captured: on pointerdown a full-viewport
/// transparent overlay is rendered so moves and the release keep arriving once
/// the cursor leaves the thin handle. A finger is captured by the browser: its
/// moves keep going to the handle itself, so the handle handles them too.
///
/// While hovered or dragged, the handle glows in the accent color, centered
/// where the pointer is along the bar (`--glow`, in px from the handle's start).
#[component]
pub fn ResizeHandle(axis: ResizeAxis, on_resize: EventHandler<f64>) -> Element {
    let mut dragging = use_signal(|| false);
    let mut last_pos = use_signal(|| 0.0_f64);
    // Where the glow is, along the bar, in px from the handle's start; None hides it.
    let mut glow = use_signal(|| None::<f64>);
    // Page coordinate of the handle's start along the bar, measured on pointerdown, so the
    // overlay (which reports page coordinates) can place the glow during a drag.
    let mut start = use_signal(|| 0.0_f64);

    let base_class = match axis {
        ResizeAxis::Horizontal => "resize-handle resize-handle-horizontal",
        ResizeAxis::Vertical => "resize-handle resize-handle-vertical",
    };
    let handle_class = if glow().is_some() || dragging() {
        format!("{base_class} resize-handle-glowing")
    } else {
        base_class.to_string()
    };
    let style = glow().map(|g| format!("--glow: {g}px")).unwrap_or_default();
    let overlay_class = match axis {
        ResizeAxis::Horizontal => "resize-overlay resize-overlay-horizontal",
        ResizeAxis::Vertical => "resize-overlay resize-overlay-vertical",
    };

    // (along the drag direction, along the bar), in page coordinates
    let page_pos = move |evt: &PointerEvent| {
        let c = evt.client_coordinates();
        match axis {
            ResizeAxis::Horizontal => (c.x, c.y),
            ResizeAxis::Vertical => (c.y, c.x),
        }
    };
    // along the bar, from the handle's start
    let on_bar = move |evt: &PointerEvent| {
        let e = evt.element_coordinates();
        match axis {
            ResizeAxis::Horizontal => e.y,
            ResizeAxis::Vertical => e.x,
        }
    };
    let mut drag_to = move |evt: &PointerEvent| {
        let (pos, along) = page_pos(evt);
        let delta = pos - last_pos();
        last_pos.set(pos);
        glow.set(Some(along - start()));
        on_resize.call(delta);
    };
    let mut stop = move || {
        dragging.set(false);
        glow.set(None);
    };

    rsx! {
        div {
            class: "{handle_class}",
            style: "{style}",
            onpointerdown: move |evt| {
                evt.stop_propagation();
                evt.prevent_default();
                let (pos, along) = page_pos(&evt);
                let bar = on_bar(&evt);
                last_pos.set(pos);
                start.set(along - bar);
                glow.set(Some(bar));
                dragging.set(true);
            },
            onpointermove: move |evt| {
                if dragging() {
                    drag_to(&evt); // a finger: captured by the handle
                } else {
                    glow.set(Some(on_bar(&evt)));
                }
            },
            onpointerup: move |_| stop(),
            onpointercancel: move |_| stop(),
            onpointerleave: move |_| {
                if !dragging() {
                    glow.set(None);
                }
            },
            div { class: "resize-glow" }
        }
        if dragging() {
            div {
                class: "{overlay_class}",
                onpointermove: move |evt| drag_to(&evt),
                onpointerup: move |_| stop(),
                onpointercancel: move |_| stop(),
                onpointerleave: move |_| stop(),
            }
        }
    }
}
