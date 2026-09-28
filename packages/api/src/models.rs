use serde::{Deserialize, Serialize};

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct PluginDescription {
    pub name: String,
    pub developer: String,
    pub description: String,
    pub version: String,
    pub contact: String,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct PluginBackendSpecification {
    pub entry_point_file_path: String,
    pub lifetime: BackendLifetime,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct PluginFrontendSpecification {
    pub entry_point_file_path: String
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum BackendLifetime {
    Panel,
    Session(bool),
    System(bool),
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct PluginMetaData {
    pub api_version: u8,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct PluginManifest {
    pub meta_data: PluginMetaData,
    pub backend_specs: PluginBackendSpecification,
    pub frontend_specs: PluginFrontendSpecification,
    pub description: PluginDescription,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct Plugin {
    pub manifest: PluginManifest,
    pub plugin_folder_name: String,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct PluginMessageFrame {
    pub frontend_process_id: u32,
    pub logical_channel: String,
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct User {
    pub user_id: u32,
}