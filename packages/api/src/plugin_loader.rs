use crate::configurable::Configurable;
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
    crate::config_loader::plugins_dir()
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
            if Path::new(&manifest_path_str).exists() {
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

const SUPPORTED_API_VERSION: i64 = 3;

fn parse_manifest(manifest: &str, plugin_folder_name: &str) -> Result<models::Plugin, ManifestError> {
    let parsed_manifest = manifest.parse::<Table>()?;

    if parsed_manifest.contains_key("frontend") {
        return Err(ManifestError::BadManifest(
            "'[frontend]' no longer exists: declare '[[frontend-component]]' tables (api-version 3).".to_string()));
    }

    // Parse meta information
    require_manifest_keys(&parsed_manifest, "meta", &vec!["api-version"])?;
    let plugin_api_version = parsed_manifest["meta"]["api-version"].as_integer()
        .ok_or(ManifestError::BadManifest("api-version needs to be an integer value".to_string()))?;
    if plugin_api_version != SUPPORTED_API_VERSION {
        return Err(ManifestError::BadManifest(format!(
            "unsupported api-version {plugin_api_version}: this Salus only supports api-version {SUPPORTED_API_VERSION}.")));
    }
    let entry_component = match parsed_manifest["meta"].as_table().and_then(|meta| meta.get("entry-component")) {
        None => None,
        Some(value) => Some(value.as_str().ok_or(ManifestError::BadManifest(
            "[meta]: 'entry-component' needs to carry a string value (a component-name).".to_string()))?.to_string()),
    };
    let plugin_meta_data = models::PluginMetaData {
        api_version: SUPPORTED_API_VERSION as u8,
        entry_component: entry_component.clone(),
    };

    // Parse plugin description
    require_manifest_keys(&parsed_manifest, "description",
                          &vec!["name", "description", "developer", "contact", "version"])?;
    let plugin_description = models::PluginDescription {
        name: parse_str(&parsed_manifest, "description", "name")?,
        description: parse_str(&parsed_manifest, "description", "description")?,
        developer: parse_str(&parsed_manifest, "description", "developer")?,
        contact: parse_str(&parsed_manifest, "description", "contact")?,
        version: parse_str(&parsed_manifest, "description", "version")?,
        icon: parse_icon(&parsed_manifest)?,
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

    let frontend_components = parse_frontend_components(&parsed_manifest)?;
    if let Some(entry) = &entry_component {
        if !frontend_components.iter().any(|component| &component.name == entry) {
            return Err(ManifestError::BadManifest(format!(
                "[meta]: 'entry-component' names '{entry}', which is not a declared component-name.")));
        }
    }

    let dependencies = parse_dependencies(&parsed_manifest)?;
    let configurable = parse_configurable(&parsed_manifest, plugin_folder_name)?;

    Ok(models::Plugin {
        manifest: models::PluginManifest {
            meta_data: plugin_meta_data,
            description: plugin_description,
            backend_specs: plugin_backend_specs,
            frontend_components,
            dependencies,
        },
        plugin_folder_name: plugin_folder_name.to_string(),
        configurable,
    })
}

/// Optional `[config]` table: `configurable = "configurable.toml"` names the file (inside the plugin folder)
/// that defines the plugin's settings. The file is read now, so a broken one stops the plugin from loading.
fn parse_configurable(manifest: &Table, plugin_folder: &str) -> Result<Option<Configurable>, ManifestError> {
    let Some(config) = manifest.get("config") else {
        return Ok(None);
    };
    let config = config.as_table().ok_or_else(|| bad("'[config]' must be a table.".to_string()))?;
    if let Some(unknown) = config.keys().find(|key| key.as_str() != "configurable") {
        return Err(bad(format!("[config]: unknown key '{unknown}'.")));
    }
    let Some(file) = config.get("configurable") else {
        return Ok(None);
    };
    let file = file.as_str().ok_or_else(|| bad("[config]: 'configurable' needs to carry a string value.".to_string()))?;
    if !Path::new(file).components().all(|c| matches!(c, std::path::Component::Normal(_))) {
        return Err(bad(format!(
            "[config]: configurable '{file}' must be a path inside the plugin folder (relative, without '..').")));
    }
    let path = Path::new(plugin_folder).join(file);
    let text = fs::read_to_string(&path)
        .map_err(|e| bad(format!("[config]: cannot read configurable '{}': {e}", path.display())))?;
    Configurable::parse(&text).map(Some).map_err(|e| bad(format!("{}: {e}", path.display())))
}

/// Optional `icon` of `[description]`: a png or svg file inside the plugin folder.
fn parse_icon(manifest: &Table) -> Result<Option<String>, ManifestError> {
    let Some(value) = manifest["description"].as_table().and_then(|table| table.get("icon")) else {
        return Ok(None);
    };
    let icon = value.as_str()
        .ok_or_else(|| bad("[description]: 'icon' needs to carry a string value.".to_string()))?;
    let path = Path::new(icon);
    let inside_folder = path.components().all(|c| matches!(c, std::path::Component::Normal(_)));
    let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase());
    if !inside_folder || !matches!(extension.as_deref(), Some("png" | "svg")) {
        return Err(bad(format!(
            "[description]: icon '{icon}' must be a .png or .svg file inside the plugin folder (a relative path without '..').")));
    }
    Ok(Some(icon.to_string()))
}

fn bad(message: String) -> ManifestError {
    ManifestError::BadManifest(message)
}

/// `[[frontend-component]]` tables: at least one, unique valid names.
fn parse_frontend_components(manifest: &Table) -> Result<Vec<models::FrontendComponent>, ManifestError> {
    let entries = manifest
        .get("frontend-component")
        .and_then(|value| value.as_array())
        .ok_or_else(|| bad("at least one '[[frontend-component]]' table is required.".to_string()))?;

    let mut components: Vec<models::FrontendComponent> = Vec::new();
    for entry in entries {
        let table = entry.as_table().ok_or_else(|| bad("'frontend-component' entries must be tables.".to_string()))?;
        let field = |key: &str| -> Result<String, ManifestError> {
            table.get(key).and_then(|value| value.as_str()).map(str::to_string)
                .ok_or_else(|| bad(format!("[[frontend-component]]: '{key}' is required and needs to carry a string value.")))
        };

        let name = field("component-name")?;
        let valid_name = !name.is_empty()
            && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if !valid_name {
            return Err(bad(format!("[[frontend-component]]: component-name '{name}' must match [a-z0-9_-]+.")));
        }
        if components.iter().any(|existing| existing.name == name) {
            return Err(bad(format!("[[frontend-component]]: component-name '{name}' is declared twice.")));
        }

        let panel = field("target-panel")?;
        let target_panel = models::TargetPanel::parse(&panel).ok_or_else(|| bad(format!(
            "[[frontend-component]] '{name}': target-panel '{panel}' must be one of left, center, right, bottom.")))?;

        let title = match table.get("title") {
            None => None,
            Some(value) => Some(value.as_str().map(str::to_string)
                .ok_or_else(|| bad(format!("[[frontend-component]] '{name}': 'title' needs to carry a string value.")))?),
        };

        let mut file_viewer_for = Vec::new();
        if let Some(value) = table.get("file_viewer_for") {
            let list = value.as_array().ok_or_else(|| bad(format!(
                "[[frontend-component]] '{name}': 'file_viewer_for' needs to be an array of extensions.")))?;
            for extension in list {
                let extension = extension.as_str().ok_or_else(|| bad(format!(
                    "[[frontend-component]] '{name}': 'file_viewer_for' needs to be an array of strings.")))?;
                if extension.is_empty() || extension.starts_with('.') || extension != extension.to_lowercase() {
                    return Err(bad(format!(
                        "[[frontend-component]] '{name}': extension '{extension}' must be lowercase without a leading dot.")));
                }
                file_viewer_for.push(extension.to_string());
            }
        }

        components.push(models::FrontendComponent {
            name,
            title,
            entry_point_file_path: field("entrypoint")?,
            target_panel,
            file_viewer_for,
        });
    }

    if components.is_empty() {
        return Err(bad("at least one '[[frontend-component]]' table is required.".to_string()));
    }
    Ok(components)
}

/// Optional `[dependencies]` table: `plugins = ["org.example.other"]`.
fn parse_dependencies(manifest: &Table) -> Result<Vec<String>, ManifestError> {
    let Some(table) = manifest.get("dependencies") else {
        return Ok(Vec::new());
    };
    let table = table
        .as_table()
        .ok_or_else(|| ManifestError::BadManifest("'[dependencies]' must be a table.".to_string()))?;
    let Some(plugins) = table.get("plugins") else {
        return Ok(Vec::new());
    };
    plugins
        .as_array()
        .ok_or_else(|| ManifestError::BadManifest("[dependencies]: 'plugins' needs to be an array of strings.".to_string()))?
        .iter()
        .map(|entry| {
            entry.as_str().map(str::to_string).ok_or_else(|| {
                ManifestError::BadManifest("[dependencies]: 'plugins' needs to be an array of strings.".to_string())
            })
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = r#"
[description]
name = "N"
description = "D"
developer = "Dev"
contact = "c"
version = "1"

[backend]
entrypoint = "backend/plugin.py"
lifetime = "session"
"#;

    fn manifest(meta_extra: &str, rest: &str) -> String {
        format!("[meta]\napi-version = 3\n{meta_extra}\n{HEADER}\n{rest}")
    }

    const MAIN: &str = r#"
[[frontend-component]]
component-name = "main"
entrypoint = "frontend/index.html"
target-panel = "center"
"#;

    #[test]
    fn single_component_with_defaults() {
        let plugin = parse_manifest(&manifest("", MAIN), "/plugins/org.example.a").unwrap();
        assert_eq!(plugin.identifier(), "org.example.a");
        assert!(plugin.manifest.dependencies.is_empty());
        assert!(plugin.entry_component().is_none());
        let component = plugin.component("main").unwrap();
        assert_eq!(component.target_panel, models::TargetPanel::Center);
        assert_eq!(component.title, None);
        assert!(component.file_viewer_for.is_empty());
    }

    #[test]
    fn configurable_is_read_from_the_plugin_folder() {
        let folder = std::env::temp_dir().join(format!("salus-loader-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("configurable.toml"),
            "[[setting]]\nkey = \"zoom\"\nname = \"Zoom\"\ntype = \"float\"\ndefault = 1.5\n").unwrap();
        std::fs::write(folder.join("broken.toml"), "[[setting]]\nkey = \"zoom\"\n").unwrap();
        let folder_name = folder.to_str().unwrap();

        let with = |file: &str| parse_manifest(&manifest("", &format!("{MAIN}\n[config]\nconfigurable = \"{file}\"\n")), folder_name);
        let plugin = with("configurable.toml").unwrap();
        assert_eq!(plugin.configurable.unwrap().setting("zoom").unwrap().default, Some(serde_json::json!(1.5)));
        assert!(with("missing.toml").is_err());
        assert!(with("broken.toml").is_err());
        assert!(with("../x.toml").is_err());
        assert!(parse_manifest(&manifest("", MAIN), folder_name).unwrap().configurable.is_none());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_example_plugins_load() {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins");
        let mut loaded = 0;
        for entry in std::fs::read_dir(examples).unwrap() {
            let folder = entry.unwrap().path();
            let Ok(text) = std::fs::read_to_string(folder.join("manifest.toml")) else { continue };
            let plugin = parse_manifest(&text, folder.to_str().unwrap())
                .unwrap_or_else(|e| panic!("{}: {e}", folder.display()));
            if let Some(icon) = &plugin.manifest.description.icon {
                assert!(folder.join(icon).is_file(), "{}: icon {icon} is missing", folder.display());
            }
            loaded += 1;
        }
        assert!(loaded >= 4, "found only {loaded} example plugins");
    }

    #[test]
    fn icon_is_optional_and_checked() {
        let plugin = parse_manifest(&manifest("", MAIN), "/plugins/x").unwrap();
        assert_eq!(plugin.manifest.description.icon, None);

        let with_icon = |icon: &str| {
            let text = manifest("", MAIN).replace("name = \"N\"", &format!("name = \"N\"\nicon = \"{icon}\""));
            parse_manifest(&text, "/plugins/x")
        };
        assert_eq!(with_icon("assets/icon.SVG").unwrap().manifest.description.icon.as_deref(), Some("assets/icon.SVG"));
        assert!(with_icon("icon.png").is_ok());
        assert!(with_icon("icon.jpg").is_err());
        assert!(with_icon("../icon.png").is_err());
        assert!(with_icon("/etc/icon.png").is_err());
    }

    #[test]
    fn several_components_entry_title_and_file_viewers() {
        let rest = r#"
[[frontend-component]]
component-name = "browser"
title = "Browser"
entrypoint = "frontend/browser/index.html"
target-panel = "left"

[[frontend-component]]
component-name = "viewer"
entrypoint = "frontend/viewer/index.html"
target-panel = "right"
file_viewer_for = ["svs", "tif"]
"#;
        let plugin = parse_manifest(&manifest("entry-component = \"browser\"", rest), "/plugins/x").unwrap();
        assert_eq!(plugin.manifest.frontend_components.len(), 2);
        assert_eq!(plugin.entry_component().unwrap().name, "browser");
        assert_eq!(plugin.component("browser").unwrap().title.as_deref(), Some("Browser"));
        assert_eq!(plugin.component("viewer").unwrap().file_viewer_for, vec!["svs", "tif"]);
        assert_eq!(plugin.component("viewer").unwrap().target_panel, models::TargetPanel::Right);
    }

    #[test]
    fn dependencies_are_parsed_and_checked() {
        let with = format!("{MAIN}\n[dependencies]\nplugins = [\"org.example.b\", \"org.example.c\"]\n");
        let plugin = parse_manifest(&manifest("", &with), "/plugins/x").unwrap();
        assert_eq!(plugin.manifest.dependencies, vec!["org.example.b", "org.example.c"]);
        assert!(parse_manifest(&manifest("", &format!("{MAIN}\n[dependencies]\nplugins = [1]\n")), "/p/x").is_err());
        assert!(parse_manifest(&manifest("", &format!("{MAIN}\n[dependencies]\nplugins = \"b\"\n")), "/p/x").is_err());
    }

    #[test]
    fn old_manifests_are_rejected() {
        let v2 = format!("[meta]\napi-version = 2\n{HEADER}\n{MAIN}");
        assert!(parse_manifest(&v2, "/p/x").unwrap_err().to_string().contains("unsupported api-version 2"));
        let legacy = manifest("", "[frontend]\nentrypoint = \"frontend/index.html\"\ntarget-panel = \"center\"\n");
        assert!(parse_manifest(&legacy, "/p/x").unwrap_err().to_string().contains("[[frontend-component]]"));
    }

    #[test]
    fn component_rules_are_enforced() {
        let ok = |rest: &str| parse_manifest(&manifest("", rest), "/p/x");
        assert!(ok("").is_err()); // no component
        assert!(ok(&format!("{MAIN}{MAIN}")).is_err()); // duplicate name
        assert!(ok(&MAIN.replace("\"main\"", "\"Main!\"")).is_err()); // bad name
        assert!(ok(&MAIN.replace("center", "middle")).is_err()); // bad panel
        assert!(ok(&format!("{MAIN}file_viewer_for = [\".svs\"]\n")).is_err()); // leading dot
        assert!(ok(&format!("{MAIN}file_viewer_for = [\"SVS\"]\n")).is_err()); // uppercase
        assert!(parse_manifest(&manifest("entry-component = \"nope\"", MAIN), "/p/x").is_err()); // unknown entry
        assert!(parse_manifest(&manifest("entry-component = 3", MAIN), "/p/x").is_err());
    }
}
