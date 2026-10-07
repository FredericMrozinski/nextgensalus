use super::state::{TabContent, WorkspaceState};
use crate::frontend_communication_relay as relay;
use api::framework_web_api;
use api::models::SpawnedComponent;
use dioxus::prelude::*;
use std::path::PathBuf;

const SAMPLE_PLUGIN_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8" />
<style>
  body { font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; margin: 0; padding: 24px; background: #1b1e25; color: #e6e6e6; }
  h1 { font-size: 18px; margin-top: 0; }
  p { color: #a9adba; }
</style>
</head>
<body>
  <h1>Sample Plugin</h1>
  <p>This is placeholder content rendered inside an iframe.</p>
</body>
</html>"#;

/// Dispatches a tab's `TabContent` to the component that renders it. This is
/// the extension point for new kinds of tabs: add a variant to `TabContent` and a match
/// arm here without touching `Panel` or `TabBar`.
#[component]
pub fn TabContentView(content: TabContent) -> Element {
    match content {
        TabContent::SamplePlugin => rsx! { SamplePluginFrame {} },
        TabContent::Component(component) => rsx! { PluginFrame { component } },
    }
}

#[component]
fn SamplePluginFrame() -> Element {
    rsx! {
        iframe {
            class: "plugin-frame",
            title: "Sample Plugin",
            srcdoc: SAMPLE_PLUGIN_HTML,
            width: "100%",
            height: "100%",
        }
    }
}

/// Shows the iframe of one frontend component. The frontend process already exists
/// (the server spawned it before the tab was opened).
#[component]
pub fn PluginFrame(component: SpawnedComponent) -> Element {
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
                let entrypoint = PathBuf::from_iter([&plugin.plugin_folder_name, &frontend.entry_point_file_path]);
                let entrypoint = entrypoint.to_str().unwrap();

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
                        src: "/plugins/{entrypoint}?{query}",
                        // Injects the theme stylesheet and sets the page's theme once the plugin has loaded.
                        onload: move |_| relay::apply_theme(pid, workspace.theme().as_str()),
                        width: "100%",
                        height: "100%",
                        style: "border: none;",
                    }
                }
            }
        }
    }
}
