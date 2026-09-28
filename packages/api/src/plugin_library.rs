use std::collections::HashMap;
use crate::models;
use crate::plugin_loader;
use std::sync::{Mutex, OnceLock};
use dioxus::logger::tracing::field::debug;
use dioxus::prelude::*;
use crate::models::Plugin;

fn plugins() -> &'static Mutex<HashMap<u32, Plugin>> {
    static PLUGINS: OnceLock<Mutex<HashMap<u32, Plugin>>> = OnceLock::new();
    PLUGINS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn init() {
    let _plugins = plugin_loader::load_available_plugins();

    {
        let mut map = plugins().lock().unwrap();

        let mut next_id: u32 = 10000;
        for plugin in _plugins.iter() {
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