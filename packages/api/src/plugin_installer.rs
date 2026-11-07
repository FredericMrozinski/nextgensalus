//! This module has 2 functions:
//! -one to check whether an installation has ever previously taken place (if an install-info.txt file even exists)
//! or if the plugin has since been updates by the developer, in which case the version field of the manifest will have changed
//!
//! -the second for the actual installation process, if the first function returns true

use crate::models::Plugin;
use log::info;
use std::fs;
use std::path::Path;
use std::process::Command;
use thiserror::Error;

const INSTALL_INFO_FILE: &str = "install-info.txt";

#[derive(Error, Debug)]
pub enum InstallError {
    #[error("File system error: {0}")]
    FileError(#[from] std::io::Error),

    #[error("Script failed to run: {0}")]
    ScriptFailure(String),
}

///install plugin when path doesn't exist (first install)or when the last saved
/// version number doesn't match the current plugin's one
pub fn needs_install(plugin: &Plugin) -> bool {
    let install_info_path = Path::new(&plugin.plugin_folder_name).join(INSTALL_INFO_FILE);

    match fs::read_to_string(install_info_path) {
        Err(_) => true,
        Ok(content) => content.trim() != plugin.manifest.description.version,
    }
}

/// Runs the plugin's install script (if it has one) and records the installed
/// version in "install-info.txt". Returns an error if the script can't be
/// started or exits unsuccessfully; in that case nothing is recorded.
pub fn install(plugin: &Plugin) -> Result<(), InstallError> {
    info!("Installing plugin: {}", plugin.identifier());

    if let Some(script) = &plugin.manifest.backend_specs.install_script_file_path {
        let full_path = Path::new(&plugin.plugin_folder_name).join(script);
        let status = Command::new(&full_path)
            .current_dir(&plugin.plugin_folder_name)
            .status()?;
        if !status.success() {
            return Err(InstallError::ScriptFailure(format!(
                "{}: install script exited with {}",
                plugin.identifier(),
                status
            )));
        }
    }

    let install_info_path = Path::new(&plugin.plugin_folder_name).join(INSTALL_INFO_FILE);
    fs::write(install_info_path, &plugin.manifest.description.version)?;
    info!("Installation successful!");
    Ok(())
}
