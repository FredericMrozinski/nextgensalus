//! This module has 2 functions:
//! -one to check whether an installation has ever previously taken place (if an install-info.txt file even exists)
//! or if the plugin has since been updates by the developer, in which case the version field of the manifest will have changed
//!
//! -the second for the actual installation process, if the first function returns true

use crate::models::{InstallMethod::{self, Script}, Plugin};
use dioxus::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command;
use thiserror::Error;
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
#[cfg(target_os = "linux")]
use landlock::{path_beneath_rules, Access, AccessFs, Ruleset, RulesetAttr, RulesetCreated, RulesetCreatedAttr, RulesetError, ABI};

const INSTALL_INFO_FILE: &str = "install-info.txt";

#[derive(Error, Debug)]
pub enum InstallError {
    #[error("File system error: {0}")]
    FileError(#[from] std::io::Error),

    #[error("Script failed to run: {0}")]
    ScriptFailure(String),

    #[cfg(target_os = "linux")]
    #[error("Sandbox setup failed: {0}")]
    Sandbox(#[from] landlock::RulesetError),
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

#[cfg(target_os = "linux")]
fn build_ruleset(plugin_folder: &Path) -> Result<RulesetCreated, RulesetError> {
    let abi = ABI::V1;
    let resolv_conf = std::fs::canonicalize("/etc/resolv.conf").unwrap_or_else(|_| "/etc/resolv.conf".into());
    Ruleset::default()
        .handle_access(AccessFs::from_all(abi))?
        .create()?
        .add_rules(path_beneath_rules(&[plugin_folder], AccessFs::from_all(abi)))?
        .add_rules(path_beneath_rules(&["/dev/null"], AccessFs::from_file(abi)))?
        .add_rules(path_beneath_rules(&[resolv_conf], AccessFs::from_read(abi)))?
        .add_rules(path_beneath_rules(&["/usr", "/lib", "/lib64", "/bin", "/etc"], AccessFs::from_read(abi)))
        
}

/// Runs the plugin's install script (if it has one) and records the installed
/// version in "install-info.txt". Returns an error if the script can't be
/// started or exits unsuccessfully; in that case nothing is recorded.
pub fn install(plugin: &Plugin) -> Result<(), InstallError> {
    info!("Installing plugin: {}", plugin.identifier());

    if let Some(Script(script)) = &plugin.manifest.backend_specs.install_method {
        let plugin_folder = Path::new(&plugin.plugin_folder_name);
        let full_path = plugin_folder.join(script);

        //Temporary files of the install script stay inside the plugin folder
        let tmp_dir = plugin_folder.join(".tmp");
        fs::create_dir_all(&tmp_dir)?;

        let mut cmd = Command::new(&full_path);
        cmd.current_dir(plugin_folder)
        .env("TMPDIR", &tmp_dir)
        .env("PIP_NO_CACHE_DIR", "1");

        #[cfg(target_os = "linux")]
        {
            let mut ruleset = Some(build_ruleset(plugin_folder)?);
            unsafe {
                cmd.pre_exec(move || {
                    let ruleset = ruleset
                        .take()
                        .ok_or_else(|| std::io::Error::other("sandbox already applied"))?;
                    ruleset.restrict_self().map_err(std::io::Error::other)?;
                    Ok(())
                });
            }
        }

        let status = cmd.status()?;
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
