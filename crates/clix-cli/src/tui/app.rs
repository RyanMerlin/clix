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

#[derive(Debug, Clone, PartialEq)]
pub enum ModalKind {
    CreateProfile,
    CreateCapability,
    CreatePack,
    InstallPack,
}

#[derive(Debug, Clone, Default)]
pub struct ModalState {
    pub kind: Option<ModalKind>,
    pub fields: Vec<String>,
    pub field_idx: usize,
    pub input_buf: String,
}

impl ModalState {
    pub fn is_open(&self) -> bool { self.kind.is_some() }
    pub fn open(&mut self, kind: ModalKind, num_fields: usize) {
        self.kind = Some(kind);
        self.fields = vec![String::new(); num_fields];
        self.field_idx = 0;
        self.input_buf = String::new();
    }
    pub fn close(&mut self) {
        self.kind = None;
        self.fields.clear();
        self.field_idx = 0;
        self.input_buf = String::new();
    }
    pub fn commit_field(&mut self) {
        if self.field_idx < self.fields.len() {
            self.fields[self.field_idx] = self.input_buf.clone();
        }
    }
    pub fn next_field(&mut self) -> bool {
        self.commit_field();
        if self.field_idx + 1 < self.fields.len() {
            self.field_idx += 1;
            self.input_buf = self.fields[self.field_idx].clone();
            true
        } else {
            false
        }
    }
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
    pub modal: ModalState,
}

impl App {
    fn validate_name(name: &str) -> Option<&'static str> {
        if name.is_empty() { return Some("Name is required"); }
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Some("Name must not contain path separators");
        }
        None
    }

    pub fn new() -> Result<Self> {
        let mut state = ClixState::load(home_dir())?;
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
            modal: ModalState::default(),
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
        // If a modal is open, it gets all keys
        if self.modal.is_open() {
            self.handle_modal_key(key);
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
            KeyCode::Char('n') => self.open_create_modal(),
            KeyCode::Char('i') if self.screen == Screen::Packs => {
                self.modal.open(ModalKind::InstallPack, 1);
                self.last_error = None;
            }
            KeyCode::Tab => self.next_screen(),
            KeyCode::Left => self.prev_screen(),
            KeyCode::Right => self.next_screen(),
            KeyCode::Down => self.cursor_down(),
            KeyCode::Up => self.cursor_up(),
            KeyCode::Enter => self.handle_enter(),
            KeyCode::Esc | KeyCode::Backspace => self.handle_back(),
            _ => {}
        }
    }

    fn handle_modal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.modal.close(),
            KeyCode::Tab => {
                if !self.modal.next_field() {
                    // Last field — wrap back to first
                    self.modal.commit_field();
                    self.modal.field_idx = 0;
                    self.modal.input_buf = self.modal.fields.get(0).cloned().unwrap_or_default();
                }
            }
            KeyCode::Enter => self.modal_confirm(),
            KeyCode::Backspace => {
                self.modal.input_buf.pop();
            }
            KeyCode::Char(c) => {
                self.modal.input_buf.push(c);
            }
            _ => {}
        }
    }

    fn modal_confirm(&mut self) {
        self.modal.commit_field();
        match self.modal.kind.clone() {
            Some(ModalKind::CreateProfile) => {
                let name = self.modal.fields.get(0).cloned().unwrap_or_default();
                let desc = self.modal.fields.get(1).cloned().unwrap_or_default();
                if let Some(err) = Self::validate_name(&name) {
                    self.last_error = Some(err.to_string());
                    return;
                }
                if let Err(e) = self.create_profile(&name, &desc) {
                    self.last_error = Some(format!("Create failed: {e}"));
                } else {
                    self.modal.close();
                    self.last_error = None;
                    if let Err(e) = self.reload() {
                        self.last_error = Some(format!("Reload failed: {e}"));
                    }
                }
            }
            Some(ModalKind::CreateCapability) => {
                let name = self.modal.fields.get(0).cloned().unwrap_or_default();
                let desc = self.modal.fields.get(1).cloned().unwrap_or_default();
                let command = self.modal.fields.get(2).cloned().unwrap_or_default();
                if let Some(err) = Self::validate_name(&name) {
                    self.last_error = Some(err.to_string());
                    return;
                }
                if command.is_empty() {
                    self.last_error = Some("Command is required".to_string());
                    return;
                }
                if let Err(e) = self.create_capability(&name, &desc, &command) {
                    self.last_error = Some(format!("Create failed: {e}"));
                } else {
                    self.modal.close();
                    self.last_error = None;
                    if let Err(e) = self.reload() {
                        self.last_error = Some(format!("Reload failed: {e}"));
                    }
                }
            }
            Some(ModalKind::CreatePack) => {
                let name = self.modal.fields.get(0).cloned().unwrap_or_default();
                let desc = self.modal.fields.get(1).cloned().unwrap_or_default();
                if let Some(err) = Self::validate_name(&name) {
                    self.last_error = Some(err.to_string());
                    return;
                }
                if let Err(e) = self.create_pack(&name, &desc) {
                    self.last_error = Some(format!("Create failed: {e}"));
                } else {
                    self.modal.close();
                    self.last_error = None;
                    if let Err(e) = self.reload() {
                        self.last_error = Some(format!("Reload failed: {e}"));
                    }
                }
            }
            Some(ModalKind::InstallPack) => {
                let path = self.modal.fields.get(0).cloned().unwrap_or_default();
                if path.is_empty() {
                    self.last_error = Some("Path is required".to_string());
                    return;
                }
                if let Err(e) = self.install_pack(&path) {
                    self.last_error = Some(format!("Install failed: {e}"));
                } else {
                    self.modal.close();
                    self.last_error = None;
                    if let Err(e) = self.reload() {
                        self.last_error = Some(format!("Reload failed: {e}"));
                    }
                }
            }
            None => {}
        }
    }

    fn open_create_modal(&mut self) {
        match self.screen {
            Screen::Profiles => self.modal.open(ModalKind::CreateProfile, 2),
            Screen::Capabilities => self.modal.open(ModalKind::CreateCapability, 3),
            Screen::Packs => self.modal.open(ModalKind::CreatePack, 2),
        }
        self.last_error = None;
    }

    fn create_profile(&self, name: &str, description: &str) -> anyhow::Result<()> {
        use clix_core::manifest::profile::ProfileManifest;
        let state = clix_core::state::ClixState::load(clix_core::state::home_dir())?;
        let manifest = ProfileManifest {
            name: name.to_string(),
            version: 1,
            description: if description.is_empty() { None } else { Some(description.to_string()) },
            capabilities: vec![],
            workflows: vec![],
            settings: serde_json::Value::Null,
        };
        let yaml = serde_yaml::to_string(&manifest)?;
        let path = state.profiles_dir.join(format!("{}.yaml", name));
        std::fs::write(path, yaml)?;
        Ok(())
    }

    fn create_capability(&self, name: &str, description: &str, command: &str) -> anyhow::Result<()> {
        use clix_core::manifest::capability::{CapabilityManifest, Backend, RiskLevel, SideEffectClass};
        let state = clix_core::state::ClixState::load(clix_core::state::home_dir())?;
        let manifest = CapabilityManifest {
            name: name.to_string(),
            version: 1,
            description: if description.is_empty() { None } else { Some(description.to_string()) },
            backend: Backend::Subprocess {
                command: command.to_string(),
                args: vec![],
                cwd_from_input: None,
            },
            risk: RiskLevel::Low,
            side_effect_class: SideEffectClass::None,
            sandbox_profile: None,
            approval_policy: None,
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            validators: vec![],
            credentials: vec![],
        };
        let yaml = serde_yaml::to_string(&manifest)?;
        let path = state.capabilities_dir.join(format!("{}.yaml", name));
        std::fs::write(path, yaml)?;
        Ok(())
    }

    fn create_pack(&self, name: &str, description: &str) -> anyhow::Result<()> {
        use clix_core::manifest::pack::PackManifest;
        let state = clix_core::state::ClixState::load(clix_core::state::home_dir())?;
        let pack_dir = state.packs_dir.join(name);
        std::fs::create_dir_all(&pack_dir)?;
        std::fs::create_dir_all(pack_dir.join("capabilities"))?;
        std::fs::create_dir_all(pack_dir.join("profiles"))?;
        let manifest = PackManifest {
            name: name.to_string(),
            version: 1,
            description: if description.is_empty() { None } else { Some(description.to_string()) },
            author: None,
            homepage: None,
            capabilities: vec![],
            profiles: vec![],
            workflows: vec![],
        };
        let yaml = serde_yaml::to_string(&manifest)?;
        std::fs::write(pack_dir.join("pack.yaml"), yaml)?;
        Ok(())
    }

    fn install_pack(&self, path_str: &str) -> anyhow::Result<()> {
        use clix_core::packs::install::install_pack;
        let state = clix_core::state::ClixState::load(clix_core::state::home_dir())?;
        let path = std::path::PathBuf::from(path_str);
        install_pack(&path, &state.packs_dir)?;
        Ok(())
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

    fn prev_screen(&mut self) {
        self.cursor = 0;
        self.screen = match self.screen {
            Screen::Profiles => Screen::Packs,
            Screen::Capabilities => Screen::Profiles,
            Screen::Packs => Screen::Capabilities,
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
