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

/// Panel of the workspace a frontend component opens in.
#[derive(Copy, PartialEq, Eq, Clone, Serialize, Deserialize, Debug)]
pub enum TargetPanel {
    Left,
    Center,
    Right,
    Bottom,
}

impl TargetPanel {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }
}

/// One frontend of a plugin (`[[frontend-component]]` in the manifest). All components of a
/// plugin share the plugin's backend.
#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct FrontendComponent {
    pub name: String,
    /// Tab label; the plugin's name when absent.
    pub title: Option<String>,
    pub entry_point_file_path: String,
    pub target_panel: TargetPanel,
    /// Lowercase extensions (without dot) of files this component can display.
    pub file_viewer_for: Vec<String>,
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
    /// Name of the component the plugin picker opens. Plugins without one are not listed there.
    pub entry_component: Option<String>,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct PluginManifest {
    pub meta_data: PluginMetaData,
    pub backend_specs: PluginBackendSpecification,
    pub frontend_components: Vec<FrontendComponent>,
    pub description: PluginDescription,
    /// Identifiers (plugin folder names) of the plugins this plugin may open via
    /// `salus.openDependency(...)`.
    pub dependencies: Vec<String>,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct Plugin {
    pub manifest: PluginManifest,
    pub plugin_folder_name: String,
}

// #[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
// pub struct PluginMessageFrame {
//     pub frontend_process_id: u32,
//     pub logical_channel: String,
//     #[serde(with = "serde_bytes")]
//     pub payload: Vec<u8>
// }

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct User {
    pub user_id: u32,
}

impl Plugin {
    pub fn component(&self, name: &str) -> Option<&FrontendComponent> {
        self.manifest.frontend_components.iter().find(|component| component.name == name)
    }

    /// The component the plugin picker opens, if the plugin has one.
    pub fn entry_component(&self) -> Option<&FrontendComponent> {
        self.manifest.meta_data.entry_component.as_deref().and_then(|name| self.component(name))
    }

    /// Stable identifier of the plugin: the name of its folder in the plugins directory.
    pub fn identifier(&self) -> String {
        std::path::Path::new(&self.plugin_folder_name)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// A frontend process that exists (spawned now, or already running when `reused`) for a component.
#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct SpawnedComponent {
    pub plugin_id: u32,
    pub plugin_name: String,
    pub component_name: String,
    /// Tab label: the component's `title`, or the plugin's name.
    pub title: String,
    pub target_panel: TargetPanel,
    pub frontend_process_id: u32,
    pub parent_frontend_process_id: Option<u32>,
    /// JSON text handed to the component as `?params=...`.
    pub params: Option<String>,
    /// The process already existed (`reuse`); its tab only has to be focused.
    pub reused: bool,
}

/// A component that can display files of some type.
#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct FileViewer {
    pub plugin_id: u32,
    pub plugin_name: String,
    pub component_name: String,
    pub title: String,
}
