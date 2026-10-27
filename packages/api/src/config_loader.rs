//! The config files of Salus and of every plugin.
//!
//! Everything lives under the platform's config directory (`directories::ProjectDirs`):
//!
//! ```text
//! <config root>/salus.toml              the settings of Salus
//! <config root>/plugins/<plugin>.toml   the settings of one plugin (<plugin> is its folder name in the plugins directory)
//! ```
//!
//! The config root is the config directory, or its `config` subfolder where the config and data
//! directories coincide (macOS), so it never mixes with the installed plugins in `<data dir>/plugins`.
//!
//! Each plugin has exactly one config file. *What* may be in it is defined by the plugin's configurable
//! (see `configurable`), which is never written to: the values are checked against it on every change, and
//! settings that are not in the file read as their default. Salus owns the file; a plugin's backend and
//! frontend read and change their values through the framework (`salus://config/…` and `/plugin-config/…`).

use crate::configurable::Configurable;
use crate::models::Plugin;
use directories::ProjectDirs;
use log::{info, warn};
use serde_json::{Map, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

const FRAMEWORK_CONFIG_FILE: &str = "salus.toml";
const SALUS_CONFIGURABLE: &str = include_str!("salus.configurable.toml");

/// The settings of Salus itself, as the typed values the rest of the server uses.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameworkConfig {
    pub plugins_dir: Option<PathBuf>,
}

/// What can be configured in Salus (`salus.configurable.toml`, embedded in the server).
pub fn salus_configurable() -> &'static Configurable {
    static CONFIGURABLE: OnceLock<Configurable> = OnceLock::new();
    CONFIGURABLE.get_or_init(|| Configurable::parse(SALUS_CONFIGURABLE).expect("salus.configurable.toml is valid"))
}

fn framework_config_from(values: &Map<String, Value>) -> FrameworkConfig {
    FrameworkConfig {
        plugins_dir: values.get("plugins_dir").and_then(Value::as_str).filter(|dir| !dir.is_empty()).map(PathBuf::from),
    }
}

/// The framework configuration, read once (changes to `salus.toml` apply after a restart). A missing or
/// unreadable file means defaults, so a typo cannot keep the server from starting.
pub fn framework_config() -> &'static FrameworkConfig {
    static CONFIG: OnceLock<FrameworkConfig> = OnceLock::new();
    CONFIG.get_or_init(|| framework_config_from(&load_effective(&framework_config_path(), salus_configurable())))
}

/// Creates the config directories on first start, then loads the framework configuration.
pub fn init() {
    let root = config_root();
    if let Err(e) = fs::create_dir_all(root.join("plugins")) {
        warn!("Cannot create config directory {}: {e}", root.display());
        return;
    }
    let path = framework_config_path();
    match create_default_config(&path, salus_configurable()) {
        Ok(true) => info!("Created {}", path.display()),
        Ok(false) => {}
        Err(e) => warn!("Cannot create {}: {e}", path.display()),
    }
    info!("Framework configuration: {}", path.display());
    framework_config();
}

/// The text of a config file for a configurable that has nothing set: every setting as a comment with its explanation and
/// default, so the file shows what can be configured. (Salus rewrites the file when a setting changes, which drops the comments.)
fn default_config_text(configurable: &Configurable) -> String {
    let mut text = String::from("# Settings of Salus. Everything is commented out, so every setting has its default.\n# Change them in the Salus settings, or uncomment a line here and restart Salus.\n");
    for setting in &configurable.settings {
        text.push_str(&format!("\n# {}\n", setting.name));
        for line in setting.description.lines() {
            text.push_str(&format!("# {line}\n"));
        }
        let default = setting.default.as_ref().map_or_else(|| "...".to_string(), |value| {
            toml::Value::try_from(value).map_or_else(|_| value.to_string(), |value| value.to_string())
        });
        text.push_str(&format!("# {} = {}\n", setting.key, default));
    }
    text
}

/// Writes the default config file if there is none. Returns whether it was created; an existing file is never touched.
fn create_default_config(path: &Path, configurable: &Configurable) -> io::Result<bool> {
    match fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            io::Write::write_all(&mut file, default_config_text(configurable).as_bytes())?;
            Ok(true)
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e),
    }
}

pub fn config_root() -> PathBuf {
    let dirs = ProjectDirs::from("org", "fredericmrozinski", "salus").expect("could not determine home directory");
    config_root_for(dirs.config_dir(), dirs.data_dir())
}

/// On macOS the config and data directories are the same. The plugin installations live in
/// `<data dir>/plugins`, so the configuration goes into a `config` folder of its own there.
fn config_root_for(config_dir: &Path, data_dir: &Path) -> PathBuf {
    if config_dir == data_dir { config_dir.join("config") } else { config_dir.to_path_buf() }
}

/// The id of the theme Salus shows (`theme` in `salus.toml`): the `data-theme` of `theme.css`. Read on every call,
/// so a change in the settings applies without a restart.
pub fn theme() -> String {
    load_effective(&framework_config_path(), salus_configurable())
        .get("theme")
        .and_then(Value::as_str)
        .unwrap_or("dark")
        .to_string()
}

/// Where plugins are installed: `plugins_dir` from `salus.toml`, else the `plugins` folder of the data directory.
pub fn plugins_dir() -> PathBuf {
    match &framework_config().plugins_dir {
        Some(dir) => config_root().join(dir), // an absolute `dir` replaces the root
        None => ProjectDirs::from("org", "fredericmrozinski", "salus")
            .expect("could not determine home directory")
            .data_dir()
            .join("plugins"),
    }
}

pub fn framework_config_path() -> PathBuf {
    config_root().join(FRAMEWORK_CONFIG_FILE)
}

/// The config file of a plugin. `identifier` is the plugin's folder name.
pub fn plugin_config_path(identifier: &str) -> io::Result<PathBuf> {
    let valid = !identifier.is_empty()
        && identifier != "."
        && identifier != ".."
        && !identifier.contains(['/', '\\', '\0', ':']);
    if !valid {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("invalid plugin identifier {identifier:?}")));
    }
    Ok(config_root().join("plugins").join(format!("{identifier}.toml")))
}

// ============================================================================
// Reading and changing a config file
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The change was refused: unknown setting, or a value of the wrong type or outside its limits.
    #[error("{0}")]
    Invalid(String),
    #[error("the config file cannot be read: {0}")]
    Corrupt(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The values stored in a config file; a file that does not exist yet holds none.
pub fn read_stored(path: &Path) -> Result<Map<String, Value>, ConfigError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(e.into()),
    };
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| ConfigError::Corrupt(e.to_string()))?;
    match serde_json::to_value(table) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(ConfigError::Corrupt("not a table".into())),
    }
}

/// The values in effect (stored values and defaults). A file that cannot be read counts as empty.
pub fn load_effective(path: &Path, configurable: &Configurable) -> Map<String, Value> {
    match read_stored(path) {
        Ok(stored) => configurable.effective(&stored),
        Err(e) => {
            warn!("Ignoring {}: {e}", path.display());
            configurable.effective(&Map::new())
        }
    }
}

/// Applies `updates` (a value sets, `null` resets to the default) and returns the values then in effect.
/// A config file that is unreadable is not overwritten.
pub fn update(path: &Path, configurable: &Configurable, updates: &Map<String, Value>) -> Result<Map<String, Value>, ConfigError> {
    static WRITE_LOCK: Mutex<()> = Mutex::new(());
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let stored = read_stored(path)?;
    let changed = configurable.apply(&stored, updates).map_err(ConfigError::Invalid)?;
    if changed != stored {
        write_stored(path, &changed)?;
    }
    Ok(configurable.effective(&changed))
}

/// The settings of a plugin: its config file and what may be in it. A plugin without a configurable has no settings.
fn plugin_settings(plugin: &Plugin) -> Result<(PathBuf, Configurable), ConfigError> {
    Ok((plugin_config_path(&plugin.identifier())?, plugin.configurable.clone().unwrap_or_default()))
}

/// The values in effect for a plugin.
pub fn plugin_values(plugin: &Plugin) -> Result<Map<String, Value>, ConfigError> {
    let (path, configurable) = plugin_settings(plugin)?;
    Ok(load_effective(&path, &configurable))
}

pub fn update_plugin_values(plugin: &Plugin, updates: &Map<String, Value>) -> Result<Map<String, Value>, ConfigError> {
    let (path, configurable) = plugin_settings(plugin)?;
    update(&path, &configurable, updates)
}

/// Names of the settings that are set explicitly in a config file (the others are at their default).
pub fn stored_keys(path: &Path) -> Vec<String> {
    read_stored(path).map(|stored| stored.keys().cloned().collect()).unwrap_or_default()
}

pub fn plugin_stored_keys(plugin: &Plugin) -> Vec<String> {
    plugin_config_path(&plugin.identifier()).map(|path| stored_keys(&path)).unwrap_or_default()
}

/// Writes atomically: readers see the old or the new file, never a half-written one.
fn write_stored(path: &Path, values: &Map<String, Value>) -> Result<(), ConfigError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let text = toml::to_string_pretty(values).map_err(|e| ConfigError::Invalid(e.to_string()))?;
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent directory"))?;
    fs::create_dir_all(parent)?;
    let name = path.file_name().unwrap().to_string_lossy();
    let temp = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
    let result = fs::write(&temp, text).and_then(|_| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    Ok(result?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("salus-config-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> Configurable {
        Configurable::parse(
            "[[setting]]\nkey = \"zoom\"\nname = \"Zoom\"\ntype = \"float\"\ndefault = 1.5\n\
             [[setting]]\nkey = \"count\"\nname = \"Count\"\ntype = \"int\"\n\
             [[setting]]\nkey = \"on\"\nname = \"On\"\ntype = \"bool\"\ndefault = true\n",
        )
        .unwrap()
    }

    fn updates(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn update_stores_validates_and_resets() {
        let path = temp_dir("update").join("plugins").join("p.toml");
        let c = sample();
        assert_eq!(Value::Object(load_effective(&path, &c)), json!({"zoom": 1.5, "on": true}));

        let values = update(&path, &c, &updates(json!({"count": 3, "zoom": 2.0}))).unwrap();
        assert_eq!(Value::Object(values), json!({"count": 3, "zoom": 2.0, "on": true}));
        assert_eq!(Value::Object(load_effective(&path, &c)), json!({"count": 3, "zoom": 2.0, "on": true}));
        assert!(fs::read_to_string(&path).unwrap().contains("zoom = 2.0"));

        assert!(matches!(update(&path, &c, &updates(json!({"count": "x"}))), Err(ConfigError::Invalid(_))));
        assert!(matches!(update(&path, &c, &updates(json!({"other": 1}))), Err(ConfigError::Invalid(_))));
        assert_eq!(read_stored(&path).unwrap().len(), 2, "a refused update changes nothing");

        let values = update(&path, &c, &updates(json!({"zoom": null}))).unwrap();
        assert_eq!(values["zoom"], 1.5);
        assert!(!read_stored(&path).unwrap().contains_key("zoom"));
        let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(leftovers.len(), 1, "no temp files are left behind");
    }

    #[test]
    fn a_missing_framework_config_is_created_and_an_existing_one_is_left_alone() {
        let dir = temp_dir("default");
        let path = dir.join("config").join("salus.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let c = salus_configurable();

        assert!(create_default_config(&path, c).unwrap());
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# theme = \"dark\""), "{text}");
        assert!(text.contains("# plugins_dir = \"\""), "{text}");
        assert!(read_stored(&path).unwrap().is_empty(), "everything is commented out");
        assert_eq!(Value::Object(load_effective(&path, c)), Value::Object(c.effective(&Map::new())));

        fs::write(&path, "theme = \"light\"\n").unwrap();
        assert!(!create_default_config(&path, c).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "theme = \"light\"\n");

        // the first change through the settings keeps working on the template
        fs::write(&path, text).unwrap();
        let values = update(&path, c, &updates(json!({"theme": "midnight"}))).unwrap();
        assert_eq!(values["theme"], "midnight");
    }

    #[test]
    fn an_unreadable_file_is_not_overwritten() {
        let path = temp_dir("corrupt").join("p.toml");
        fs::write(&path, "this is = = not toml").unwrap();
        let c = sample();
        assert_eq!(Value::Object(load_effective(&path, &c)), json!({"zoom": 1.5, "on": true}));
        assert!(matches!(update(&path, &c, &updates(json!({"count": 1}))), Err(ConfigError::Corrupt(_))));
        assert_eq!(fs::read_to_string(&path).unwrap(), "this is = = not toml");
    }

    #[test]
    fn plugin_config_paths_cannot_leave_the_plugins_folder() {
        assert!(plugin_config_path("org.example.a").unwrap().ends_with("plugins/org.example.a.toml"));
        for bad in ["", ".", "..", "a/b", "a\\b", "c:x", "a\0b"] {
            assert!(plugin_config_path(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn config_root_stays_apart_from_the_data_dir() {
        let same = Path::new("/home/u/Library/Application Support/salus");
        assert_eq!(config_root_for(same, same), same.join("config"));
        assert_eq!(config_root_for(Path::new("/home/u/.config/salus"), Path::new("/home/u/.local/share/salus")), Path::new("/home/u/.config/salus"));
    }

    #[test]
    fn every_theme_of_the_settings_is_defined_in_the_stylesheet() {
        let css = include_str!("../../web/assets/theme.css");
        let mut defined: Vec<&str> = css.split("data-theme=\"").skip(1).filter_map(|rest| rest.split('"').next()).collect();
        defined.sort();
        defined.dedup();
        let mut offered = salus_configurable().setting("theme").unwrap().choices.iter().map(String::as_str).collect::<Vec<_>>();
        offered.sort();
        assert_eq!(defined, offered, "theme.css and the choices of `theme` in salus.configurable.toml must list the same themes");
        assert_eq!(salus_configurable().setting("theme").unwrap().default, Some(serde_json::json!("dark")));
    }

    #[test]
    fn the_salus_configurable_is_valid_and_feeds_the_framework_config() {
        let c = salus_configurable();
        assert!(c.setting("plugins_dir").is_some());
        assert_eq!(framework_config_from(&c.effective(&Map::new())), FrameworkConfig::default());
        let values = c.effective(&updates(json!({"plugins_dir": "/opt/plugins"})));
        assert_eq!(framework_config_from(&values).plugins_dir, Some(PathBuf::from("/opt/plugins")));
        assert_eq!(Path::new("/root/config").join("/opt/plugins"), Path::new("/opt/plugins"));
    }
}
