use super::state::{TabContent, WorkspaceState};
use crate::frontend_communication_relay as relay;
use api::framework_web_api;
use api::models::SpawnedComponent;
use dioxus::prelude::*;

/// Dispatches a tab's `TabContent` to the component that renders it. This is
/// the extension point for new kinds of tabs: add a variant to `TabContent` and a match
/// arm here without touching `Panel` or `TabBar`.
#[component]
pub fn TabContentView(content: TabContent, active: bool) -> Element {
    match content {
        TabContent::Component(component) => rsx! { PluginFrame { component, active } },
    }
}

/// Shows the iframe of one frontend component. The frontend process already exists
/// (the server spawned it before the tab was opened).
#[component]
pub fn PluginFrame(component: SpawnedComponent, active: bool) -> Element {
    let workspace = use_context::<WorkspaceState>();
    let plugin_id = component.plugin_id;
    let pid = component.frontend_process_id;

    let plugin = use_resource(move || async move {
        framework_web_api::get_plugin_from_id(plugin_id).await
    });

    // The page needs to know which plugin and component an iframe shows (component messaging, discovery).
    use_hook({
        let component_name = component.component_name.clone();
        move || relay::register_frontend(pid, plugin_id, component_name)
    });
    use_drop(move || relay::unregister_frontend(pid));

    // Tell the page when its tab is shown or hidden (`onShown` / `onHidden` in the SDK), once it has loaded.
    // `loads` counts how often the iframe loaded: a page that navigates or reloads starts without knowing its state.
    let mut loads = use_signal(|| 0u32);
    let mut announced = use_signal(|| None::<(u32, bool)>);
    use_effect(use_reactive!(|active| {
        let loads = loads();
        if loads > 0 && *announced.peek() != Some((loads, active)) {
            announced.set(Some((loads, active)));
            relay::send_lifecycle(pid, if active { "shown" } else { "hidden" });
        }
    }));

    rsx! {
        match &*plugin.read() {
            None => {
                rsx! { div { class: "panel-empty", "Loading..." } }
            }
            Some(Err(err)) => {
                debug!("{}", err);
                rsx! { div { class: "panel-empty", "ERROR: Could not load the plugin." } }
            }
            Some(Ok(None)) => {
                rsx! { div { class: "panel-empty", "Invalid plugin id." } }
            }
            Some(Ok(Some(plugin))) => {
                let Some(frontend) = plugin.component(&component.component_name) else {
                    return rsx! { div { class: "panel-empty", "ERROR: The plugin has no such component." } };
                };
                // Relative to the plugins directory: the server resolves it (and the browser never sees the
                // server's absolute paths, which a proxy collapsing `//` would also break).
                let entrypoint = format!("{}/{}", plugin.identifier(), frontend.entry_point_file_path.trim_start_matches('/'));

                let mut query = format!("fe_process_id={pid}&component={}", component.component_name);
                if let Some(parent) = component.parent_frontend_process_id {
                    query.push_str(&format!("&parent_fe_process_id={parent}"));
                }
                if let Some(params) = &component.params {
                    query.push_str(&format!("&params={}", urlencoding::encode(params)));
                }

                rsx! {
                    iframe {
                        id: pid,
                        class: "plugin-frame",
                        src: api::base_url::with_base(&format!("/plugins/{entrypoint}?{query}")),
                        // Injects the theme stylesheet and sets the page's theme once the plugin has loaded.
                        onload: move |_| {
                            relay::apply_theme(pid, &workspace.theme());
                            loads += 1;
                        },
                        width: "100%",
                        height: "100%",
                        style: "border: none;",
                    }
                }
            }
        }
    }
}
