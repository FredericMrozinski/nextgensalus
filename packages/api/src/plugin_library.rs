use crate::models;
use crate::models::{FileViewer, Plugin};
use crate::plugin_installer;
use crate::plugin_installer::{install, needs_install};
use crate::plugin_loader;
use dioxus::logger::tracing::field::debug;
use dioxus::prelude::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn plugins() -> &'static Mutex<HashMap<u32, Plugin>> {
    static PLUGINS: OnceLock<Mutex<HashMap<u32, Plugin>>> = OnceLock::new();
    PLUGINS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn init() {
    let _plugins = plugin_loader::load_available_plugins();

    // New vector instead of filtering `_plugins`: the loop takes ownership of each
    // plugin, so ready ones are moved over and failed installs are left out.
    let mut ready = Vec::new();
    for plugin in _plugins {
        if !plugin_installer::needs_install(&plugin) {
            ready.push(plugin);
        } else {
            match plugin_installer::install(&plugin) {
                Err(e) => error!("Install failed for plugin {}: {}", plugin.identifier(), e),
                Ok(()) => ready.push(plugin),
            }
        }
    }

    {
        let mut map = plugins().lock().unwrap();

        let mut next_id: u32 = 10000;
        for plugin in ready.iter() {
            map.insert(next_id, plugin.clone());
            next_id += 1;
        }
    }

    debug!("The following plugins are loaded into the runtime library:");
    debug!("======================== BEGIN ========================");

    debug!("{:#?}", get_plugins_vec());
    debug!("========================= END =========================");
}

pub fn get_plugins_vec() -> Vec<models::Plugin> {
    plugins().lock().unwrap().values().cloned().collect()
}

pub fn get_plugin_from_id(plugin_id: &u32) -> Option<Plugin> {
    let plugins = plugins().lock().unwrap();

    let res = plugins.get(&plugin_id).cloned();

    res
}

pub fn get_plugins() -> Vec<(u32, Plugin)> {
    let res: Vec<(u32, Plugin)> = plugins()
        .lock()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    res
}

/// Looks a plugin up by its identifier (the name of its folder).
pub fn get_plugin_by_identifier(identifier: &str) -> Option<(u32, Plugin)> {
    plugins()
        .lock()
        .unwrap()
        .iter()
        .find(|(_, plugin)| plugin.identifier() == identifier)
        .map(|(id, plugin)| (*id, plugin.clone()))
}

/// The installed plugins among `identifiers` that can be opened from the dock (they have an entry component), in the
/// order given and each once. Identifiers of plugins that are not installed, or that have no entry component, are skipped.
pub fn openable_plugins(identifiers: &[String]) -> Vec<u32> {
    let plugins = plugins().lock().unwrap();
    let mut ids: Vec<u32> = Vec::new();
    for identifier in identifiers {
        let found = plugins.iter().find(|(_, plugin)| plugin.identifier() == *identifier && plugin.entry_component().is_some());
        match found {
            Some((id, _)) if !ids.contains(id) => ids.push(*id),
            Some(_) => {}
            None => debug!("Skipping {identifier:?}: no such plugin installed, or it has no entry component."),
        }
    }
    ids
}

/// Components that can display files with the given extension (lowercase, without dot).
pub fn get_file_viewers(extension: &str) -> Vec<FileViewer> {
    let extension = extension.to_lowercase();
    let mut viewers: Vec<FileViewer> = plugins()
        .lock()
        .unwrap()
        .iter()
        .flat_map(|(plugin_id, plugin)| {
            plugin
                .manifest
                .frontend_components
                .iter()
                .filter(|component| component.file_viewer_for.contains(&extension))
                .map(|component| FileViewer {
                    plugin_id: *plugin_id,
                    plugin_name: plugin.manifest.description.name.clone(),
                    component_name: component.name.clone(),
                    title: component
                        .title
                        .clone()
                        .unwrap_or_else(|| plugin.manifest.description.name.clone()),
                })
                .collect::<Vec<_>>()
        })
        .collect();
    viewers.sort_by_key(|viewer| (viewer.plugin_id, viewer.component_name.clone()));
    viewers
}

#[cfg(test)]
pub fn insert_test_plugin(id: u32, plugin: Plugin) {
    plugins().lock().unwrap().insert(id, plugin);
}
