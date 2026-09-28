use std::path::PathBuf;
use dioxus::prelude::*;
use api::models::{Plugin, User};
use api::framework_web_api;

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