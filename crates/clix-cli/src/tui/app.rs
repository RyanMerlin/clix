use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use clix_core::loader::build_registry;
use clix_core::manifest::pack::PackManifest;
use clix_core::manifest::profile::ProfileManifest;
use clix_core::registry::CapabilityRegistry;
use clix_core::state::{home_dir, ClixState};
use clix_core::manifest::loader::load_dir;
use clix_core::manifest::capability::{
    CapabilityManifest, Backend, SideEffectClass,
};
use clix_core::packs::scaffold::{scaffold_pack, Preset};

use crate::tui::screens::wizards::pack::{PackWizard, PackWizardAction};
use crate::tui::screens::wizards::profile::{ProfileWizard, ProfileWizardAction};
use crate::tui::screens::wizards::capability::{CapabilityWizard, CapWizardAction};

// ─── screen ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Dashboard,
    Profiles,
    Capabilities,
    Packs,
    Receipts,
    Workflows,
    Broker,
}

impl Screen {
    pub fn sidebar_index(&self) -> usize {
        match self {
            Screen::Dashboard => 0,
            Screen::Profiles => 1,
            Screen::Capabilities => 2,
            Screen::Packs => 3,
            Screen::Receipts => 4,
            Screen::Workflows => 5,
            Screen::Broker => 6,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Screen::Dashboard,
            1 => Screen::Profiles,
            2 => Screen::Capabilities,
            3 => Screen::Packs,
            4 => Screen::Receipts,
            5 => Screen::Workflows,
            6 => Screen::Broker,
            _ => Screen::Dashboard,
        }
    }

    pub fn next(&self) -> Self { Screen::from_index((self.sidebar_index() + 1) % 7) }
    pub fn prev(&self) -> Self {
        Screen::from_index(self.sidebar_index().checked_sub(1).unwrap_or(6))
    }
}

// ─── capabilities drill-down ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum CapView {
    Namespaces,
    Listing(String),
    Detail(String),
}

// ─── receipt preview row ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ReceiptRow {
    pub time: String,
    pub capability: String,
    pub profile: String,
    pub outcome: String,
    pub latency: String,
}

// ─── overlay ─────────────────────────────────────────────────────────────────

pub enum Overlay {
    None,
    ProfileCreate(ProfileWizard),
    CapabilityCreate(CapabilityWizard),
    PackCreate(PackWizard),
    PackEdit { pack_name: String, checklist: crate::tui::widgets::checklist::Checklist },
    InstallPack(String),  // path buffer
    Toast { message: String, is_error: bool, expires_at: std::time::Instant },
    Help,
}

impl Overlay {
    pub fn is_open(&self) -> bool { !matches!(self, Overlay::None) }
}

// ─── app ─────────────────────────────────────────────────────────────────────

pub struct App {
    pub screen: Screen,
    pub overlay: Overlay,
    // data
    pub profiles: Vec<ProfileManifest>,
    pub active_profiles: Vec<String>,
    pub registry: CapabilityRegistry,
    pub packs: Vec<PackManifest>,
    pub receipts_preview: Vec<ReceiptRow>,
    // per-screen cursors
    pub profiles_cursor: usize,
    pub caps_view: CapView,
    pub caps_cursor: usize,
    pub packs_cursor: usize,
    pub should_quit: bool,
}

impl App {
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
        let packs = load_packs_from_dir(&state.packs_dir);
        let profiles = load_all_profiles(&state);
        let active_profiles = state.config.active_profiles.clone();
        Ok(Self {
            screen: Screen::Dashboard,
            overlay: Overlay::None,
            profiles,
            active_profiles,
            registry,
            packs,
            receipts_preview: vec![],
            profiles_cursor: 0,
            caps_view: CapView::Namespaces,
            caps_cursor: 0,
            packs_cursor: 0,
            should_quit: false,
        })
    }

    pub fn reload(&mut self) -> Result<()> {
        let new = Self::new()?;
        self.profiles = new.profiles;
        self.active_profiles = new.active_profiles;
        self.registry = new.registry;
        self.packs = new.packs;
        self.profiles_cursor = 0;
        self.caps_view = CapView::Namespaces;
        self.caps_cursor = 0;
        self.packs_cursor = 0;
        Ok(())
    }

    pub fn cursor(&self) -> usize {
        match self.screen {
            Screen::Profiles => self.profiles_cursor,
            Screen::Capabilities => self.caps_cursor,
            Screen::Packs => self.packs_cursor,
            _ => 0,
        }
    }

    fn cursor_mut(&mut self) -> &mut usize {
        match self.screen {
            Screen::Profiles => &mut self.profiles_cursor,
            Screen::Capabilities => &mut self.caps_cursor,
            Screen::Packs => &mut self.packs_cursor,
            _ => &mut self.profiles_cursor,  // unused placeholder
        }
    }

    fn toast(&mut self, msg: &str, is_error: bool) {
        self.overlay = Overlay::Toast {
            message: msg.to_string(),
            is_error,
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(3),
        };
    }

    pub fn tick(&mut self) {
        // Dismiss expired toasts
        if let Overlay::Toast { expires_at, .. } = &self.overlay {
            if std::time::Instant::now() >= *expires_at {
                self.overlay = Overlay::None;
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        // Delegate to overlay first
        match &self.overlay {
            Overlay::None | Overlay::Toast { .. } => {}
            _ => {
                self.handle_overlay_key(key);
                return;
            }
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('r') => {
                match self.reload() {
                    Ok(_) => self.toast("Reloaded", false),
                    Err(e) => self.toast(&format!("Reload failed: {e}"), true),
                }
            }
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            // Number shortcuts
            KeyCode::Char('1') => self.switch_to(Screen::Profiles),
            KeyCode::Char('2') => self.switch_to(Screen::Capabilities),
            KeyCode::Char('3') => self.switch_to(Screen::Packs),
            KeyCode::Char('4') => self.switch_to(Screen::Receipts),
            KeyCode::Char('5') => self.switch_to(Screen::Workflows),
            KeyCode::Char('6') => self.switch_to(Screen::Broker),
            KeyCode::Char('0') => self.switch_to(Screen::Dashboard),
            // Tab / arrows for screen navigation
            KeyCode::Tab => self.switch_to(self.screen.next()),
            KeyCode::BackTab => self.switch_to(self.screen.prev()),
            // n = new (create wizard)
            KeyCode::Char('n') => self.open_create_wizard(),
            // i = install pack
            KeyCode::Char('i') if self.screen == Screen::Packs => {
                self.overlay = Overlay::InstallPack(String::new());
            }
            // e = edit pack capabilities
            KeyCode::Char('e') if self.screen == Screen::Packs => {
                self.open_pack_edit();
            }
            // Content navigation — arrow keys
            KeyCode::Down => self.cursor_down(),
            KeyCode::Up => self.cursor_up(),
            KeyCode::Enter => self.handle_enter(),
            KeyCode::Esc | KeyCode::Backspace => self.handle_back(),
            _ => {}
        }
    }

    fn switch_to(&mut self, screen: Screen) {
        self.screen = screen;
    }

    fn open_pack_edit(&mut self) {
        use crate::tui::widgets::checklist::{Checklist, ChecklistItem};
        let Some(pack) = self.packs.get(self.packs_cursor) else { return; };
        let pack_name = pack.name.clone();
        let current_caps: std::collections::HashSet<String> = pack.capabilities.iter().cloned().collect();

        // Build checklist from all known capabilities
        let mut items: Vec<ChecklistItem> = self.registry.all().iter().map(|cap| {
            let risk_str = format!("{:?}", cap.risk).to_lowercase();
            let tag_color = crate::tui::theme::risk_color(&risk_str);
            let mut item = ChecklistItem::new(
                &cap.name,
                &cap.name,
                cap.description.as_deref().unwrap_or(""),
                &risk_str,
                tag_color,
                "",
            );
            item.selected = current_caps.contains(&cap.name);
            item
        }).collect();
        items.sort_by(|a, b| a.id.cmp(&b.id));

        self.overlay = Overlay::PackEdit { pack_name, checklist: Checklist::new(items) };
    }

    fn open_create_wizard(&mut self) {
        match self.screen {
            Screen::Profiles => {
                self.overlay = Overlay::ProfileCreate(ProfileWizard::new(&self.registry));
            }
            Screen::Capabilities => {
                self.overlay = Overlay::CapabilityCreate(CapabilityWizard::new());
            }
            Screen::Packs => {
                self.overlay = Overlay::PackCreate(PackWizard::new());
            }
            _ => {}
        }
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) {
        // Dismiss help overlay on any key
        if matches!(self.overlay, Overlay::Help) {
            self.overlay = Overlay::None;
            return;
        }

        // Install pack text buffer
        if let Overlay::InstallPack(ref mut buf) = self.overlay {
            match key.code {
                KeyCode::Esc => { self.overlay = Overlay::None; }
                KeyCode::Enter => {
                    let path = buf.clone();
                    if path.is_empty() {
                        self.overlay = Overlay::None;
                        return;
                    }
                    match self.do_install_pack(&path) {
                        Ok(_) => {
                            self.overlay = Overlay::None;
                            let _ = self.reload();
                            self.toast("Pack installed", false);
                        }
                        Err(e) => self.toast(&format!("Install failed: {e}"), true),
                    }
                }
                KeyCode::Backspace => { buf.pop(); }
                KeyCode::Char(c) => { buf.push(c); }
                _ => {}
            }
            return;
        }

        // Pack edit overlay
        if let Overlay::PackEdit { ref pack_name, ref mut checklist } = self.overlay {
            match key.code {
                KeyCode::Esc => { self.overlay = Overlay::None; return; }
                KeyCode::Enter => {
                    let pack_name = pack_name.clone();
                    let selected = checklist.selected_ids();
                    let res = self.do_edit_pack_capabilities(&pack_name, &selected);
                    match res {
                        Ok(msg) => {
                            self.overlay = Overlay::None;
                            let _ = self.reload();
                            self.toast(&msg, false);
                        }
                        Err(e) => self.toast(&format!("Error: {e}"), true),
                    }
                    return;
                }
                code => { checklist.handle_key(code); return; }
            }
        }

        // Wizard key delegation
        let code = key.code;
        let result: Option<Result<String>> = match &mut self.overlay {
            Overlay::ProfileCreate(wiz) => {
                let action = wiz.handle_key(code);
                match action {
                    ProfileWizardAction::Cancel => {
                        self.overlay = Overlay::None;
                        None
                    }
                    ProfileWizardAction::Submit { name, description, capabilities } => {
                        Some(self.do_create_profile(&name, &description, &capabilities))
                    }
                    ProfileWizardAction::None => None,
                }
            }
            Overlay::CapabilityCreate(wiz) => {
                let action = wiz.handle_key(code);
                match action {
                    CapWizardAction::Cancel => {
                        self.overlay = Overlay::None;
                        None
                    }
                    CapWizardAction::Submit { name, description, command, args, risk, side_effect } => {
                        Some(self.do_create_capability(&name, &description, &command, &args, &risk, &side_effect))
                    }
                    CapWizardAction::None => None,
                }
            }
            Overlay::PackCreate(wiz) => {
                let action = wiz.handle_key(code);
                match action {
                    PackWizardAction::Cancel => {
                        self.overlay = Overlay::None;
                        None
                    }
                    PackWizardAction::Submit { name, description, author, preset, seed_command, capability_names } => {
                        Some(self.do_create_pack(&name, &description, &author, preset, &seed_command, &capability_names))
                    }
                    PackWizardAction::None => None,
                }
            }
            _ => None,
        };

        if let Some(res) = result {
            match res {
                Ok(msg) => {
                    self.overlay = Overlay::None;
                    let _ = self.reload();
                    self.toast(&msg, false);
                }
                Err(e) => {
                    // Don't close wizard — show error in it if possible
                    self.toast(&format!("Error: {e}"), true);
                }
            }
        }
    }

    // ─── cursor navigation ────────────────────────────────────────────────────

    fn cursor_down(&mut self) {
        let len = self.current_list_len();
        if len > 0 {
            let c = self.cursor_mut();
            *c = (*c + 1).min(len - 1);
        }
    }

    fn cursor_up(&mut self) {
        let c = self.cursor_mut();
        if *c > 0 { *c -= 1; }
    }

    fn current_list_len(&self) -> usize {
        match self.screen {
            Screen::Profiles => self.profiles.len(),
            Screen::Capabilities => match &self.caps_view {
                CapView::Namespaces => self.registry.namespaces().len(),
                CapView::Listing(ns) => self.registry.by_namespace(ns).len(),
                CapView::Detail(_) => 0,
            },
            Screen::Packs => self.packs.len(),
            _ => 0,
        }
    }

    fn handle_enter(&mut self) {
        match self.screen {
            Screen::Capabilities => {
                match self.caps_view.clone() {
                    CapView::Namespaces => {
                        let namespaces = self.registry.namespaces();
                        if let Some(ns) = namespaces.get(self.caps_cursor) {
                            self.caps_view = CapView::Listing(ns.key.clone());
                            self.caps_cursor = 0;
                        }
                    }
                    CapView::Listing(ns) => {
                        let caps = self.registry.by_namespace(&ns);
                        if let Some(cap) = caps.get(self.caps_cursor) {
                            self.caps_view = CapView::Detail(cap.name.clone());
                            self.caps_cursor = 0;
                        }
                    }
                    CapView::Detail(_) => {}
                }
            }
            Screen::Profiles => {
                if let Some(profile) = self.profiles.get(self.profiles_cursor) {
                    let name = profile.name.clone();
                    match self.toggle_profile(&name) {
                        Ok(_) => { let _ = self.reload(); }
                        Err(e) => self.toast(&format!("Toggle failed: {e}"), true),
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_back(&mut self) {
        match self.screen {
            Screen::Capabilities => {
                match self.caps_view.clone() {
                    CapView::Detail(name) => {
                        let ns = CapabilityRegistry::group_key(&name);
                        self.caps_view = CapView::Listing(ns);
                        self.caps_cursor = 0;
                    }
                    CapView::Listing(_) => {
                        self.caps_view = CapView::Namespaces;
                        self.caps_cursor = 0;
                    }
                    CapView::Namespaces => {}
                }
            }
            _ => {}
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

    // ─── write operations ─────────────────────────────────────────────────────

    fn do_create_profile(&self, name: &str, description: &str, capabilities: &[String]) -> Result<String> {
        let state = ClixState::load(home_dir())?;
        let manifest = ProfileManifest {
            name: name.to_string(),
            version: 1,
            description: if description.is_empty() { None } else { Some(description.to_string()) },
            capabilities: capabilities.to_vec(),
            workflows: vec![],
            settings: serde_json::Value::Null,
            isolation_defaults: Default::default(),
        };
        let yaml = serde_yaml::to_string(&manifest)?;
        let path = state.profiles_dir.join(format!("{}.yaml", name));
        std::fs::write(path, yaml)?;
        Ok(format!("Profile '{}' created with {} capabilities", name, capabilities.len()))
    }

    fn do_create_capability(
        &self, name: &str, description: &str,
        command: &str, args: &[String],
        risk_str: &str, side_effect_str: &str,
    ) -> Result<String> {
        use clix_core::manifest::capability::RiskLevel;
        let state = ClixState::load(home_dir())?;
        let risk = match risk_str {
            "medium" => RiskLevel::Medium,
            "high" => RiskLevel::High,
            "critical" => RiskLevel::Critical,
            _ => RiskLevel::Low,
        };
        let side_effect = match side_effect_str {
            "readOnly" => SideEffectClass::ReadOnly,
            "additive" => SideEffectClass::Additive,
            "mutating" => SideEffectClass::Mutating,
            "destructive" => SideEffectClass::Destructive,
            _ => SideEffectClass::None,
        };
        let manifest = CapabilityManifest {
            name: name.to_string(),
            version: 1,
            description: if description.is_empty() { None } else { Some(description.to_string()) },
            backend: Backend::Subprocess {
                command: command.to_string(),
                args: args.to_vec(),
                cwd_from_input: None,
            },
            risk,
            side_effect_class: side_effect,
            sandbox_profile: None,
            isolation: Default::default(),
            approval_policy: None,
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            validators: vec![],
            credentials: vec![],
            argv_pattern: None,
        };
        let yaml = serde_yaml::to_string(&manifest)?;
        let path = state.capabilities_dir.join(format!("{}.yaml", name));
        std::fs::write(path, yaml)?;
        Ok(format!("Capability '{}' created", name))
    }

    fn do_create_pack(
        &self, name: &str, description: &str, author: &str,
        preset: Preset, seed_command: &str, capability_names: &[String],
    ) -> Result<String> {
        let state = ClixState::load(home_dir())?;
        let pack_dir = scaffold_pack(name, preset, if seed_command.is_empty() { None } else { Some(seed_command) }, &state.packs_dir)?;

        // Write extra capability YAML files for each selected subcommand
        let caps_dir = pack_dir.join("capabilities");
        for cap_name in capability_names {
            let file_name = format!("{}.yaml", cap_name.replace('.', "_"));
            let yaml = format!(
                "name: {cap_name}\nversion: 1\ndescription: ''\nbackend:\n  type: subprocess\n  command: {}\nrisk: low\nsideEffectClass: readOnly\ninputSchema:\n  type: object\n  properties: {{}}\n",
                cap_name.split('.').next().unwrap_or(name)
            );
            let _ = std::fs::write(caps_dir.join(&file_name), yaml);
        }

        // Patch pack.yaml: add description, author, and capabilities list
        {
            let pack_yaml_path = pack_dir.join("pack.yaml");
            if let Ok(existing) = std::fs::read_to_string(&pack_yaml_path) {
                let mut val: serde_yaml::Value = serde_yaml::from_str(&existing).unwrap_or_default();
                if let serde_yaml::Value::Mapping(ref mut m) = val {
                    if !description.is_empty() {
                        m.insert(serde_yaml::Value::String("description".into()),
                            serde_yaml::Value::String(description.to_string()));
                    }
                    if !author.is_empty() {
                        m.insert(serde_yaml::Value::String("author".into()),
                            serde_yaml::Value::String(author.to_string()));
                    }
                    // Merge discovered capabilities into the pack's capabilities list
                    if !capability_names.is_empty() {
                        let existing_caps: Vec<String> = m.get("capabilities")
                            .and_then(|v| v.as_sequence())
                            .map(|seq| seq.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect())
                            .unwrap_or_default();
                        let mut all_caps = existing_caps;
                        for cn in capability_names {
                            if !all_caps.contains(cn) {
                                all_caps.push(cn.clone());
                            }
                        }
                        m.insert(serde_yaml::Value::String("capabilities".into()),
                            serde_yaml::Value::Sequence(
                                all_caps.iter().map(|s| serde_yaml::Value::String(s.clone())).collect()
                            ));
                    }
                }
                if let Ok(patched) = serde_yaml::to_string(&val) {
                    let _ = std::fs::write(&pack_yaml_path, patched);
                }
            }
        }

        Ok(format!("Pack '{}' created with {} capabilities", name, capability_names.len()))
    }

    fn do_edit_pack_capabilities(&self, pack_name: &str, capability_names: &[String]) -> Result<String> {
        let state = ClixState::load(home_dir())?;
        let pack_dir = state.packs_dir.join(pack_name);
        let pack_yaml_path = pack_dir.join("pack.yaml");
        if !pack_yaml_path.exists() {
            return Err(anyhow::anyhow!("pack.yaml not found for '{}'", pack_name));
        }
        let existing = std::fs::read_to_string(&pack_yaml_path)?;
        let mut val: serde_yaml::Value = serde_yaml::from_str(&existing).unwrap_or_default();
        if let serde_yaml::Value::Mapping(ref mut m) = val {
            m.insert(serde_yaml::Value::String("capabilities".into()),
                serde_yaml::Value::Sequence(
                    capability_names.iter().map(|s| serde_yaml::Value::String(s.clone())).collect()
                ));
        }
        let patched = serde_yaml::to_string(&val)?;
        std::fs::write(&pack_yaml_path, patched)?;
        Ok(format!("Pack '{}' updated with {} capabilities", pack_name, capability_names.len()))
    }

    fn do_install_pack(&self, path_str: &str) -> Result<String> {
        use clix_core::packs::install::install_pack;
        let state = ClixState::load(home_dir())?;
        let path = std::path::PathBuf::from(path_str);
        install_pack(&path, &state.packs_dir)?;
        Ok(format!("Pack installed from {}", path_str))
    }
}

// ─── loaders ─────────────────────────────────────────────────────────────────

fn load_packs_from_dir(packs_dir: &std::path::Path) -> Vec<PackManifest> {
    if !packs_dir.exists() { return vec![]; }
    let Ok(entries) = std::fs::read_dir(packs_dir) else { return vec![]; };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| std::fs::read_to_string(e.path().join("pack.yaml")).ok())
        .filter_map(|s| serde_yaml::from_str::<PackManifest>(&s).ok())
        .collect()
}

fn load_profiles_from_packs(packs_dir: &std::path::Path) -> Vec<ProfileManifest> {
    if !packs_dir.exists() { return vec![]; }
    let Ok(entries) = std::fs::read_dir(packs_dir) else { return vec![]; };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .flat_map(|e| load_dir::<ProfileManifest>(&e.path().join("profiles")).unwrap_or_default())
        .collect()
}

fn load_all_profiles(state: &ClixState) -> Vec<ProfileManifest> {
    let mut all: Vec<ProfileManifest> = load_dir(&state.profiles_dir).unwrap_or_default();
    all.extend(load_profiles_from_packs(&state.packs_dir));
    all
}
