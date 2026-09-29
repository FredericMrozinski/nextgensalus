use std::path::PathBuf;
use super::state::TabContent;
use dioxus::prelude::*;
use api::framework_web_api;
use api::models::User;

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
/// the extension point for wiring up real plugins later: add a variant to
/// `TabContent` and a match arm here (e.g. routing to the existing,
/// backend-wired `plugin_frame::PluginFrame`) without touching `Panel` or
/// `TabBar`.
#[component]
pub fn TabContentView(content: TabContent) -> Element {
    match content {
        TabContent::SamplePlugin => rsx! { SamplePluginFrame {} },
        TabContent::Plugin(id, _description) => rsx! { PluginFrame{ plugin_id: id } },
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

#[component]
pub fn PluginFrame(plugin_id: u32) -> Element {

    let plugin = use_resource(move || async move {
        framework_web_api::get_plugin_from_id(plugin_id).await
    });
    let sample_user = use_hook(|| User {user_id: 42});
    let fe_pid = use_resource(move || {
        let sample_user = sample_user.clone();

        async move {
            framework_web_api::spawn_frontend_plugin_process(plugin_id, sample_user).await
        }
    });

    rsx! {
        match &*plugin.read() {
            None => {
                rsx! { div { "Loading..." } }
            }
            Some(Err(err)) => {
                debug!("{}", err);
                rsx! { div { "ERROR: Invalid plugin id." } }
            }
            Some(Ok(plugin)) => {
                match &*fe_pid.read() {
                    None => { rsx! { div { "Awaiting scheduling..." }} }
                    Some(Err(err)) => { rsx! { div { "ERROR: Failed to spawn frontend process." } } }
                    Some(Ok(pid)) => {
                        match plugin {
                            None => { rsx! { div { "Invalid plugin id." } }}
                            Some(_plugin) => {
                                let tmp = PathBuf::from_iter([
                                    &_plugin.plugin_folder_name,
                                    &_plugin.manifest.frontend_specs.entry_point_file_path,
                                ]);
                                let fe_entrypoint = tmp.to_str().unwrap();

                                rsx! {
                                    div { "{fe_entrypoint}" }

                                    iframe {
                                        id: pid,
                                        src: "/plugins/{fe_entrypoint}?fe_process_id={pid}",
                                        width: "100%",
                                        height: "800px",
                                        style: "border: none;",
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
