use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use clix_core::loader::build_registry;
use clix_core::manifest::pack::PackManifest;
use clix_core::manifest::profile::{ProfileManifest, ProfileSecretBinding};
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
use crate::tui::screens::infisical_setup::{InfisicalSetupState, InfisicalSetupAction};

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
    Secrets,
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
            Screen::Secrets => 7,
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
            7 => Screen::Secrets,
            _ => Screen::Dashboard,
        }
    }

    pub fn next(&self) -> Self { Screen::from_index((self.sidebar_index() + 1) % 8) }
    pub fn prev(&self) -> Self {
        Screen::from_index(self.sidebar_index().checked_sub(1).unwrap_or(7))
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
    ProfileSecrets(crate::tui::screens::wizards::profile::SecretsEditState),
    CapabilityCreate(CapabilityWizard),
    PackCreate(PackWizard),
    PackEdit { pack_name: String, checklist: crate::tui::widgets::checklist::Checklist },
    InstallPack(String),  // path buffer
    Toast { message: String, is_error: bool, expires_at: std::time::Instant },
    Help,
    InfisicalSetup(InfisicalSetupState),
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
    pub infisical_cfg: Option<clix_core::state::InfisicalConfig>,
    pub connectivity_report: Option<clix_core::secrets::ConnectivityReport>,
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
        let infisical_cfg = state.config.infisical.clone();
        Ok(Self {
            screen: Screen::Dashboard,
            overlay: Overlay::None,
            profiles,
            active_profiles,
            registry,
            packs,
            receipts_preview: vec![],
            infisical_cfg,
            connectivity_report: None,
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
        self.infisical_cfg = new.infisical_cfg;
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
            KeyCode::Char('7') => self.switch_to(Screen::Secrets),
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
            // s = edit profile secrets
            KeyCode::Char('s') if self.screen == Screen::Profiles => {
                self.open_profile_secrets();
            }
            // c = configure Infisical (Dashboard or Broker)
            KeyCode::Char('c') if matches!(self.screen, Screen::Dashboard | Screen::Broker) => {
                self.open_infisical_setup();
            }
            // e = edit Infisical config on Secrets screen
            KeyCode::Char('e') if self.screen == Screen::Secrets => {
                self.open_infisical_setup();
            }
            // t = test connectivity on Secrets screen
            KeyCode::Char('t') if self.screen == Screen::Secrets => {
                if let Some(ref cfg) = self.infisical_cfg.clone() {
                    let report = clix_core::secrets::test_connectivity(cfg);
                    let msg = if report.auth_ok {
                        format!("Connected ({}ms)", report.latency_ms)
                    } else {
                        format!("Error: {}", report.error.as_deref().unwrap_or("unknown"))
                    };
                    let is_err = !report.auth_ok;
                    self.connectivity_report = Some(report);
                    self.toast(&msg, is_err);
                } else {
                    self.toast("Infisical not configured — press e to configure", true);
                }
            }
            // Content navigation — arrow keys + page
            KeyCode::Down => self.cursor_down(),
            KeyCode::Up => self.cursor_up(),
            KeyCode::PageDown => self.cursor_page(15),
            KeyCode::PageUp => self.cursor_page(-15),
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
        use clix_core::manifest::loader::load_dir;
        use clix_core::manifest::capability::CapabilityManifest;

        let Some(pack) = self.packs.get(self.packs_cursor) else { return; };
        let pack_name = pack.name.clone();
        let current_caps: std::collections::HashSet<String> = pack.capabilities.iter().cloned().collect();

        // Load ALL capabilities from disk (unfiltered — registry may be scoped to active profiles)
        let state = ClixState::load(home_dir()).ok();
        let mut all_caps: Vec<CapabilityManifest> = vec![];
        if let Some(ref s) = state {
            let _ = load_dir::<CapabilityManifest>(&s.capabilities_dir).map(|mut v| all_caps.append(&mut v));
            if s.packs_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&s.packs_dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            let _ = load_dir::<CapabilityManifest>(&entry.path().join("capabilities"))
                                .map(|mut v| all_caps.append(&mut v));
                        }
                    }
                }
            }
        }
        all_caps.sort_by(|a, b| a.name.cmp(&b.name));
        all_caps.dedup_by(|a, b| a.name == b.name);

        let items: Vec<ChecklistItem> = all_caps.iter().map(|cap| {
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

        // Pre-filter to pack name so user sees relevant caps immediately
        let mut checklist = Checklist::new(items);
        checklist.filter = pack_name.to_lowercase();
        self.overlay = Overlay::PackEdit { pack_name, checklist };
    }

    fn open_profile_secrets(&mut self) {
        use crate::tui::screens::wizards::profile::SecretsEditState;
        let Some(profile) = self.profiles.get(self.profiles_cursor) else { return; };
        let registry = self.registry.clone();
        let state = SecretsEditState::new(profile, &registry);
        self.overlay = Overlay::ProfileSecrets(state);
    }

    fn open_infisical_setup(&mut self) {
        let state = InfisicalSetupState::new(self.infisical_cfg.as_ref());
        self.overlay = Overlay::InfisicalSetup(state);
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

        // Profile secrets edit overlay
        if let Overlay::ProfileSecrets(ref mut state) = self.overlay {
            use crate::tui::screens::wizards::profile::SecretsEditAction;
            let infisical = self.infisical_cfg.clone();
            let action = state.handle_key(key.code, infisical.as_ref());
            match action {
                SecretsEditAction::Cancel => { self.overlay = Overlay::None; }
                SecretsEditAction::Save(bindings) => {
                    let profile_name = state.profile_name.clone();
                    let res = self.do_save_profile_secrets(&profile_name, &bindings);
                    match res {
                        Ok(msg) => { self.overlay = Overlay::None; let _ = self.reload(); self.toast(&msg, false); }
                        Err(e) => { self.toast(&format!("Error: {e}"), true); }
                    }
                }
                SecretsEditAction::None => {}
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

        // Infisical setup overlay
        if let Overlay::InfisicalSetup(ref mut state) = self.overlay {
            let action = state.handle_key(key.code);
            match action {
                InfisicalSetupAction::Cancel => { self.overlay = Overlay::None; }
                InfisicalSetupAction::Save { site_url, client_id, client_secret, project_id, environment } => {
                    let res = self.do_save_infisical_config(&site_url, &client_id, &client_secret, &project_id, &environment);
                    match res {
                        Ok(msg) => { self.overlay = Overlay::None; let _ = self.reload(); self.toast(&msg, false); }
                        Err(e) => { self.toast(&format!("Error: {e}"), true); }
                    }
                }
                InfisicalSetupAction::None => {}
            }
            return;
        }

        // Wizard key delegation
        let code = key.code;
        // ProfileCreate: pass registry + infisical so the Secrets step can build rows + open picker
        if let Overlay::ProfileCreate(ref mut wiz) = self.overlay {
            // Clone refs so we can borrow self mutably inside
            let registry = self.registry.clone();
            let infisical = self.infisical_cfg.clone();
            let action = wiz.handle_key(code, Some(&registry), infisical.as_ref());
            let result: Option<Result<String>> = match action {
                ProfileWizardAction::Cancel => { self.overlay = Overlay::None; None }
                ProfileWizardAction::Submit { name, description, capabilities, secret_bindings, folder_bindings } => {
                    Some(self.do_create_profile(&name, &description, &capabilities, &secret_bindings, &folder_bindings))
                }
                ProfileWizardAction::None => None,
            };
            if let Some(res) = result {
                match res {
                    Ok(msg) => { self.overlay = Overlay::None; let _ = self.reload(); self.toast(&msg, false); }
                    Err(e) => { self.toast(&format!("Error: {e}"), true); }
                }
            }
            return;
        }

        let result: Option<Result<String>> = match &mut self.overlay {
            Overlay::ProfileCreate(_) => None, // handled above
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

    fn cursor_page(&mut self, delta: i32) {
        let len = self.current_list_len();
        if len == 0 { return; }
        let c = self.cursor_mut();
        *c = ((*c as i32) + delta).max(0).min((len as i32) - 1) as usize;
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

    fn do_create_profile(&self, name: &str, description: &str, capabilities: &[String], secret_bindings: &[ProfileSecretBinding], folder_bindings: &[clix_core::manifest::profile::ProfileFolderBinding]) -> Result<String> {
        let state = ClixState::load(home_dir())?;
        let manifest = ProfileManifest {
            name: name.to_string(),
            version: 1,
            description: if description.is_empty() { None } else { Some(description.to_string()) },
            capabilities: capabilities.to_vec(),
            workflows: vec![],
            settings: serde_json::Value::Null,
            isolation_defaults: Default::default(),
            secret_bindings: secret_bindings.to_vec(),
            folder_bindings: folder_bindings.to_vec(),
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

    fn do_save_profile_secrets(&self, profile_name: &str, bindings: &[ProfileSecretBinding]) -> Result<String> {
        let state = ClixState::load(home_dir())?;
        let path = state.profiles_dir.join(format!("{}.yaml", profile_name));
        if !path.exists() {
            return Err(anyhow::anyhow!("Profile file not found: {}", path.display()));
        }
        let existing = std::fs::read_to_string(&path)?;
        let mut val: serde_yaml::Value = serde_yaml::from_str(&existing)?;
        if let serde_yaml::Value::Mapping(ref mut m) = val {
            let bindings_yaml = serde_yaml::to_value(bindings)?;
            m.insert(serde_yaml::Value::String("secretBindings".into()), bindings_yaml);
        }
        std::fs::write(&path, serde_yaml::to_string(&val)?)?;
        Ok(format!("Profile '{}' secrets updated ({} bindings)", profile_name, bindings.len()))
    }

    fn do_save_infisical_config(&self, site_url: &str, client_id: &str, client_secret: &str, project_id: &str, environment: &str) -> Result<String> {
        let state = ClixState::load(home_dir())?;
        // Try keyring first (Linux only)
        #[cfg(target_os = "linux")]
        let keyring_ok = {
            use clix_core::secrets::keyring::{store_credentials, KeyringResult};
            matches!(store_credentials(client_id, client_secret), KeyringResult::Ok)
        };
        #[cfg(not(target_os = "linux"))]
        let keyring_ok = false;

        // Read or create config, patch infisical fields
        let config_path = &state.config_path;
        let existing = if config_path.exists() {
            std::fs::read_to_string(config_path)?
        } else {
            String::new()
        };
        let mut val: serde_yaml::Value = if existing.is_empty() {
            serde_yaml::Value::Mapping(Default::default())
        } else {
            serde_yaml::from_str(&existing)?
        };
        let infisical_map = {
            let mut m = serde_yaml::Mapping::new();
            m.insert(sv("siteUrl"), sv(site_url));
            if !project_id.is_empty() {
                m.insert(sv("defaultProjectId"), sv(project_id));
            }
            m.insert(sv("defaultEnvironment"), sv(environment));
            // Only store credentials in config.yaml if keyring failed
            if !keyring_ok {
                m.insert(sv("clientId"), sv(client_id));
                m.insert(sv("clientSecret"), sv(client_secret));
            }
            serde_yaml::Value::Mapping(m)
        };
        if let serde_yaml::Value::Mapping(ref mut root) = val {
            root.insert(sv("infisical"), infisical_map);
        }
        std::fs::create_dir_all(config_path.parent().unwrap_or(config_path))?;
        let config_yaml = serde_yaml::to_string(&val)?;
        std::fs::write(config_path, &config_yaml)?;
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(config_path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(config_path, perms);
            }
        }

        // Post-save connectivity check
        let cfg_for_test = clix_core::state::InfisicalConfig {
            site_url: site_url.to_string(),
            client_id: if keyring_ok || client_id.is_empty() { None } else { Some(client_id.to_string()) },
            client_secret: if keyring_ok || client_secret.is_empty() { None } else { Some(client_secret.to_string()) },
            default_project_id: if project_id.is_empty() { None } else { Some(project_id.to_string()) },
            default_environment: environment.to_string(),
        };
        // Build effective cfg (merge keyring creds if they were just stored)
        let effective_cfg = if keyring_ok {
            let mut c = cfg_for_test;
            c.client_id = Some(client_id.to_string());
            c.client_secret = Some(client_secret.to_string());
            c
        } else {
            cfg_for_test
        };
        let report = clix_core::secrets::test_connectivity(&effective_cfg);
        if report.auth_ok {
            let msg = format!("✓ Infisical connected ({}ms){}", report.latency_ms,
                if keyring_ok { " — credentials secured in keyring" } else { " — credentials in config.yaml" });
            Ok(msg)
        } else {
            let err_short: String = report.error.as_deref().unwrap_or("auth failed").chars().take(60).collect();
            Ok(format!("Saved (✗ connectivity: {})", err_short))
        }
    }

    fn do_install_pack(&self, path_str: &str) -> Result<String> {
        use clix_core::packs::install::install_pack;
        let state = ClixState::load(home_dir())?;
        let path = std::path::PathBuf::from(path_str);
        install_pack(&path, &state.packs_dir)?;
        Ok(format!("Pack installed from {}", path_str))
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn sv(s: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(s.to_string())
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
    use std::collections::HashMap;
    // Global profiles win over pack-shipped ones (user-level override)
    let mut by_name: HashMap<String, ProfileManifest> = HashMap::new();
    for p in load_dir::<ProfileManifest>(&state.profiles_dir).unwrap_or_default() {
        by_name.insert(p.name.clone(), p);
    }
    for p in load_profiles_from_packs(&state.packs_dir) {
        by_name.entry(p.name.clone()).or_insert(p);
    }
    let mut profiles: Vec<ProfileManifest> = by_name.into_values().collect();
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    profiles
}
