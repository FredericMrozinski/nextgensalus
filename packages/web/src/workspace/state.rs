use dioxus::prelude::*;
use std::collections::HashMap;
use api::models::PluginDescription;

/// The four fixed slots that make up the workspace shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    Left,
    Center,
    Right,
    Bottom,
}

impl PanelId {
    pub const ALL: [PanelId; 4] = [PanelId::Left, PanelId::Center, PanelId::Right, PanelId::Bottom];
}

/// Unique handle for a single open tab, scoped to the panel it lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabId(u32);

impl TabId {
    pub fn raw(self) -> u32 {
        self.0
    }
}

/// What a tab renders. New plugin kinds are added here as variants; the
/// panel/tab-bar machinery never needs to change to support them.
#[derive(Debug, Clone, PartialEq)]
pub enum TabContent {
    SamplePlugin,
    Plugin(u32, PluginDescription),
}

impl TabContent {
    fn default_title(&self) -> String {
        match self {
            TabContent::SamplePlugin => String::from("Sample Plugin"),
            TabContent::Plugin(_id, description) => description.name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabInfo {
    pub id: TabId,
    pub title: String,
    pub content: TabContent,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct PanelState {
    tabs: Vec<TabInfo>,
    active: Option<TabId>,
}

const DEFAULT_LEFT_WIDTH: f64 = 260.0;
const DEFAULT_RIGHT_WIDTH: f64 = 260.0;
const DEFAULT_BOTTOM_HEIGHT: f64 = 220.0;

const MIN_SIDE_WIDTH: f64 = 160.0;
const MAX_SIDE_WIDTH: f64 = 640.0;
const MIN_BOTTOM_HEIGHT: f64 = 120.0;
const MAX_BOTTOM_HEIGHT: f64 = 640.0;

/// Shared workspace state, provided once via context at the `Workspace` root
/// and consumed by `Panel`, `TabBar` and `ResizeHandle` wherever they sit in
/// the tree. All mutation goes through its methods so behavior like "which
/// tab becomes active after a close" is defined in exactly one place.
#[derive(Clone, Copy)]
pub struct WorkspaceState {
    panels: Signal<HashMap<PanelId, PanelState>>,
    next_tab_id: Signal<u32>,
    left_width: Signal<f64>,
    right_width: Signal<f64>,
    bottom_height: Signal<f64>,
    /// Which panel's "+" button opened the plugin picker, if it's open.
    /// Kept per-panel (rather than a bare bool) so a future version can open
    /// the chosen plugin directly into the panel that requested it.
    plugin_picker_target: Signal<Option<PanelId>>,
}

impl WorkspaceState {
    pub fn new() -> Self {
        let mut panels = HashMap::new();
        for id in PanelId::ALL {
            panels.insert(id, PanelState::default());
        }

        Self {
            panels: Signal::new(panels),
            next_tab_id: Signal::new(0),
            left_width: Signal::new(DEFAULT_LEFT_WIDTH),
            right_width: Signal::new(DEFAULT_RIGHT_WIDTH),
            bottom_height: Signal::new(DEFAULT_BOTTOM_HEIGHT),
            plugin_picker_target: Signal::new(None),
        }
    }

    /// Seeds every panel with a single demo tab so the layout isn't empty on first load.
    pub fn seed_demo_tabs(&self) {
        for id in PanelId::ALL {
            self.open_tab(id, TabContent::SamplePlugin);
        }
    }

    fn next_id(&self) -> TabId {
        let mut counter = self.next_tab_id;
        let id = *counter.read();
        counter.set(id + 1);
        TabId(id)
    }

    pub fn tabs(&self, panel_id: PanelId) -> Vec<TabInfo> {
        self.panels
            .read()
            .get(&panel_id)
            .map(|panel| panel.tabs.clone())
            .unwrap_or_default()
    }

    pub fn active_tab(&self, panel_id: PanelId) -> Option<TabInfo> {
        let panels = self.panels.read();
        let panel = panels.get(&panel_id)?;
        let active_id = panel.active?;
        panel.tabs.iter().find(|tab| tab.id == active_id).cloned()
    }

    pub fn is_active(&self, panel_id: PanelId, tab_id: TabId) -> bool {
        self.panels
            .read()
            .get(&panel_id)
            .and_then(|panel| panel.active)
            .is_some_and(|active_id| active_id == tab_id)
    }

    pub fn open_tab(&self, panel_id: PanelId, content: TabContent) {
        let id = self.next_id();
        let title = content.default_title();
        let tab = TabInfo { id, title, content };

        let mut panels = self.panels;
        panels.write().entry(panel_id).or_default().tabs.push(tab);
        self.activate_tab(panel_id, id);
    }

    pub fn close_tab(&self, panel_id: PanelId, tab_id: TabId) {
        let mut panels = self.panels;
        let mut panels = panels.write();
        let Some(panel) = panels.get_mut(&panel_id) else {
            return;
        };
        let Some(pos) = panel.tabs.iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        panel.tabs.remove(pos);

        if panel.active == Some(tab_id) {
            // Prefer the tab that slid into this slot; fall back to the one before it.
            panel.active = panel
                .tabs
                .get(pos)
                .or_else(|| pos.checked_sub(1).and_then(|i| panel.tabs.get(i)))
                .map(|tab| tab.id);
        }
    }

    pub fn activate_tab(&self, panel_id: PanelId, tab_id: TabId) {
        let mut panels = self.panels;
        let mut panels = panels.write();
        if let Some(panel) = panels.get_mut(&panel_id) {
            if panel.tabs.iter().any(|tab| tab.id == tab_id) {
                panel.active = Some(tab_id);
            }
        }
    }

    pub fn resize_left(&self, delta_x: f64) {
        let mut width = self.left_width;
        let next = (*width.read() + delta_x).clamp(MIN_SIDE_WIDTH, MAX_SIDE_WIDTH);
        width.set(next);
    }

    pub fn resize_right(&self, delta_x: f64) {
        // The handle sits on the right panel's left edge, so dragging left grows it.
        let mut width = self.right_width;
        let next = (*width.read() - delta_x).clamp(MIN_SIDE_WIDTH, MAX_SIDE_WIDTH);
        width.set(next);
    }

    pub fn resize_bottom(&self, delta_y: f64) {
        // The handle sits on the bottom panel's top edge, so dragging up grows it.
        let mut height = self.bottom_height;
        let next = (*height.read() - delta_y).clamp(MIN_BOTTOM_HEIGHT, MAX_BOTTOM_HEIGHT);
        height.set(next);
    }

    pub fn plugin_picker_target(&self) -> Option<PanelId> {
        *self.plugin_picker_target.read()
    }

    pub fn open_plugin_picker(&self, panel_id: PanelId) {
        let mut target = self.plugin_picker_target;
        target.set(Some(panel_id));
    }

    pub fn close_plugin_picker(&self) {
        let mut target = self.plugin_picker_target;
        target.set(None);
    }

    pub fn left_width(&self) -> f64 {
        *self.left_width.read()
    }

    pub fn right_width(&self) -> f64 {
        *self.right_width.read()
    }

    pub fn bottom_height(&self) -> f64 {
        *self.bottom_height.read()
    }
}
