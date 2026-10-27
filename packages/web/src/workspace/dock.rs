use super::state::{TabContent, WorkspaceState};
use api::framework_web_api::{self, DockPlugin};
use dioxus::prelude::*;

/// Material "apps" icon (nine dots), drawn in the current text color so it follows the theme.
const LAUNCHER_ICON_PATH: &str = "M205.04-205.04q-14.27-14.27-14.27-34.96t14.27-34.96q14.27-14.27 34.96-14.27t34.96 14.27q14.27 14.27 14.27 34.96t-14.27 34.96q-14.27 14.27-34.96 14.27t-34.96-14.27Zm240 0q-14.27-14.27-14.27-34.96t14.27-34.96q14.27-14.27 34.96-14.27t34.96 14.27q14.27 14.27 14.27 34.96t-14.27 34.96q-14.27 14.27-34.96 14.27t-34.96-14.27Zm240 0q-14.27-14.27-14.27-34.96t14.27-34.96q14.27-14.27 34.96-14.27t34.96 14.27q14.27 14.27 14.27 34.96t-14.27 34.96q-14.27 14.27-34.96 14.27t-34.96-14.27Zm-480-240q-14.27-14.27-14.27-34.96t14.27-34.96q14.27-14.27 34.96-14.27t34.96 14.27q14.27 14.27 14.27 34.96t-14.27 34.96q-14.27 14.27-34.96 14.27t-34.96-14.27Zm240 0q-14.27-14.27-14.27-34.96t14.27-34.96q14.27-14.27 34.96-14.27t34.96 14.27q14.27 14.27 14.27 34.96t-14.27 34.96q-14.27 14.27-34.96 14.27t-34.96-14.27Zm240 0q-14.27-14.27-14.27-34.96t14.27-34.96q14.27-14.27 34.96-14.27t34.96 14.27q14.27 14.27 14.27 34.96t-14.27 34.96q-14.27 14.27-34.96 14.27t-34.96-14.27Zm-480-240q-14.27-14.27-14.27-34.96t14.27-34.96q14.27-14.27 34.96-14.27t34.96 14.27q14.27 14.27 14.27 34.96t-14.27 34.96q-14.27 14.27-34.96 14.27t-34.96-14.27Zm240 0q-14.27-14.27-14.27-34.96t14.27-34.96q14.27-14.27 34.96-14.27t34.96 14.27q14.27 14.27 14.27 34.96t-14.27 34.96q-14.27 14.27-34.96 14.27t-34.96-14.27Zm240 0q-14.27-14.27-14.27-34.96t14.27-34.96q14.27-14.27 34.96-14.27t34.96 14.27q14.27 14.27 14.27 34.96t-14.27 34.96q-14.27 14.27-34.96 14.27t-34.96-14.27Z";

/// Bar on the left edge of the window with one button per plugin. Pressing it opens the plugin's
/// entry component in the panel the component asks for. The launcher button at the bottom extends
/// the bar to show the plugin names and a search field.
#[component]
pub fn Dock() -> Element {
    let plugins = use_resource(|| async move { framework_web_api::get_dock_plugins().await });
    let mut expanded = use_signal(|| false);
    let mut query = use_signal(String::new);
    // Center (in the page) of the icon the mouse is over: a light source glows there.
    let mut hover = use_signal(|| None::<(f64, f64)>);

    let needle = query.read().trim().to_lowercase();
    let class = match (expanded(), hover().is_some()) {
        (true, true) => "dock dock-expanded dock-hovering",
        (true, false) => "dock dock-expanded",
        (false, true) => "dock dock-hovering",
        (false, false) => "dock",
    };
    let style = hover().map(|(x, y)| format!("--hover-x: {x}px; --hover-y: {y}px")).unwrap_or_default();

    rsx! {
        nav { class: "{class}", style: "{style}",
            if expanded() {
                input {
                    class: "dock-search",
                    r#type: "search",
                    placeholder: "Search plugins",
                    value: "{query}",
                    oninput: move |evt| query.set(evt.value()),
                    onmounted: move |evt| async move {
                        let _ = evt.set_focus(true).await;
                    },
                    onkeydown: move |evt| {
                        if evt.key() == Key::Escape {
                            expanded.set(false);
                        }
                    },
                }
            }
            div { class: "dock-list",
                if let Some(Ok(plugins)) = &*plugins.read() {
                    for plugin in plugins.iter().filter(|p| p.name.to_lowercase().contains(&needle)).cloned() {
                        DockItem {
                            key: "{plugin.plugin_id}",
                            plugin,
                            show_name: expanded(),
                            on_opened: move |_| expanded.set(false),
                            on_hover: move |center| hover.set(center),
                        }
                    }
                }
            }
            button {
                class: "dock-item dock-launcher",
                title: if expanded() { "Close the plugin list" } else { "All plugins" },
                onclick: move |_| {
                    query.set(String::new());
                    expanded.set(!expanded());
                },
                svg {
                    class: "dock-launcher-icon",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 -960 960 960",
                    fill: "currentColor",
                    path { d: LAUNCHER_ICON_PATH }
                }
            }
        }
    }
}

#[component]
fn DockItem(
    plugin: DockPlugin,
    show_name: bool,
    on_opened: EventHandler<()>,
    /// The mouse entered (`Some(center of this item's icon)`) or left (`None`) the item.
    on_hover: EventHandler<Option<(f64, f64)>>,
) -> Element {
    let workspace = use_context::<WorkspaceState>();
    let id = plugin.plugin_id;
    let mut mounted = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let initial = plugin.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();

    rsx! {
        button {
            class: "dock-item",
            title: "{plugin.name}",
            onmouseenter: move |_| async move {
                let element = mounted.peek().clone();
                if let Some(Ok(rect)) = match element {
                    Some(element) => Some(element.get_client_rect().await),
                    None => None,
                } {
                    on_hover.call(Some((rect.origin.x + rect.size.width / 2.0, rect.origin.y + rect.size.height / 2.0)));
                }
            },
            onmouseleave: move |_| on_hover.call(None),
            onclick: move |_| {
                spawn(async move {
                    match framework_web_api::open_plugin(id, Some(crate::frontend_communication_relay::page_id())).await {
                        Ok(component) => {
                            workspace.open_tab(component.target_panel.into(), TabContent::Component(component));
                            on_opened.call(());
                        }
                        Err(err) => error!("Could not open the plugin: {err}"),
                    }
                });
            },
            // The light is centered on the icon itself, also when the dock is expanded and the icon sits at its left.
            span { class: "dock-icon-frame", onmounted: move |evt| mounted.set(Some(evt.data())),
                if let Some(icon) = &plugin.icon {
                    img { class: "dock-icon", src: api::base_url::with_base(&format!("/plugins/{icon}")), alt: "" }
                } else {
                    span { class: "dock-icon-letter", "{initial}" }
                }
            }
            if show_name {
                span { class: "dock-name", "{plugin.name}" }
            }
        }
    }
}
