use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use clix_core::loader::{build_registry, build_workflow_registry};
use clix_core::manifest::pack::PackManifest;
use clix_core::manifest::profile::{ProfileManifest, ProfileSecretBinding};
use clix_core::registry::{CapabilityRegistry, WorkflowRegistry};
use clix_core::state::{home_dir, ClixState};
use clix_core::manifest::loader::load_dir;
use clix_core::manifest::capability::{
    CapabilityManifest, Backend, SideEffectClass,
};
use clix_core::packs::scaffold::{scaffold_pack, Preset};

use crate::tui::screens::wizards::pack::{PackWizard, PackWizardAction};
use crate::tui::screens::wizards::profile::{ProfileWizard, ProfileWizardAction, SecretsEditAction};
use crate::tui::screens::wizards::capability::{CapabilityWizard, CapWizardAction};
use crate::tui::screens::infisical_setup::{InfisicalSetupState, InfisicalSetupAction, SubmitState};
use crate::tui::work::{WorkPool, WorkRequest, WorkResult, next_job_id, JobId};

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
    Help,
    InfisicalSetup(InfisicalSetupState),
    SecretsTreeBrowser(crate::tui::widgets::secrets_tree::SecretsTree),
    InfisicalAccounts(crate::tui::screens::infisical_accounts::InfisicalAccountsState),
}

impl Overlay {
    pub fn is_open(&self) -> bool { !matches!(self, Overlay::None) }
}

// ─── toast ────────────────────────────────────────────────────────────────────

pub struct ToastState {
    pub message: String,
    pub is_error: bool,
    pub expires_at: std::time::Instant,
}

// ─── focus ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    Sidebar,
    Content,
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
    pub workflow_registry: WorkflowRegistry,
    pub infisical_cfg: Option<clix_core::state::InfisicalConfig>,
    pub infisical_profile_name: String,
    pub connectivity_report: Option<clix_core::secrets::ConnectivityReport>,
    pub broker_status: Option<crate::tui::screens::broker::BrokerScreenState>,
    /// Receipt IDs with PendingApproval status, for the approval banner.
    pub pending_approval_ids: Vec<String>,
    // per-screen cursors
    pub profiles_cursor: usize,
    pub caps_view: CapView,
    pub caps_cursor: usize,
    pub packs_cursor: usize,
    pub receipts_cursor: usize,
    pub workflows_cursor: usize,
    pub should_quit: bool,
    // async work
    pub work: WorkPool,
    pub dropped_jobs: std::collections::HashSet<JobId>,
    /// Tracks the in-flight receipt approval (receipt_id_str, job_id)
    pub approving_receipt: Option<(String, JobId)>,
    // navigation
    pub focus: Focus,
    pub toast_state: Option<ToastState>,
    // discard confirmation — rendered over the current overlay; keeps overlay intact
    pub confirming_discard: bool,
    // git sync badge — polled async on startup and after each sync
    pub git_badge: Option<GitBadge>,
    pub git_syncing: bool,
}

#[derive(Debug, Clone)]
pub struct GitBadge {
    pub configured: bool,
    pub dirty: usize,
    pub ahead: usize,
    pub behind: usize,
}

impl App {
    pub fn new() -> Result<Self> {
        let mut state = ClixState::load(home_dir())?;
        if state.config.active_profiles.is_empty() {
            let base_pack_dir = state.packs_dir.join("base");
            if base_pack_dir.exists() {
                state.config.active_profiles.push("base".to_string());
                state.save_config()?;
            }
        }
        let registry = build_registry(&state)?;
        let workflow_registry = build_workflow_registry(&state).unwrap_or_default();
        let packs = load_packs_from_dir(&state.packs_dir);
        let profiles = load_all_profiles(&state);
        let active_profiles = state.config.active_profiles.clone();
        let infisical_profile_name = state.config.active_infisical.clone().unwrap_or_else(|| "default".to_string());
        let infisical_cfg = state.config.infisical().active_profile().cloned();
        // Load receipts from DB for the Receipts screen and pending-approval banner
        let (receipts_preview, pending_approval_ids) = load_receipts(&state.receipts_db);
        let work = WorkPool::new();
        // Kick off initial git status poll immediately
        work.dispatch(WorkRequest::GitPoll { home: state.home.clone() });
        Ok(Self {
            screen: Screen::Dashboard,
            overlay: Overlay::None,
            profiles,
            active_profiles,
            registry,
            packs,
            receipts_preview,
            workflow_registry,
            infisical_cfg,
            infisical_profile_name,
            connectivity_report: None,
            broker_status: None,
            pending_approval_ids,
            profiles_cursor: 0,
            caps_view: CapView::Namespaces,
            caps_cursor: 0,
            packs_cursor: 0,
            receipts_cursor: 0,
            workflows_cursor: 0,
            should_quit: false,
            work,
            dropped_jobs: std::collections::HashSet::new(),
            approving_receipt: None,
            focus: Focus::Sidebar,
            toast_state: None,
            confirming_discard: false,
            git_badge: None,
            git_syncing: false,
        })
    }

    pub fn reload(&mut self) -> Result<()> {
        let new = Self::new()?;
        self.profiles = new.profiles;
        self.active_profiles = new.active_profiles;
        self.registry = new.registry;
        self.packs = new.packs;
        self.infisical_cfg = new.infisical_cfg;
        self.pending_approval_ids = new.pending_approval_ids;
        self.receipts_preview = new.receipts_preview;
        self.workflow_registry = new.workflow_registry;
        self.profiles_cursor = 0;
        self.caps_view = CapView::Namespaces;
        self.caps_cursor = 0;
        self.packs_cursor = 0;
        self.receipts_cursor = 0;
        self.workflows_cursor = 0;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn cursor(&self) -> usize {
        match self.screen {
            Screen::Profiles => self.profiles_cursor,
            Screen::Capabilities => self.caps_cursor,
            Screen::Packs => self.packs_cursor,
            Screen::Receipts => self.receipts_cursor,
            Screen::Workflows => self.workflows_cursor,
            _ => 0,
        }
    }

    fn cursor_mut(&mut self) -> &mut usize {
        match self.screen {
            Screen::Profiles => &mut self.profiles_cursor,
            Screen::Capabilities => &mut self.caps_cursor,
            Screen::Packs => &mut self.packs_cursor,
            Screen::Receipts => &mut self.receipts_cursor,
            Screen::Workflows => &mut self.workflows_cursor,
            _ => &mut self.profiles_cursor,  // unused placeholder
        }
    }

    fn toast(&mut self, msg: &str, is_error: bool) {
        self.toast_state = Some(ToastState {
            message: msg.to_string(),
            is_error,
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(3),
        });
    }

    pub fn tick(&mut self) {
        // Dismiss expired toasts
        if let Some(ref t) = self.toast_state {
            if std::time::Instant::now() >= t.expires_at {
                self.toast_state = None;
            }
        }
        // Drain async work results (collect first to free the borrow on result_rx)
        let work_results: Vec<WorkResult> = self.work.result_rx.try_iter().collect();
        for result in work_results {
            match result {
                WorkResult::GitPolled { configured, dirty, ahead, behind } => {
                    self.git_badge = Some(GitBadge { configured, dirty, ahead, behind });
                }
                WorkResult::GitSynced { ok, message } => {
                    self.git_syncing = false;
                    let short = if ok { "Synced with remote".to_string() }
                        else { message.lines().next().unwrap_or("sync failed").chars().take(70).collect::<String>() };
                    self.toast(&short, !ok);
                    self.work.dispatch(WorkRequest::GitPoll { home: home_dir() });
                }
                WorkResult::ConnectivityPinged { ok, latency_ms, error } => {
                    let msg = if ok {
                        format!("✓ Connected ({}ms)", latency_ms)
                    } else {
                        format!("✗ {}", error.as_deref().unwrap_or("auth failed"))
                    };
                    self.toast(&msg, !ok);
                }
                WorkResult::SecretFoldersLoaded { job_id, folders, error } => {
                    // Route to whichever picker/tree is currently open
                    let mut delivered = false;
                    if let Overlay::SecretsTreeBrowser(ref mut tree) = self.overlay {
                        if tree.deliver_folders(job_id, folders.clone(), error.clone()) {
                            delivered = true;
                        }
                    }
                    if !delivered {
                        if let Overlay::ProfileCreate(ref mut wiz) = self.overlay {
                            if wiz.deliver_tree_folders(job_id, folders.clone(), error.clone()) {
                                delivered = true;
                            }
                        }
                    }
                    if !delivered {
                        if let Overlay::ProfileSecrets(ref mut state) = self.overlay {
                            state.deliver_tree_folders(job_id, folders, error);
                        }
                    }
                }
                WorkResult::SecretNamesLoaded { job_id, names, error } => {
                    let mut delivered = false;
                    if let Overlay::SecretsTreeBrowser(ref mut tree) = self.overlay {
                        if tree.deliver_names(job_id, names.clone(), error.clone()) {
                            delivered = true;
                        }
                    }
                    if !delivered {
                        if let Overlay::ProfileCreate(ref mut wiz) = self.overlay {
                            if wiz.deliver_tree_names(job_id, names.clone(), error.clone()) {
                                delivered = true;
                            }
                        }
                    }
                    if !delivered {
                        if let Overlay::ProfileSecrets(ref mut state) = self.overlay {
                            state.deliver_tree_names(job_id, names, error);
                        }
                    }
                }
                WorkResult::HelpParsed { job_id, command, subcmds } => {
                    if let Overlay::PackCreate(ref mut wiz) = self.overlay {
                        wiz.deliver_help(job_id, &command, subcmds);
                    }
                }
                WorkResult::ReceiptApproved { job_id, id, ok, error } => {
                    if let Some((ref pending_id, pending_job)) = self.approving_receipt.clone() {
                        if pending_job == job_id {
                            self.approving_receipt = None;
                            if ok {
                                let id_str = id.to_string();
                                self.pending_approval_ids.retain(|i| i != &id_str);
                                self.toast(&format!("Approved: {}", &id_str[..8.min(id_str.len())]), false);
                            } else {
                                let err = error.as_deref().unwrap_or("unknown error");
                                self.toast(&format!("Approve failed: {err}"), true);
                            }
                            let _ = pending_id; // suppress unused warning
                        }
                    }
                }
                WorkResult::InfisicalTested { job_id, ok, latency_ms, keyring_used, error } => {
                    if self.dropped_jobs.remove(&job_id) {
                        continue; // user cancelled — discard
                    }
                    let is_our_job = if let Overlay::InfisicalSetup(ref state) = self.overlay {
                        matches!(&state.submit_state, SubmitState::Saving { job_id: jid } if *jid == job_id)
                    } else {
                        false
                    };
                    if !is_our_job { continue; }
                    if ok {
                        self.overlay = Overlay::None;
                        let keyring_note = if keyring_used { " — credentials in keyring" } else { " — credentials in config.yaml" };
                        let msg = format!("✓ Infisical connected ({}ms){}", latency_ms, keyring_note);
                        let _ = self.reload();
                        self.toast(&msg, false);
                    } else {
                        let err = error.as_deref().unwrap_or("auth failed").chars().take(60).collect::<String>();
                        if let Overlay::InfisicalSetup(ref mut state) = self.overlay {
                            state.submit_state = SubmitState::Err(err);
                        }
                    }
                }
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        // Ctrl+G = full git sync (add → commit → pull --rebase → push)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('g') {
            self.git_sync();
            return;
        }

        // Delegate to overlay first (overlay implies Focus::Content-like routing)
        if self.overlay.is_open() {
            self.handle_overlay_key(key);
            return;
        }

        // Sidebar focus: up/down navigate sidebar, enter/right enters content
        if self.focus == Focus::Sidebar {
            match key.code {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Char('?') => self.overlay = Overlay::Help,
                // Number shortcuts jump directly to screen (and enter content)
                KeyCode::Char('1') => { self.switch_to(Screen::Profiles); self.focus = Focus::Content; }
                KeyCode::Char('2') => { self.switch_to(Screen::Capabilities); self.focus = Focus::Content; }
                KeyCode::Char('3') => { self.switch_to(Screen::Packs); self.focus = Focus::Content; }
                KeyCode::Char('4') => { self.switch_to(Screen::Receipts); self.focus = Focus::Content; }
                KeyCode::Char('5') => { self.switch_to(Screen::Workflows); self.focus = Focus::Content; }
                KeyCode::Char('6') => { self.switch_to(Screen::Broker); self.focus = Focus::Content; }
                KeyCode::Char('7') => { self.switch_to(Screen::Secrets); self.focus = Focus::Content; }
                KeyCode::Char('0') => { self.switch_to(Screen::Dashboard); self.focus = Focus::Content; }
                // Tab / arrow keys only move the sidebar cursor — never steal focus
                KeyCode::Tab => self.screen = self.screen.next(),
                KeyCode::BackTab => self.screen = self.screen.prev(),
                KeyCode::Up => self.screen = self.screen.prev(),
                KeyCode::Down => self.screen = self.screen.next(),
                // Enter or right arrow → enter content (auto-focus applies here)
                KeyCode::Enter | KeyCode::Right => self.focus = Focus::Content,
                _ => {}
            }
            return;
        }

        // Content focus
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('r') if self.screen == Screen::Broker => {
                self.broker_status = Some(crate::tui::screens::broker::BrokerScreenState::probe());
                self.toast("Broker status refreshed", false);
            }
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
            // Tab cycles screens
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
            // t = test connectivity on Secrets screen (async)
            KeyCode::Char('t') if self.screen == Screen::Secrets => {
                if let Some(ref cfg) = self.infisical_cfg.clone() {
                    self.work.dispatch(WorkRequest::PingConnectivity { cfg: cfg.clone() });
                    self.toast("Testing connectivity…", false);
                } else {
                    self.toast("Infisical not configured — press e to configure", true);
                }
            }
            // b = browse secrets tree on Secrets screen
            KeyCode::Char('b') if self.screen == Screen::Secrets => {
                self.open_secrets_tree();
            }
            // m = manage Infisical accounts
            KeyCode::Char('m') if self.screen == Screen::Secrets => {
                self.open_infisical_accounts();
            }
            // A = approve first pending receipt on Receipts screen (async)
            KeyCode::Char('A') if self.screen == Screen::Receipts => {
                if self.approving_receipt.is_some() {
                    self.toast("Approval already in progress…", false);
                } else if let Some(id) = self.pending_approval_ids.first().cloned() {
                    use uuid::Uuid;
                    match id.parse::<Uuid>() {
                        Ok(uid) => {
                            let job_id = next_job_id();
                            self.approving_receipt = Some((id, job_id));
                            self.work.dispatch(WorkRequest::ApproveReceipt {
                                id: uid,
                                approver: whoami::username(),
                                job_id,
                            });
                            self.toast("Sending approval…", false);
                        }
                        Err(_) => self.toast("Invalid receipt id", true),
                    }
                } else {
                    self.toast("No pending approvals", false);
                }
            }
            // Broker screen keys
            KeyCode::Char('s') if self.screen == Screen::Broker => {
                self.toast("run `clix broker start` in terminal to start broker", false);
            }
            KeyCode::Char('x') if self.screen == Screen::Broker => {
                self.toast("run `clix broker stop` in terminal to stop broker", false);
            }
            // Content navigation — arrow keys + page
            KeyCode::Down => self.cursor_down(),
            KeyCode::Up => self.cursor_up(),
            KeyCode::Left => self.focus = Focus::Sidebar,
            KeyCode::PageDown => self.cursor_page(15),
            KeyCode::PageUp => self.cursor_page(-15),
            KeyCode::Enter => self.handle_enter(),
            KeyCode::Esc | KeyCode::Backspace => self.handle_back_or_sidebar(),
            _ => {}
        }
    }

    fn switch_to(&mut self, screen: Screen) {
        // Screens with no list to navigate have no use for sidebar focus —
        // enter content focus automatically so action keys (e, m, t, b…) work immediately.
        if matches!(screen, Screen::Secrets | Screen::Dashboard | Screen::Broker) {
            self.focus = Focus::Content;
        }
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
            if s.storage.exists(&s.packs_dir) {
                if let Ok(paths) = s.storage.list(&s.packs_dir) {
                    for path in paths {
                        if s.storage.is_dir(&path) {
                            let _ = load_dir::<CapabilityManifest>(&path.join("capabilities"))
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

        // Pre-filter: derive a common namespace prefix from the pack's selected capabilities
        // so the checklist opens already scoped to this pack's caps (handles gcloud.aiplatform.* etc.)
        let filter = {
            let mut it = current_caps.iter().map(|s| s.as_str());
            if let Some(first) = it.next() {
                let mut common = first.to_string();
                for cap in it {
                    let shared: String = common.chars().zip(cap.chars())
                        .take_while(|(a, b)| a == b)
                        .map(|(a, _)| a)
                        .collect();
                    common = if let Some(pos) = shared.rfind('.') {
                        shared[..pos].to_string()
                    } else {
                        shared
                    };
                }
                common
            } else {
                String::new()
            }
        };
        let mut checklist = Checklist::new(items);
        checklist.filter = filter;
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

    fn open_secrets_tree(&mut self) {
        use crate::tui::widgets::secrets_tree::{SecretsTree, TreeMode};
        let Some(ref cfg) = self.infisical_cfg.clone() else {
            self.toast("Infisical not configured — press e to configure", true);
            return;
        };
        let project_id = cfg.default_project_id.clone().unwrap_or_default();
        if project_id.is_empty() {
            self.toast("No project_id configured — press e to set one", true);
            return;
        }
        let mut tree = SecretsTree::new(&project_id, &cfg.default_environment, TreeMode::Browse);
        let fid = next_job_id();
        let nid = next_job_id();
        tree.initial_load_ids(fid, nid);
        self.work.dispatch(WorkRequest::LoadSecretFolders {
            cfg: cfg.clone(), project_id: project_id.clone(),
            environment: cfg.default_environment.clone(), path: "/".to_string(), job_id: fid,
        });
        self.work.dispatch(WorkRequest::LoadSecretNames {
            cfg: cfg.clone(), project_id,
            environment: cfg.default_environment.clone(), path: "/".to_string(), job_id: nid,
        });
        self.overlay = Overlay::SecretsTreeBrowser(tree);
    }

    fn git_sync(&mut self) {
        if self.git_syncing { return; }
        let home = home_dir();
        if !home.join(".git").exists() {
            self.toast("Git sync not configured — run `clix sync init <url>`", true);
            return;
        }
        let branch = ClixState::load(home_dir()).ok()
            .map(|s| s.config.git_branch.clone())
            .unwrap_or_else(|| "main".to_string());
        self.git_syncing = true;
        self.toast("Syncing with remote…", false);
        self.work.dispatch(WorkRequest::GitSync { home, branch });
    }

    fn open_infisical_accounts(&mut self) {
        use crate::tui::screens::infisical_accounts::InfisicalAccountsState;
        let state = match ClixState::load(home_dir()) {
            Ok(s) => {
                let profiles: Vec<_> = s.config.infisical_profiles.into_iter().collect();
                let active = s.config.active_infisical;
                InfisicalAccountsState::new(profiles, active)
            }
            Err(_) => InfisicalAccountsState::new(vec![], None),
        };
        self.overlay = Overlay::InfisicalAccounts(state);
    }

    fn handle_infisical_accounts(&mut self, key: crossterm::event::KeyEvent) {
        use crate::tui::screens::infisical_accounts::AccountsAction;
        let action = if let Overlay::InfisicalAccounts(ref mut state) = self.overlay {
            state.handle_key(key.code)
        } else {
            return;
        };

        match action {
            AccountsAction::Cancel => { self.overlay = Overlay::None; }
            AccountsAction::SetActive(name) => {
                if let Ok(mut s) = ClixState::load(home_dir()) {
                    s.config.active_infisical = Some(name.clone());
                    let _ = s.save_config();
                }
                let _ = self.reload();
                if let Overlay::InfisicalAccounts(ref mut state) = self.overlay {
                    state.status = Some((format!("Active profile set to '{}'", name), false));
                }
            }
            AccountsAction::Remove(name) => {
                if let Ok(mut s) = ClixState::load(home_dir()) {
                    s.config.infisical_profiles.remove(&name);
                    if s.config.active_infisical.as_deref() == Some(&name) {
                        s.config.active_infisical = s.config.infisical_profiles.keys().next().cloned();
                    }
                    let _ = s.save_config();
                    #[cfg(target_os = "linux")]
                    { let _ = clix_core::secrets::keyring::delete_credentials(&name); }
                }
                let _ = self.reload();
                self.open_infisical_accounts();
                if let Overlay::InfisicalAccounts(ref mut state) = self.overlay {
                    state.status = Some((format!("Profile '{}' removed", name), false));
                }
            }
            AccountsAction::Save { name, site_url, service_token, project_id, environment } => {
                if let Ok(mut s) = ClixState::load(home_dir()) {
                    let cfg = s.config.infisical_profiles.entry(name.clone()).or_insert_with(|| {
                        clix_core::state::InfisicalConfig {
                            site_url: String::new(), client_id: None, client_secret: None,
                            service_token: None,
                            default_project_id: None, default_environment: "dev".to_string(),
                        }
                    });
                    cfg.site_url = site_url;
                    if !project_id.is_empty() { cfg.default_project_id = Some(project_id); }
                    cfg.default_environment = environment;
                    if s.config.active_infisical.is_none() {
                        s.config.active_infisical = Some(name.clone());
                    }
                    if !service_token.is_empty() {
                        #[cfg(target_os = "linux")]
                        {
                            use clix_core::secrets::keyring::{store_service_token, KeyringResult};
                            if !matches!(store_service_token(&name, &service_token), KeyringResult::Ok) {
                                cfg.service_token = Some(service_token);
                            }
                        }
                        #[cfg(not(target_os = "linux"))]
                        { cfg.service_token = Some(service_token); }
                    }
                    let _ = s.save_config();
                }
                let _ = self.reload();
                self.open_infisical_accounts();
                if let Overlay::InfisicalAccounts(ref mut state) = self.overlay {
                    state.status = Some((format!("Profile '{}' saved", name), false));
                }
            }
            AccountsAction::Test(name) => {
                if let Ok(s) = ClixState::load(home_dir()) {
                    if let Some(cfg) = s.config.infisical_profiles.get(&name) {
                        let report = clix_core::secrets::test_connectivity(cfg);
                        let (msg, is_err) = if report.auth_ok {
                            (format!("✓ '{}' connected ({}ms)", name, report.latency_ms), false)
                        } else {
                            (format!("✗ '{}': {}", name, report.error.as_deref().unwrap_or("auth failed")), true)
                        };
                        if let Overlay::InfisicalAccounts(ref mut state) = self.overlay {
                            state.status = Some((msg, is_err));
                        }
                    }
                }
            }
            AccountsAction::None => {}
        }
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
        // Confirm-discard dialog takes priority
        if self.confirming_discard {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.confirming_discard = false;
                    self.overlay = Overlay::None;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.confirming_discard = false;
                }
                _ => {}
            }
            return;
        }

        // Dismiss help overlay on any key
        if matches!(self.overlay, Overlay::Help) {
            self.overlay = Overlay::None;
            return;
        }

        // Infisical accounts manager
        if matches!(self.overlay, Overlay::InfisicalAccounts(_)) {
            self.handle_infisical_accounts(key);
            return;
        }

        // Secrets tree browser
        if let Overlay::SecretsTreeBrowser(ref mut tree) = self.overlay {
            use crate::tui::widgets::secrets_tree::SecretsTreeAction;
            let cfg = self.infisical_cfg.clone();
            let action = tree.handle_key(key.code, cfg.as_ref());
            match action {
                SecretsTreeAction::Cancelled => { self.overlay = Overlay::None; }
                SecretsTreeAction::NeedsLoad(path) => {
                    if let Overlay::SecretsTreeBrowser(ref mut tree) = self.overlay {
                        let (fid, nid) = tree.request_load(&path);
                        if let Some(ref cfg) = self.infisical_cfg.clone() {
                            self.work.dispatch(WorkRequest::LoadSecretFolders {
                                cfg: cfg.clone(), project_id: tree.project_id.clone(),
                                environment: tree.environment.clone(), path: path.clone(), job_id: fid,
                            });
                            self.work.dispatch(WorkRequest::LoadSecretNames {
                                cfg: cfg.clone(), project_id: tree.project_id.clone(),
                                environment: tree.environment.clone(), path, job_id: nid,
                            });
                        }
                    }
                }
                SecretsTreeAction::Selected(_) | SecretsTreeAction::SelectedMany(_)
                | SecretsTreeAction::SelectedFolder { .. } => {
                    // Browse mode — no-op for selection; Bind mode wired via profile wizard
                    self.overlay = Overlay::None;
                }
                SecretsTreeAction::None => {}
            }
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
        let mut handled_secrets_overlay = false;
        if let Overlay::ProfileSecrets(ref mut state) = self.overlay {
            handled_secrets_overlay = true;
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
                SecretsEditAction::TreeNeedsLoad { project_id, environment, path, folders_job, names_job } => {
                    if let Some(ref cfg) = self.infisical_cfg.clone() {
                        self.work.dispatch(WorkRequest::LoadSecretFolders {
                            cfg: cfg.clone(), project_id: project_id.clone(),
                            environment: environment.clone(), path: path.clone(), job_id: folders_job,
                        });
                        self.work.dispatch(WorkRequest::LoadSecretNames {
                            cfg: cfg.clone(), project_id, environment, path, job_id: names_job,
                        });
                    }
                }
                SecretsEditAction::None => {}
            }
        }
        if handled_secrets_overlay { return; }

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
        let infisical_action = if let Overlay::InfisicalSetup(ref mut state) = self.overlay {
            Some(state.handle_key(key.code))
        } else {
            None
        };
        if let Some(action) = infisical_action {
            match action {
                InfisicalSetupAction::Cancel => {
                    let (is_dirty, is_saving) = if let Overlay::InfisicalSetup(ref state) = self.overlay {
                        let saving = matches!(&state.submit_state, SubmitState::Saving { .. });
                        (state.is_dirty(), saving)
                    } else {
                        (false, false)
                    };
                    if is_saving {
                        // Drop the in-flight job and close
                        if let Overlay::InfisicalSetup(ref state) = self.overlay {
                            if let SubmitState::Saving { job_id } = &state.submit_state {
                                self.dropped_jobs.insert(*job_id);
                            }
                        }
                        self.overlay = Overlay::None;
                    } else if is_dirty {
                        self.confirming_discard = true; // show confirm dialog, keep overlay intact
                    } else {
                        self.overlay = Overlay::None;
                    }
                }
                InfisicalSetupAction::Save { site_url, service_token, project_id, environment } => {
                    match self.do_write_infisical_config(&site_url, &service_token, &project_id, &environment) {
                        Ok(effective_cfg) => {
                            let job_id = next_job_id();
                            if let Overlay::InfisicalSetup(ref mut state) = self.overlay {
                                state.submit_state = SubmitState::Saving { job_id };
                                state.status = None;
                            }
                            self.work.dispatch(WorkRequest::TestInfisical { cfg: effective_cfg, job_id });
                        }
                        Err(e) => {
                            if let Overlay::InfisicalSetup(ref mut state) = self.overlay {
                                state.status = Some(format!("Save failed: {e}"));
                                state.status_is_error = true;
                            }
                        }
                    }
                }
                InfisicalSetupAction::None => {}
            }
            return;
        }

        // Wizard key delegation
        let code = key.code;
        // ProfileCreate overlay
        let mut handled_profile_create = false;
        if let Overlay::ProfileCreate(ref mut wiz) = self.overlay {
            handled_profile_create = true;
            let registry = self.registry.clone();
            let infisical = self.infisical_cfg.clone();
            let is_dirty = wiz.is_dirty();
            let action = wiz.handle_key(code, Some(&registry), infisical.as_ref());
            let result: Option<Result<String>> = match action {
                ProfileWizardAction::Cancel => {
                    if is_dirty { self.confirming_discard = true; } else { self.overlay = Overlay::None; }
                    None
                }
                ProfileWizardAction::Submit { name, description, capabilities, secret_bindings, folder_bindings } => {
                    Some(self.do_create_profile(&name, &description, &capabilities, &secret_bindings, &folder_bindings))
                }
                ProfileWizardAction::TreeNeedsLoad { project_id, environment, path, folders_job, names_job } => {
                    if let Some(ref cfg) = self.infisical_cfg.clone() {
                        self.work.dispatch(WorkRequest::LoadSecretFolders {
                            cfg: cfg.clone(), project_id: project_id.clone(),
                            environment: environment.clone(), path: path.clone(), job_id: folders_job,
                        });
                        self.work.dispatch(WorkRequest::LoadSecretNames {
                            cfg: cfg.clone(), project_id, environment, path, job_id: names_job,
                        });
                    }
                    None
                }
                ProfileWizardAction::None => None,
            };
            if let Some(res) = result {
                match res {
                    Ok(msg) => { self.overlay = Overlay::None; let _ = self.reload(); self.toast(&msg, false); }
                    Err(e) => { self.toast(&format!("Error: {e}"), true); }
                }
            }
        }
        if handled_profile_create { return; }

        // CapabilityCreate — two-step borrow to support dirty-tracking on cancel
        let cap_action_with_dirty = if let Overlay::CapabilityCreate(ref mut wiz) = self.overlay {
            let dirty = wiz.is_dirty();
            Some((wiz.handle_key(code), dirty))
        } else { None };
        if let Some((action, is_dirty)) = cap_action_with_dirty {
            match action {
                CapWizardAction::Cancel => {
                    if is_dirty { self.confirming_discard = true; } else { self.overlay = Overlay::None; }
                }
                CapWizardAction::Submit { name, description, command, args, risk, side_effect } => {
                    match self.do_create_capability(&name, &description, &command, &args, &risk, &side_effect) {
                        Ok(msg) => { self.overlay = Overlay::None; let _ = self.reload(); self.toast(&msg, false); }
                        Err(e) => { self.toast(&format!("Error: {e}"), true); }
                    }
                }
                CapWizardAction::None => {}
            }
            return;
        }

        // PackCreate — two-step borrow; also dispatches async help probes
        let pack_action_info = if let Overlay::PackCreate(ref mut wiz) = self.overlay {
            let dirty = wiz.is_dirty();
            Some((wiz.handle_key(code), dirty))
        } else { None };
        if let Some((action, is_dirty)) = pack_action_info {
            match action {
                PackWizardAction::Cancel => {
                    if is_dirty { self.confirming_discard = true; } else { self.overlay = Overlay::None; }
                }
                PackWizardAction::Submit { name, description, author, preset, seed_command, capability_names } => {
                    match self.do_create_pack(&name, &description, &author, preset, &seed_command, &capability_names) {
                        Ok(msg) => { self.overlay = Overlay::None; let _ = self.reload(); self.toast(&msg, false); }
                        Err(e) => { self.toast(&format!("Error: {e}"), true); }
                    }
                }
                PackWizardAction::ParseHelpFor(commands) => {
                    for cmd in commands {
                        let job_id = next_job_id();
                        self.work.dispatch(WorkRequest::ParseHelp { command: cmd, job_id });
                    }
                }
                PackWizardAction::None => {}
            }
            return;
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
            Screen::Receipts => self.receipts_preview.len(),
            Screen::Workflows => self.workflow_registry.all().len(),
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
                    let saved_cursor = self.profiles_cursor;
                    match self.toggle_profile(&name) {
                        Ok(_) => {
                            let _ = self.reload();
                            // Restore cursor position after reload resets it to 0
                            self.profiles_cursor = saved_cursor.min(self.profiles.len().saturating_sub(1));
                        }
                        Err(e) => self.toast(&format!("Toggle failed: {e}"), true),
                    }
                }
            }
            _ => {}
        }
    }


    fn handle_back_or_sidebar(&mut self) {
        // For Capabilities: pop drill level first; once at top, return to sidebar
        if self.screen == Screen::Capabilities {
            match self.caps_view.clone() {
                CapView::Detail(name) => {
                    self.caps_view = CapView::Listing(CapabilityRegistry::group_key(&name));
                    self.caps_cursor = 0;
                    return;
                }
                CapView::Listing(_) => {
                    self.caps_view = CapView::Namespaces;
                    self.caps_cursor = 0;
                    return;
                }
                CapView::Namespaces => {} // fall through to sidebar
            }
        }
        // Return focus to sidebar
        self.focus = Focus::Sidebar;
    }

    pub fn toggle_profile(&mut self, name: &str) -> Result<()> {
        let mut state = ClixState::load(home_dir())?;
        if state.config.active_profiles.contains(&name.to_string()) {
            state.config.active_profiles.retain(|p| p != name);
        } else {
            state.config.active_profiles.push(name.to_string());
        }
        state.save_config()?;
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
        state.storage.write(&path, yaml.as_bytes())?;
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
        state.storage.write(&path, yaml.as_bytes())?;
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
            state.storage.write(&caps_dir.join(&file_name), yaml.as_bytes())?;
        }

        // Patch pack.yaml: add description, author, and capabilities list
        {
            let pack_yaml_path = pack_dir.join("pack.yaml");
            let existing = state.storage.read_to_string(&pack_yaml_path)?;
            let mut val: serde_yaml::Value = serde_yaml::from_str(&existing)
                .map_err(|e| anyhow::anyhow!("pack.yaml parse error: {e}"))?;
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
            let patched = serde_yaml::to_string(&val)?;
            state.storage.write(&pack_yaml_path, patched.as_bytes())?;
        }

        Ok(format!("Pack '{}' created with {} capabilities", name, capability_names.len()))
    }

    fn do_edit_pack_capabilities(&self, pack_name: &str, capability_names: &[String]) -> Result<String> {
        let state = ClixState::load(home_dir())?;
        let pack_dir = state.packs_dir.join(pack_name);
        let pack_yaml_path = pack_dir.join("pack.yaml");
        if !state.storage.exists(&pack_yaml_path) {
            return Err(anyhow::anyhow!("pack.yaml not found for '{}'", pack_name));
        }
        let existing = state.storage.read_to_string(&pack_yaml_path)?;
        let mut val: serde_yaml::Value = serde_yaml::from_str(&existing)
            .map_err(|e| anyhow::anyhow!("pack.yaml parse error: {e}"))?;
        if let serde_yaml::Value::Mapping(ref mut m) = val {
            m.insert(serde_yaml::Value::String("capabilities".into()),
                serde_yaml::Value::Sequence(
                    capability_names.iter().map(|s| serde_yaml::Value::String(s.clone())).collect()
                ));
        }
        let patched = serde_yaml::to_string(&val)?;
        state.storage.write(&pack_yaml_path, patched.as_bytes())?;
        Ok(format!("Pack '{}' updated with {} capabilities", pack_name, capability_names.len()))
    }

    fn do_save_profile_secrets(&self, profile_name: &str, bindings: &[ProfileSecretBinding]) -> Result<String> {
        let state = ClixState::load(home_dir())?;
        let path = state.profiles_dir.join(format!("{}.yaml", profile_name));
        if !state.storage.exists(&path) {
            return Err(anyhow::anyhow!("Profile file not found: {}", path.display()));
        }
        let existing = state.storage.read_to_string(&path)?;
        let mut val: serde_yaml::Value = serde_yaml::from_str(&existing)?;
        if let serde_yaml::Value::Mapping(ref mut m) = val {
            let bindings_yaml = serde_yaml::to_value(bindings)?;
            m.insert(serde_yaml::Value::String("secretBindings".into()), bindings_yaml);
        }
        state.storage.write(&path, serde_yaml::to_string(&val)?.as_bytes())?;
        Ok(format!("Profile '{}' secrets updated ({} bindings)", profile_name, bindings.len()))
    }

    fn do_write_infisical_config(&self, site_url: &str, service_token: &str, project_id: &str, environment: &str) -> Result<clix_core::state::InfisicalConfig> {
        let mut state = ClixState::load(home_dir())?;
        let profile_name = state.config.active_infisical
            .clone()
            .unwrap_or_else(|| "default".to_string());

        #[cfg(target_os = "linux")]
        let token_in_keyring = if !service_token.is_empty() {
            use clix_core::secrets::keyring::{store_service_token, KeyringResult};
            matches!(store_service_token(&profile_name, service_token), KeyringResult::Ok)
        } else {
            false
        };
        #[cfg(not(target_os = "linux"))]
        let token_in_keyring = false;

        let profile = state.config.infisical_profiles
            .entry(profile_name.clone())
            .or_insert_with(|| clix_core::state::InfisicalConfig {
                site_url: "https://app.infisical.com".to_string(),
                client_id: None, client_secret: None, service_token: None,
                default_project_id: None,
                default_environment: "dev".to_string(),
            });
        profile.site_url = site_url.to_string();
        profile.default_environment = environment.to_string();
        if !project_id.is_empty() { profile.default_project_id = Some(project_id.to_string()); }
        if !service_token.is_empty() && !token_in_keyring {
            profile.service_token = Some(service_token.to_string());
        }
        if state.config.active_infisical.is_none() {
            state.config.active_infisical = Some(profile_name.clone());
        }
        // Clone before save_config consumes the mutable borrow
        let saved_token = profile.service_token.clone();
        let saved_client_id = profile.client_id.clone();
        let saved_client_secret = profile.client_secret.clone();
        state.save_config()?;

        let effective_token = if !service_token.is_empty() {
            Some(service_token.to_string())
        } else {
            saved_token
        };
        Ok(clix_core::state::InfisicalConfig {
            site_url: site_url.to_string(),
            client_id: saved_client_id,
            client_secret: saved_client_secret,
            service_token: effective_token,
            default_project_id: if project_id.is_empty() { None } else { Some(project_id.to_string()) },
            default_environment: environment.to_string(),
        })
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

fn load_profiles_from_packs(state: &ClixState) -> Vec<ProfileManifest> {
    let packs_dir = &state.packs_dir;
    if !state.storage.exists(packs_dir) { return vec![]; }
    let Ok(paths) = state.storage.list(packs_dir) else { return vec![]; };
    paths.into_iter()
        .filter(|p| state.storage.is_dir(p))
        .flat_map(|p| load_dir::<ProfileManifest>(&p.join("profiles")).unwrap_or_default())
        .collect()
}

fn load_all_profiles(state: &ClixState) -> Vec<ProfileManifest> {
    use std::collections::HashMap;
    // Global profiles win over pack-shipped ones (user-level override)
    let mut by_name: HashMap<String, ProfileManifest> = HashMap::new();
    for p in load_dir::<ProfileManifest>(&state.profiles_dir).unwrap_or_default() {
        by_name.insert(p.name.clone(), p);
    }
    for p in load_profiles_from_packs(state) {
        by_name.entry(p.name.clone()).or_insert(p);
    }
    let mut profiles: Vec<ProfileManifest> = by_name.into_values().collect();
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    profiles
}

fn load_receipts(db: &std::path::Path) -> (Vec<ReceiptRow>, Vec<String>) {
    use clix_core::receipts::{ReceiptStore, ReceiptStatus};
    let store = match ReceiptStore::open(db) {
        Ok(s) => s,
        Err(_) => return (vec![], vec![]),
    };
    let all = store.list(200, None).unwrap_or_default();
    let pending: Vec<String> = all.iter()
        .filter(|r| matches!(r.status, ReceiptStatus::PendingApproval))
        .map(|r| r.id.to_string())
        .collect();
    let rows = all.into_iter().map(|r| {
        let time = r.created_at.format("%m-%d %H:%M:%S").to_string();
        let outcome = match r.status {
            ReceiptStatus::Succeeded => "✓",
            ReceiptStatus::Failed => "✗",
            ReceiptStatus::Denied => "⊘",
            ReceiptStatus::PendingApproval => "…",
            ReceiptStatus::ApprovalDenied => "✗",
        }.to_string();
        let profile = r.context.get("profile")
            .and_then(|v| v.as_str())
            .unwrap_or("—")
            .to_string();
        let latency = r.execution.as_ref()
            .and_then(|e| e.get("latencyMs"))
            .and_then(|v| v.as_u64())
            .map(|ms| format!("{ms}ms"))
            .unwrap_or_default();
        ReceiptRow { time, capability: r.capability, profile, outcome, latency }
    }).collect();
    (rows, pending)
}
