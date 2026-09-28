use directories::ProjectDirs;
use crate::models;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use toml::Table;
use dioxus::prelude::*;
use log::info;

pub fn load_available_plugins() -> Vec<models::Plugin> {
    let root_path = get_plugins_root_path();
    build_plugins_root_if_not_exists(&root_path);
    read_available_plugins_from_disk(root_path.to_str().unwrap())
}

pub fn get_plugins_root_path() -> std::path::PathBuf {
    let dirs = ProjectDirs::from("org", "fredericmrozinski", "salus")
        .expect("could not determine home directory");
    dirs.data_dir().join("plugins")
}

fn build_plugins_root_if_not_exists(path_buf: &PathBuf) {

    if !path_buf.exists() {
        info!("Creating plugin root directory at: {}", path_buf.to_str().unwrap());
        std::fs::create_dir_all(path_buf).unwrap();
    } else {
        info!("Plugin root directory found at: {}", path_buf.to_str().unwrap());
    }
}

fn read_available_plugins_from_disk(plugin_dir: &str) -> Vec<models::Plugin> {
    let mut valid_plugins: Vec<models::Plugin> = Vec::new();

    info!{"Scanning for plugins from {}", plugin_dir};

    let paths = fs::read_dir(plugin_dir).unwrap();

    for path in paths {

        let path = path.unwrap().path();
        let is_dir = fs::metadata(&path).unwrap().is_dir();
        let path_str = path.display().to_string();
        let manifest_path_str = path.join("manifest.toml").display().to_string();

        if is_dir {
            if Path::new(&path_str).exists() {
                info!("manifest.toml exists for {}", &manifest_path_str);

                let manifest_content = fs::read_to_string(&manifest_path_str).unwrap();
                let parse_res = parse_manifest(&manifest_content, &path_str);

                if parse_res.is_err() {
                    error!("{}: {}", &manifest_path_str, parse_res.err().unwrap().to_string());
                } else {
                    info!("manifest.toml successfully read for {}", &manifest_path_str);
                    valid_plugins.push(parse_res.unwrap());
                }
            }
        }
    }

    valid_plugins
}


// ===== Parsing

#[derive(Error, Debug)]
enum ManifestError {
    #[error("TOML parsing error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Bad manifest definition: {0}")]
    BadManifest(String),
}

fn parse_manifest(manifest: &str, plugin_folder_name: &str) -> Result<models::Plugin, ManifestError> {
    parse_manifest_api_v2(manifest, plugin_folder_name)
}

fn parse_manifest_api_v2(manifest: &str, plugin_folder_name: &str) -> Result<models::Plugin, ManifestError> {
    let parsed_manifest = manifest.parse::<Table>()?;

    // Parse plugin description
    require_manifest_keys(&parsed_manifest, "description",
                          &vec!["name", "description", "developer", "contact", "version"])?;
    let plugin_description_name = parse_str(&parsed_manifest, "description", "name")?;
    let plugin_description_description = parse_str(&parsed_manifest, "description", "description")?;
    let plugin_description_developer = parse_str(&parsed_manifest, "description", "developer")?;
    let plugin_description_contact = parse_str(&parsed_manifest, "description", "contact")?;
    let plugin_description_version = parse_str(&parsed_manifest, "description", "version")?;
    let plugin_description = models::PluginDescription {
        name: plugin_description_name,
        description: plugin_description_description,
        developer: plugin_description_developer,
        contact: plugin_description_contact,
        version: plugin_description_version,
    };

    // Parse meta information
    require_manifest_keys(&parsed_manifest, "meta", &vec!["api-version"])?;
    let plugin_api_version = parsed_manifest["meta"]["api-version"].as_integer()
        .ok_or(ManifestError::BadManifest("api-version needs to be an integer value".to_string()))?;
    let plugin_api_version = u8::try_from(plugin_api_version)
        .map_err(|_| ManifestError::BadManifest("api-version needs to be in the range 0 - 255.".to_string()))?;
    let plugin_meta_data = models::PluginMetaData {
        api_version: plugin_api_version
    };

    // Parse backend information
    require_manifest_keys(&parsed_manifest, "backend", &vec!["entrypoint", "lifetime"])?;
    let plugin_backend_entrypoint = parse_str(&parsed_manifest, "backend", "entrypoint")?;
    let plugin_backend_lifetime = parse_str(&parsed_manifest, "backend", "lifetime")?;
    let lifetime = match plugin_backend_lifetime.as_str() {
        "panel" => Ok(models::BackendLifetime::Panel),
        "session" => Ok(models::BackendLifetime::Session(true)),
        "system" => Ok(models::BackendLifetime::System(true)),
        _ => Err(ManifestError::BadManifest("Invalid lifetime format for [backend]. \
        Choose 'panel', 'session', or 'system'.".to_string())),
    }?;
    let plugin_backend_specs = models::PluginBackendSpecification {
        lifetime,
        entry_point_file_path: plugin_backend_entrypoint
    };

    // Parse front information
    require_manifest_keys(&parsed_manifest, "frontend", &vec!["entrypoint", "target-panel"])?;
    let plugin_frontend_entrypoint = parse_str(&parsed_manifest, "frontend", "entrypoint")?;
    let plugin_target_panel = parse_str(&parsed_manifest, "frontend", "target-panel")?;
    let plugin_frontend_specs = models::PluginFrontendSpecification {
        entry_point_file_path: plugin_frontend_entrypoint
    };

    let plugin_manifest = models::PluginManifest {
        meta_data: plugin_meta_data,
        description: plugin_description,
        backend_specs: plugin_backend_specs,
        frontend_specs: plugin_frontend_specs
    };

    let plugin = models::Plugin {
        manifest: plugin_manifest,
        plugin_folder_name: plugin_folder_name.to_string(),
    };

    Ok(plugin)
}

fn parse_str(manifest: &Table, table: &str, key: &str) -> Result<String, ManifestError> {
    Ok(manifest[table][key].as_str().ok_or(
        ManifestError::BadManifest(format!("[{}]: '{}' needs to carry a string value.", table, key).to_string()))?.to_string())
}

fn require_manifest_keys(root: &Table, table_name: &str, keys: &Vec<&str>) -> Result<(), ManifestError> {
    let table = root[table_name]
        .as_table()
        .ok_or_else(|| ManifestError::BadManifest(format!("'[{}]' must exist as table in manifest.", table_name)))?;

    for key in keys.iter() {
        if !table.contains_key(*key) {
            return Err(ManifestError::BadManifest(format!("[{}] must contain key '{}'", table_name, key)));
        }
    }

    Ok(())
}

