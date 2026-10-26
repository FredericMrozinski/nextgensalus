//! Configuration directories of the framework and of every plugin.
//!
//! Everything lives under the platform's config directory (`directories::ProjectDirs`):
//!
//! ```text
//! <config root>/salus.toml            framework configuration
//! <config root>/plugins/<plugin>/     one directory per plugin (its folder name in the plugins directory)
//! ```
//!
//! The config root is the config directory, or its `config` subfolder where the config and data
//! directories coincide (macOS), so it never mixes with the installed plugins in `<data dir>/plugins`.
//!
//! A plugin's backend reads and writes its directory directly (its path is passed in `SALUS_CONFIG_DIR`);
//! its frontend goes through `plugin_config_api`. Both end up in the functions of this module, which refuse
//! every path that would leave the plugin's directory.

use directories::ProjectDirs;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

const FRAMEWORK_CONFIG_FILE: &str = "salus.toml";
const DEFAULT_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

const DEFAULT_FRAMEWORK_CONFIG: &str = "\
# Salus framework configuration. Every setting is optional; the values below are the defaults.

# Largest file a plugin may write into its config directory through the frontend SDK, in bytes.
# plugin_config_max_file_bytes = 8388608

# Directory the plugins are installed in. A relative path is taken relative to this file's directory.
# Defaults to the \"plugins\" folder of the platform's data directory.
# plugins_dir = \"/path/to/plugins\"
";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct FrameworkConfig {
    pub plugin_config_max_file_bytes: u64,
    pub plugins_dir: Option<PathBuf>,
}

impl Default for FrameworkConfig {
    fn default() -> Self {
        Self { plugin_config_max_file_bytes: DEFAULT_MAX_FILE_BYTES, plugins_dir: None }
    }
}

/// The framework configuration, read once. A missing file means defaults; an unreadable one is reported
/// and also means defaults, so a typo cannot keep the server from starting.
pub fn framework_config() -> &'static FrameworkConfig {
    static CONFIG: OnceLock<FrameworkConfig> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let path = framework_config_path();
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                warn!("Ignoring {}: {e}", path.display());
                FrameworkConfig::default()
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => FrameworkConfig::default(),
            Err(e) => {
                warn!("Cannot read {}: {e}", path.display());
                FrameworkConfig::default()
            }
        }
    })
}

/// Creates the config directory and a commented `salus.toml` on first start, then loads the configuration.
pub fn init() {
    let root = config_root();
    if let Err(e) = fs::create_dir_all(root.join("plugins")) {
        warn!("Cannot create config directory {}: {e}", root.display());
        return;
    }
    let path = framework_config_path();
    if !path.exists() {
        if let Err(e) = fs::write(&path, DEFAULT_FRAMEWORK_CONFIG) {
            warn!("Cannot write {}: {e}", path.display());
        }
    }
    info!("Framework configuration: {}", path.display());
    framework_config();
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

/// The config directory of a plugin, created if needed. `identifier` is the plugin's folder name.
pub fn plugin_config_dir(identifier: &str) -> io::Result<PathBuf> {
    check_component(identifier).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
    let dir = config_root().join("plugins").join(identifier);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

// ============================================================================
// Files inside a plugin's directory
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid path: {0}")]
    InvalidPath(&'static str),
    #[error("not found")]
    NotFound,
    #[error("file is larger than the allowed {0} bytes")]
    TooLarge(u64),
    #[error(transparent)]
    Io(io::Error),
}

impl From<io::Error> for ConfigError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::NotFound { ConfigError::NotFound } else { ConfigError::Io(e) }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConfigEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

fn check_component(part: &str) -> Result<(), ConfigError> {
    if part.is_empty() || part == "." || part == ".." {
        return Err(ConfigError::InvalidPath("empty, '.' or '..' path segment"));
    }
    if part.contains(['\\', '\0', ':']) {
        return Err(ConfigError::InvalidPath("path segments must not contain '\\', ':' or NUL"));
    }
    Ok(())
}

/// Maps `/`-separated `relative` (no leading slash; `""` is the directory itself) to a path below `dir`.
fn resolve(dir: &Path, relative: &str) -> Result<PathBuf, ConfigError> {
    let mut path = dir.to_path_buf();
    if !relative.is_empty() {
        for part in relative.split('/') {
            check_component(part)?;
            path.push(part);
        }
    }
    // A symlink inside the directory must not lead out of it: check where the deepest existing part really is.
    let mut existing = path.as_path();
    while !existing.exists() {
        match existing.parent() {
            Some(parent) => existing = parent,
            None => break,
        }
    }
    let root = dir.canonicalize()?;
    if !existing.canonicalize()?.starts_with(&root) {
        return Err(ConfigError::InvalidPath("path leaves the config directory"));
    }
    Ok(path)
}

pub fn read(dir: &Path, relative: &str) -> Result<Vec<u8>, ConfigError> {
    let path = resolve(dir, relative)?;
    if path.is_dir() {
        return Err(ConfigError::InvalidPath("is a directory"));
    }
    Ok(fs::read(path)?)
}

/// Writes atomically: readers see the old or the new content, never a half-written file.
pub fn write(dir: &Path, relative: &str, data: &[u8], max_bytes: u64) -> Result<(), ConfigError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    if relative.is_empty() {
        return Err(ConfigError::InvalidPath("a file name is required"));
    }
    if data.len() as u64 > max_bytes {
        return Err(ConfigError::TooLarge(max_bytes));
    }
    let path = resolve(dir, relative)?;
    if path.is_dir() {
        return Err(ConfigError::InvalidPath("is a directory"));
    }
    let parent = path.parent().ok_or(ConfigError::InvalidPath("a file name is required"))?;
    fs::create_dir_all(parent)?;
    let name = path.file_name().unwrap().to_string_lossy();
    let temp = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
    let result = fs::write(&temp, data).and_then(|_| fs::rename(&temp, &path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    Ok(result?)
}

/// Deletes a file, or a directory with everything in it.
pub fn delete(dir: &Path, relative: &str) -> Result<(), ConfigError> {
    if relative.is_empty() {
        return Err(ConfigError::InvalidPath("cannot delete the config directory itself"));
    }
    let path = resolve(dir, relative)?;
    if path.is_dir() { Ok(fs::remove_dir_all(path)?) } else { Ok(fs::remove_file(path)?) }
}

/// The entries of a directory, sorted by name. A directory that does not exist yet is empty.
pub fn list(dir: &Path, relative: &str) -> Result<Vec<ConfigEntry>, ConfigError> {
    let path = resolve(dir, relative)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    if !path.is_dir() {
        return Err(ConfigError::InvalidPath("not a directory"));
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".tmp") && name.starts_with('.') {
            continue; // a write in progress
        }
        let meta = entry.metadata()?;
        entries.push(ConfigEntry { name, is_dir: meta.is_dir(), size: if meta.is_dir() { 0 } else { meta.len() } });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("salus-config-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_read_list_delete() {
        let dir = temp_dir("roundtrip");
        write(&dir, "a.json", b"{}", 100).unwrap();
        write(&dir, "sub/deep/b.txt", b"hello", 100).unwrap();
        assert_eq!(read(&dir, "sub/deep/b.txt").unwrap(), b"hello");
        write(&dir, "a.json", b"[1]", 100).unwrap();
        assert_eq!(read(&dir, "a.json").unwrap(), b"[1]");

        let names: Vec<_> = list(&dir, "").unwrap().into_iter().map(|e| (e.name, e.is_dir)).collect();
        assert_eq!(names, [("a.json".to_string(), false), ("sub".to_string(), true)]);
        assert_eq!(list(&dir, "missing").unwrap(), []);

        delete(&dir, "sub").unwrap();
        assert!(matches!(read(&dir, "sub/deep/b.txt"), Err(ConfigError::NotFound)));
        assert!(matches!(delete(&dir, "a.json"), Ok(())));
        assert!(matches!(delete(&dir, "a.json"), Err(ConfigError::NotFound)));
        assert!(list(&dir, "").unwrap().is_empty(), "no temp files are left behind");
    }

    #[test]
    fn rejects_paths_leaving_the_directory() {
        let dir = temp_dir("escape");
        for bad in ["../x", "a/../../x", "/etc/passwd", "a//b", "a/./b", ".", "..", "a\\b", "c:x", "a\0b"] {
            assert!(matches!(read(&dir, bad), Err(ConfigError::InvalidPath(_))), "{bad:?}");
            assert!(matches!(write(&dir, bad, b"x", 10), Err(ConfigError::InvalidPath(_))), "{bad:?}");
        }
        assert!(matches!(write(&dir, "", b"x", 10), Err(ConfigError::InvalidPath(_))));
        assert!(matches!(delete(&dir, ""), Err(ConfigError::InvalidPath(_))));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_lead_out() {
        let dir = temp_dir("symlink");
        let outside = temp_dir("symlink-outside");
        fs::write(outside.join("secret"), "s").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();
        assert!(matches!(read(&dir, "link/secret"), Err(ConfigError::InvalidPath(_))));
        assert!(matches!(write(&dir, "link/new", b"x", 10), Err(ConfigError::InvalidPath(_))));
        assert!(!outside.join("new").exists());
    }

    #[test]
    fn enforces_the_size_limit() {
        let dir = temp_dir("limit");
        assert!(matches!(write(&dir, "big", &[0; 11], 10), Err(ConfigError::TooLarge(10))));
        assert!(write(&dir, "ok", &[0; 10], 10).is_ok());
    }

    #[test]
    fn config_root_stays_apart_from_the_data_dir() {
        let same = Path::new("/home/u/Library/Application Support/salus");
        assert_eq!(config_root_for(same, same), same.join("config"));
        assert_eq!(config_root_for(Path::new("/home/u/.config/salus"), Path::new("/home/u/.local/share/salus")), Path::new("/home/u/.config/salus"));
    }

    #[test]
    fn framework_config_parses() {
        let c: FrameworkConfig = toml::from_str("plugin_config_max_file_bytes = 5").unwrap();
        assert_eq!(c.plugin_config_max_file_bytes, 5);
        assert_eq!(toml::from_str::<FrameworkConfig>("").unwrap(), FrameworkConfig::default());
        assert!(toml::from_str::<FrameworkConfig>("typo = 1").is_err());
        let c: FrameworkConfig = toml::from_str("plugins_dir = \"/opt/plugins\"").unwrap();
        assert_eq!(c.plugins_dir, Some(PathBuf::from("/opt/plugins")));
        assert_eq!(Path::new("/root/config").join("/opt/plugins"), Path::new("/opt/plugins"));
        assert!(toml::from_str::<FrameworkConfig>(DEFAULT_FRAMEWORK_CONFIG).is_ok());
    }
}
