use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use clix_core::loader::build_registry;
use clix_core::manifest::loader::load_dir;
use clix_core::manifest::pack::PackManifest;
use clix_core::manifest::profile::ProfileManifest;
use clix_core::registry::CapabilityRegistry;
use clix_core::state::{home_dir, ClixState};

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Profiles,
    Capabilities,
    Packs,
}

#[derive(Debug, Clone)]
pub enum CapView {
    Namespaces,
    Listing(String),
    Detail(String),
}

pub struct App {
    pub screen: Screen,
    pub cap_view: CapView,
    pub profiles: Vec<ProfileManifest>,
    pub active_profiles: Vec<String>,
    pub registry: CapabilityRegistry,
    pub packs: Vec<PackManifest>,
    pub cursor: usize,
    pub should_quit: bool,
    pub last_error: Option<String>,
}

impl App {
    pub fn new() -> Result<Self> {
        let mut state = ClixState::load(home_dir())?;
        // First-run guard: activate base profile if missing and base pack is installed.
        // Handles the case where the user runs 'clix tui' before 'clix init'.
        if state.config.active_profiles.is_empty() {
            let base_pack_dir = state.packs_dir.join("base");
            if base_pack_dir.exists() {
                state.config.active_profiles.push("base".to_string());
                let yaml = serde_yaml::to_string(&state.config)?;
                std::fs::write(&state.config_path, yaml)?;
            }
        }
        let registry = build_registry(&state)?;
        let profiles: Vec<ProfileManifest> = load_dir(&state.profiles_dir).unwrap_or_default();
        let packs: Vec<PackManifest> = load_dir(&state.packs_dir).unwrap_or_default();
        let active_profiles = state.config.active_profiles.clone();
        Ok(Self {
            screen: Screen::Profiles,
            cap_view: CapView::Namespaces,
            profiles,
            active_profiles,
            registry,
            packs,
            cursor: 0,
            should_quit: false,
            last_error: None,
        })
    }

    pub fn reload(&mut self) -> Result<()> {
        let new = Self::new()?;
        self.profiles = new.profiles;
        self.active_profiles = new.active_profiles;
        self.registry = new.registry;
        self.packs = new.packs;
        self.cursor = 0;
        self.cap_view = CapView::Namespaces;
        Ok(())
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Ctrl-C always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('r') => {
                if let Err(e) = self.reload() {
                    self.last_error = Some(format!("Reload failed: {e}"));
                } else {
                    self.last_error = None;
                }
            }
            KeyCode::Char('1') => { self.screen = Screen::Profiles; self.cursor = 0; }
            KeyCode::Char('2') => { self.screen = Screen::Capabilities; self.cursor = 0; self.cap_view = CapView::Namespaces; }
            KeyCode::Char('3') => { self.screen = Screen::Packs; self.cursor = 0; }
            KeyCode::Tab => self.next_screen(),
            KeyCode::Down | KeyCode::Char('j') => self.cursor_down(),
            KeyCode::Up | KeyCode::Char('k') => self.cursor_up(),
            KeyCode::Enter => self.handle_enter(),
            KeyCode::Esc | KeyCode::Backspace => self.handle_back(),
            _ => {}
        }
    }

    fn next_screen(&mut self) {
        self.cursor = 0;
        self.screen = match self.screen {
            Screen::Profiles => Screen::Capabilities,
            Screen::Capabilities => Screen::Packs,
            Screen::Packs => Screen::Profiles,
        };
        self.cap_view = CapView::Namespaces;
    }

    fn cursor_down(&mut self) {
        let len = self.current_list_len();
        if len > 0 { self.cursor = (self.cursor + 1).min(len - 1); }
    }

    fn cursor_up(&mut self) {
        if self.cursor > 0 { self.cursor -= 1; }
    }

    fn current_list_len(&self) -> usize {
        match self.screen {
            Screen::Profiles => self.profiles.len(),
            Screen::Capabilities => match &self.cap_view {
                CapView::Namespaces => self.registry.namespaces().len(),
                CapView::Listing(ns) => self.registry.by_namespace(ns).len(),
                CapView::Detail(_) => 0,
            },
            Screen::Packs => self.packs.len(),
        }
    }

    fn handle_enter(&mut self) {
        match self.screen {
            Screen::Capabilities => {
                match self.cap_view.clone() {
                    CapView::Namespaces => {
                        let namespaces = self.registry.namespaces();
                        if let Some(ns) = namespaces.get(self.cursor) {
                            self.cap_view = CapView::Listing(ns.key.clone());
                            self.cursor = 0;
                        }
                    }
                    CapView::Listing(ns) => {
                        let caps = self.registry.by_namespace(&ns);
                        if let Some(cap) = caps.get(self.cursor) {
                            self.cap_view = CapView::Detail(cap.name.clone());
                            self.cursor = 0;
                        }
                    }
                    CapView::Detail(_) => {}
                }
            }
            Screen::Profiles => {
                if let Some(profile) = self.profiles.get(self.cursor) {
                    let name = profile.name.clone();
                    if let Err(e) = self.toggle_profile(&name) {
                        self.last_error = Some(format!("Toggle failed: {e}"));
                    } else {
                        self.last_error = None;
                    }
                }
            }
            Screen::Packs => {}
        }
    }

    pub fn toggle_profile(&mut self, name: &str) -> Result<()> {
        let mut state = ClixState::load(home_dir())?;
        if state.config.active_profiles.contains(&name.to_string()) {
            state.config.active_profiles.retain(|p| p != name);
        } else {
            state.config.active_profiles.push(name.to_string());
        }
        // Note: re-serializes the full config, which will strip comments and any
        // unknown keys not present in ClixConfig. Same trade-off as the CLI's profile activate/deactivate.
        let yaml = serde_yaml::to_string(&state.config)?;
        std::fs::write(&state.config_path, yaml)?;
        self.active_profiles = state.config.active_profiles.clone();
        Ok(())
    }

    fn handle_back(&mut self) {
        match self.screen {
            Screen::Capabilities => {
                match self.cap_view.clone() {
                    CapView::Detail(ref name) => {
                        let ns = CapabilityRegistry::group_key(name);
                        self.cap_view = CapView::Listing(ns);
                        self.cursor = 0;
                    }
                    CapView::Listing(_) => { self.cap_view = CapView::Namespaces; self.cursor = 0; }
                    CapView::Namespaces => self.should_quit = true,
                }
            }
            _ => self.should_quit = true,
        }
    }
}
