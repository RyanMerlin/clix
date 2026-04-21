use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use clix_core::manifest::capability::{CredentialSource, RiskLevel, SideEffectClass};
use clix_core::manifest::profile::{ProfileManifest, ProfileSecretBinding, ProfileFolderBinding};
use clix_core::registry::CapabilityRegistry;
use clix_core::state::InfisicalConfig;
use crate::tui::theme;
use crate::tui::widgets::checklist::{Checklist, ChecklistItem};
use crate::tui::widgets::form::FieldInput;
use crate::tui::widgets::secret_picker::{SecretPicker, SecretPickerAction};
use crate::tui::widgets::secrets_tree::{SecretsTree, SecretsTreeAction, TreeMode};

#[derive(Debug, Clone, PartialEq)]
pub enum ProfileWizardStep {
    Identity,      // step 0: name + description
    Capabilities,  // step 1: capability checklist
    Secrets,       // step 2: bind credentials (skipped if no creds needed)
    Confirm,       // step 3: summary
}

/// One row in the Secrets step.
#[derive(Debug, Clone)]
pub struct BindingRow {
    pub inject_as: String,
    pub used_by: Vec<String>,          // capability names that need this
    pub binding: Option<CredentialSource>,
}

impl BindingRow {
    fn display(&self) -> String {
        match &self.binding {
            None => "○ unbound".to_string(),
            Some(CredentialSource::Infisical { secret_ref, .. }) => {
                format!("● Infisical {}/{}", secret_ref.secret_path.trim_end_matches('/'), secret_ref.secret_name)
            }
            Some(CredentialSource::Env { env_var, .. }) => format!("● env ${}", env_var),
            Some(CredentialSource::Literal { .. }) => "● literal ••••".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileWizard {
    pub step: ProfileWizardStep,
    // Step 0
    pub name: FieldInput,
    pub description: FieldInput,
    pub active_field: usize,
    // Step 1
    pub checklist: Checklist,
    // Step 2 — secrets
    pub binding_rows: Vec<BindingRow>,
    pub secrets_cursor: usize,
    /// When Some, the tree picker is open for that row index
    pub tree_picker: Option<(usize, SecretsTree)>,
    /// "e" sub-mode: user is typing an env var name
    pub env_input: Option<(usize, FieldInput)>,
    /// "l" sub-mode: user is typing a literal value
    pub literal_input: Option<(usize, FieldInput)>,
    /// Folder-level bindings added via the F key in SecretPicker
    pub folder_bindings: Vec<ProfileFolderBinding>,
    // Shared error/info
    pub error: Option<String>,
}

pub enum ProfileWizardAction {
    None,
    Cancel,
    Submit {
        name: String,
        description: String,
        capabilities: Vec<String>,
        secret_bindings: Vec<ProfileSecretBinding>,
        folder_bindings: Vec<ProfileFolderBinding>,
    },
    /// Tree picker needs a folder loaded — caller dispatches the work jobs.
    TreeNeedsLoad { project_id: String, environment: String, path: String, folders_job: u64, names_job: u64 },
}

impl ProfileWizard {
    pub fn new(registry: &CapabilityRegistry) -> Self {
        let items: Vec<ChecklistItem> = registry.namespaces().iter().flat_map(|ns| {
            registry.by_namespace(&ns.key).into_iter().map(|cap| {
                let risk_str = match cap.risk {
                    RiskLevel::Low => "low",
                    RiskLevel::Medium => "med",
                    RiskLevel::High => "high",
                    RiskLevel::Critical => "crit",
                };
                let se_str = match cap.side_effect_class {
                    SideEffectClass::None => "—",
                    SideEffectClass::ReadOnly => "read",
                    SideEffectClass::Additive => "add",
                    SideEffectClass::Mutating => "mutate",
                    SideEffectClass::Destructive => "destr",
                };
                let tag = format!("{:<4} {}", risk_str, se_str);
                let tag_color = theme::risk_color(risk_str);
                let pack = cap.name.split('.').next().unwrap_or("?").to_string();
                ChecklistItem::new(
                    &cap.name,
                    &cap.name,
                    cap.description.as_deref().unwrap_or(""),
                    &tag,
                    tag_color,
                    &pack,
                )
            })
        }).collect();

        Self {
            step: ProfileWizardStep::Identity,
            name: FieldInput::default(),
            description: FieldInput::default(),
            active_field: 0,
            checklist: Checklist::new(items),
            binding_rows: vec![],
            secrets_cursor: 0,
            tree_picker: None,
            env_input: None,
            literal_input: None,
            folder_bindings: vec![],
            error: None,
        }
    }

    /// Build binding rows from selected capabilities. Dedupes inject_as names.
    pub fn build_binding_rows(&mut self, registry: &CapabilityRegistry) {
        use std::collections::HashMap;
        let selected_ids = self.checklist.selected_ids();
        let mut by_inject: HashMap<String, BindingRow> = HashMap::new();
        for cap_name in &selected_ids {
            if let Some(cap) = registry.get(cap_name) {
                for cred in &cap.credentials {
                    let inject_as = inject_as_of(cred).to_string();
                    let row = by_inject.entry(inject_as.clone()).or_insert(BindingRow {
                        inject_as: inject_as.clone(),
                        used_by: vec![],
                        // Pre-populate from capability default if it has an InfisicalRef
                        binding: match cred {
                            CredentialSource::Infisical { .. } => Some(cred.clone()),
                            _ => None,
                        },
                    });
                    row.used_by.push(cap_name.clone());
                }
            }
        }
        let mut rows: Vec<BindingRow> = by_inject.into_values().collect();
        rows.sort_by(|a, b| a.inject_as.cmp(&b.inject_as));
        self.binding_rows = rows;
        self.secrets_cursor = 0;
    }

    pub fn is_dirty(&self) -> bool {
        !self.name.value.is_empty()
    }

    pub fn deliver_tree_folders(&mut self, job_id: u64, folders: Vec<String>, error: Option<String>) -> bool {
        if let Some((_, ref mut tree)) = self.tree_picker {
            tree.deliver_folders(job_id, folders, error)
        } else {
            false
        }
    }

    pub fn deliver_tree_names(&mut self, job_id: u64, names: Vec<String>, error: Option<String>) -> bool {
        if let Some((_, ref mut tree)) = self.tree_picker {
            tree.deliver_names(job_id, names, error)
        } else {
            false
        }
    }

    pub fn handle_key(&mut self, code: KeyCode, registry: Option<&CapabilityRegistry>, infisical_cfg: Option<&InfisicalConfig>) -> ProfileWizardAction {
        // Tree picker takes priority over flat picker
        if let Some((row_idx, ref mut tree)) = self.tree_picker {
            let action = tree.handle_key(code, infisical_cfg);
            match action {
                SecretsTreeAction::Cancelled => { self.tree_picker = None; }
                SecretsTreeAction::NeedsLoad(path) => {
                    let project_id = tree.project_id.clone();
                    let environment = tree.environment.clone();
                    let (fid, nid) = tree.request_load(&path);
                    return ProfileWizardAction::TreeNeedsLoad {
                        project_id, environment, path, folders_job: fid, names_job: nid,
                    };
                }
                SecretsTreeAction::Selected(iref) => {
                    let inject_as = self.binding_rows.get(row_idx).map(|r| r.inject_as.clone()).unwrap_or_default();
                    if let Some(row) = self.binding_rows.get_mut(row_idx) {
                        row.binding = Some(CredentialSource::Infisical { secret_ref: iref, inject_as });
                    }
                    self.tree_picker = None;
                }
                SecretsTreeAction::SelectedMany(refs) => {
                    // Bind all selected secrets as folder-style snapshot bindings
                    for iref in refs {
                        self.folder_bindings.push(ProfileFolderBinding {
                            project_id: iref.project_id.clone().unwrap_or_default(),
                            environment: iref.environment.clone(),
                            secret_path: iref.secret_path.clone(),
                            inject_prefix: None,
                            synced_at: chrono::Utc::now(),
                            snapshot: vec![iref.secret_name],
                            infisical_profile: None,
                        });
                    }
                    self.tree_picker = None;
                }
                SecretsTreeAction::SelectedFolder { project_id, environment, secret_path, inject_prefix, snapshot } => {
                    self.folder_bindings.push(ProfileFolderBinding {
                        project_id,
                        environment,
                        secret_path,
                        inject_prefix,
                        synced_at: chrono::Utc::now(),
                        snapshot,
                        infisical_profile: None,
                    });
                    self.tree_picker = None;
                }
                SecretsTreeAction::None => {}
            }
            return ProfileWizardAction::None;
        }

        // Env-var input sub-mode
        if let Some((row_idx, ref mut fi)) = self.env_input {
            match code {
                KeyCode::Esc => { self.env_input = None; }
                KeyCode::Enter => {
                    let val = fi.value.trim().to_string();
                    if !val.is_empty() {
                        if let Some(row) = self.binding_rows.get_mut(row_idx) {
                            row.binding = Some(CredentialSource::Env {
                                env_var: val.clone(),
                                inject_as: row.inject_as.clone(),
                            });
                        }
                    }
                    self.env_input = None;
                }
                _ => { fi.handle_key(code); }
            }
            return ProfileWizardAction::None;
        }

        // Literal input sub-mode
        if let Some((row_idx, ref mut fi)) = self.literal_input {
            match code {
                KeyCode::Esc => { self.literal_input = None; }
                KeyCode::Enter => {
                    let val = fi.value.clone();
                    if let Some(row) = self.binding_rows.get_mut(row_idx) {
                        row.binding = Some(CredentialSource::Literal {
                            value: val,
                            inject_as: row.inject_as.clone(),
                        });
                    }
                    self.literal_input = None;
                }
                _ => { fi.handle_key(code); }
            }
            return ProfileWizardAction::None;
        }

        match &self.step {
            ProfileWizardStep::Identity => self.handle_identity(code),
            ProfileWizardStep::Capabilities => self.handle_capabilities(code, registry),
            ProfileWizardStep::Secrets => self.handle_secrets(code, infisical_cfg),
            ProfileWizardStep::Confirm => self.handle_confirm(code),
        }
    }

    fn handle_identity(&mut self, code: KeyCode) -> ProfileWizardAction {
        match code {
            KeyCode::Esc => return ProfileWizardAction::Cancel,
            KeyCode::Tab => self.active_field = (self.active_field + 1) % 2,
            KeyCode::BackTab => self.active_field = if self.active_field == 0 { 1 } else { 0 },
            KeyCode::Enter => {
                let name = self.name.value.trim().to_string();
                if name.is_empty() {
                    self.error = Some("Name is required".into());
                    return ProfileWizardAction::None;
                }
                if name.contains('/') || name.contains('\\') || name.contains("..") {
                    self.error = Some("Name must not contain path separators".into());
                    return ProfileWizardAction::None;
                }
                self.error = None;
                self.step = ProfileWizardStep::Capabilities;
            }
            _ => {
                self.error = None;
                if self.active_field == 0 { self.name.handle_key(code); } else { self.description.handle_key(code); }
            }
        }
        ProfileWizardAction::None
    }

    fn handle_capabilities(&mut self, code: KeyCode, registry: Option<&CapabilityRegistry>) -> ProfileWizardAction {
        match code {
            KeyCode::Esc => self.step = ProfileWizardStep::Identity,
            KeyCode::Enter => {
                // Build binding rows from selected caps, then advance
                if let Some(reg) = registry { self.build_binding_rows(reg); }
                if self.binding_rows.is_empty() {
                    self.step = ProfileWizardStep::Confirm;
                } else {
                    self.step = ProfileWizardStep::Secrets;
                }
            }
            _ => { self.checklist.handle_key(code); }
        }
        ProfileWizardAction::None
    }

    fn handle_secrets(&mut self, code: KeyCode, infisical_cfg: Option<&InfisicalConfig>) -> ProfileWizardAction {
        match code {
            KeyCode::Esc => self.step = ProfileWizardStep::Capabilities,
            KeyCode::Tab | KeyCode::Enter => self.step = ProfileWizardStep::Confirm,
            KeyCode::Up => { if self.secrets_cursor > 0 { self.secrets_cursor -= 1; } }
            KeyCode::Down => {
                if self.secrets_cursor + 1 < self.binding_rows.len() { self.secrets_cursor += 1; }
            }
            KeyCode::Char('p') | KeyCode::Char('i') => {
                // Open Infisical tree picker in Bind mode
                if let Some(row) = self.binding_rows.get(self.secrets_cursor) {
                    let (project_id, environment) = derive_project_env(row);
                    if let Some(cfg) = infisical_cfg {
                        let proj = if project_id.is_empty() {
                            cfg.default_project_id.clone().unwrap_or_default()
                        } else {
                            project_id.clone()
                        };
                        let env = if environment.is_empty() {
                            cfg.default_environment.clone()
                        } else {
                            environment.clone()
                        };
                        let mut tree = SecretsTree::new(&proj, &env, TreeMode::Bind);
                        let fid = crate::tui::work::next_job_id();
                        let nid = crate::tui::work::next_job_id();
                        tree.initial_load_ids(fid, nid);
                        let row_idx = self.secrets_cursor;
                        self.tree_picker = Some((row_idx, tree));
                        return ProfileWizardAction::TreeNeedsLoad {
                            project_id: proj, environment: env,
                            path: "/".to_string(), folders_job: fid, names_job: nid,
                        };
                    } else {
                        self.error = Some("Infisical not configured — press e to configure it first".into());
                    }
                }
            }
            KeyCode::Char('e') => {
                // Bind from env var
                self.env_input = Some((self.secrets_cursor, FieldInput::default()));
            }
            KeyCode::Char('l') => {
                // Bind literal value
                self.literal_input = Some((self.secrets_cursor, FieldInput::default()));
            }
            KeyCode::Char('d') => {
                // Clear binding
                if let Some(row) = self.binding_rows.get_mut(self.secrets_cursor) {
                    row.binding = None;
                }
            }
            _ => {}
        }
        ProfileWizardAction::None
    }

    fn handle_confirm(&mut self, code: KeyCode) -> ProfileWizardAction {
        match code {
            KeyCode::Esc => {
                if self.binding_rows.is_empty() {
                    self.step = ProfileWizardStep::Capabilities;
                } else {
                    self.step = ProfileWizardStep::Secrets;
                }
            }
            KeyCode::Enter | KeyCode::Char('w') => {
                let secret_bindings: Vec<ProfileSecretBinding> = self.binding_rows.iter()
                    .filter_map(|r| r.binding.as_ref().map(|src| ProfileSecretBinding {
                        inject_as: r.inject_as.clone(),
                        source: src.clone(),
                    }))
                    .collect();
                return ProfileWizardAction::Submit {
                    name: self.name.value.trim().to_string(),
                    description: self.description.value.trim().to_string(),
                    capabilities: self.checklist.selected_ids(),
                    secret_bindings,
                    folder_bindings: self.folder_bindings.clone(),
                };
            }
            _ => {}
        }
        ProfileWizardAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let width = area.width.saturating_sub(4).max(50);
        let height = area.height.saturating_sub(2).max(12);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let dialog = Rect::new(x, y, width, height);

        f.render_widget(Clear, dialog);

        let total_steps = if self.binding_rows.is_empty() { 3 } else { 4 };
        let step_n = match self.step {
            ProfileWizardStep::Identity => 0,
            ProfileWizardStep::Capabilities => 1,
            ProfileWizardStep::Secrets => 2,
            ProfileWizardStep::Confirm => if self.binding_rows.is_empty() { 2 } else { 3 },
        };
        let title = format!(" New Profile · {}", step_dots(step_n, total_steps));
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title, theme::accent_bold()))
            .border_style(theme::border_focused());
        let inner = block.inner(dialog);
        f.render_widget(block, dialog);

        match &self.step {
            ProfileWizardStep::Identity => self.render_identity(f, inner),
            ProfileWizardStep::Capabilities => self.render_capabilities(f, inner),
            ProfileWizardStep::Secrets => self.render_secrets(f, inner),
            ProfileWizardStep::Confirm => self.render_confirm(f, inner),
        }

        // Tree picker sub-overlay
        if let Some((_, ref tree)) = self.tree_picker {
            tree.render(f, area);
        }
        // Env input sub-overlay
        if let Some((row_idx, ref fi)) = self.env_input {
            render_inline_input(f, fi, &format!("Env var for {}", self.binding_rows.get(row_idx).map(|r| r.inject_as.as_str()).unwrap_or("?")), area);
        }
        // Literal input sub-overlay
        if let Some((row_idx, ref fi)) = self.literal_input {
            render_inline_input(f, fi, &format!("Literal value for {}", self.binding_rows.get(row_idx).map(|r| r.inject_as.as_str()).unwrap_or("?")), area);
        }
    }

    fn render_identity(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .margin(1)
            .split(area);

        render_text_field(f, &self.name, "Name *", self.active_field == 0, chunks[0]);
        render_text_field(f, &self.description, "Description", self.active_field == 1, chunks[1]);

        if let Some(err) = &self.error {
            f.render_widget(Paragraph::new(Span::styled(err.as_str(), theme::danger())), chunks[2]);
        }
        f.render_widget(
            Paragraph::new("tab: next field   enter: continue   esc: cancel").style(theme::muted()),
            chunks[3],
        );
    }

    fn render_capabilities(&self, f: &mut Frame, area: Rect) {
        if self.checklist.items.is_empty() {
            f.render_widget(Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled("  No capabilities loaded.", theme::muted())),
                Line::from(""),
                Line::from(Span::styled("  Install a pack first, then create your profile.", theme::dim())),
                Line::from(""),
                Line::from("  enter: continue   esc: back"),
            ]), area);
            return;
        }

        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let title = format!("Capabilities ({} selected)", self.checklist.selected_count());
        self.checklist.render(f, chunks[0], &title, true);
        f.render_widget(
            Paragraph::new("enter: continue   esc: back").style(theme::muted()),
            chunks[1],
        );
    }

    fn render_secrets(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(2)])
            .margin(1)
            .split(area);

        // Binding rows list
        let items: Vec<ListItem> = self.binding_rows.iter().enumerate().map(|(i, row)| {
            let is_cursor = i == self.secrets_cursor;
            let name_style = if is_cursor { theme::selected() } else { theme::normal() };
            let used = row.used_by.iter().take(2).cloned().collect::<Vec<_>>().join(", ");
            let binding_display = row.display();
            let binding_style = if row.binding.is_some() { theme::ok() } else { theme::muted() };

            ListItem::new(Line::from(vec![
                Span::styled(format!("  {:<24}", row.inject_as), name_style),
                Span::styled(format!("{:<30}  ", used), theme::dim()),
                Span::styled(binding_display, binding_style),
            ]))
        }).collect();

        let header = if self.binding_rows.is_empty() {
            "No credentials required by selected capabilities"
        } else {
            "Credential bindings"
        };

        let list = List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .title(header)
                .border_style(theme::border_normal()))
            .highlight_style(theme::selected());
        let mut state = ListState::default();
        state.select(Some(self.secrets_cursor));
        f.render_stateful_widget(list, chunks[0], &mut state);

        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "p: Infisical picker   e: env var   l: literal   d: clear   tab/enter: next   esc: back",
                    theme::muted(),
                )),
            ]),
            chunks[1],
        );
    }

    fn render_confirm(&self, f: &mut Frame, area: Rect) {
        let cap_ids = self.checklist.selected_ids();
        let bound_count = self.binding_rows.iter().filter(|r| r.binding.is_some()).count();
        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Name        ", theme::muted()),
                Span::styled(self.name.value.trim(), theme::normal()),
            ]),
        ];
        if !self.description.value.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  Description ", theme::muted()),
                Span::styled(self.description.value.trim(), theme::dim()),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Capabilities", theme::muted()),
            Span::styled(format!("  {} selected", cap_ids.len()), theme::accent()),
        ]));
        for id in cap_ids.iter().take(6) {
            lines.push(Line::from(Span::styled(format!("    • {}", id), theme::dim())));
        }
        if cap_ids.len() > 6 {
            lines.push(Line::from(Span::styled(format!("    … and {} more", cap_ids.len() - 6), theme::muted())));
        }

        if !self.binding_rows.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  Secrets     ", theme::muted()),
                Span::styled(
                    format!("  {}/{} bound", bound_count, self.binding_rows.len()),
                    if bound_count == self.binding_rows.len() { theme::ok() } else { theme::warn() },
                ),
            ]));
            for row in self.binding_rows.iter().take(4) {
                let src = row.display();
                lines.push(Line::from(vec![
                    Span::styled(format!("    {:<22}", row.inject_as), theme::muted()),
                    Span::styled(src, theme::dim()),
                ]));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  enter", theme::accent()),
            Span::raw(" write profile   "),
            Span::styled("esc", theme::muted()),
            Span::raw(" back"),
        ]));

        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }
}

// ─── helpers ────────────────────────────────────────────────────────────────

fn step_dots(current: usize, total: usize) -> String {
    (0..total).map(|i| if i == current { "●" } else { "○" }).collect::<Vec<_>>().join("")
}

fn inject_as_of(c: &CredentialSource) -> &str {
    match c {
        CredentialSource::Literal { inject_as, .. } => inject_as,
        CredentialSource::Env { inject_as, .. } => inject_as,
        CredentialSource::Infisical { inject_as, .. } => inject_as,
    }
}

/// Derive project_id and environment from an existing binding (if it's Infisical) or use empty strings.
fn derive_project_env(row: &BindingRow) -> (String, String) {
    if let Some(CredentialSource::Infisical { secret_ref, .. }) = &row.binding {
        return (
            secret_ref.project_id.clone().unwrap_or_default(),
            secret_ref.environment.clone(),
        );
    }
    (String::new(), String::new())
}

fn render_inline_input(f: &mut Frame, fi: &FieldInput, label: &str, area: Rect) {
    let width = 52u16.min(area.width);
    let height = 5u16;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let dialog = Rect::new(x, y, width, height);
    f.render_widget(Clear, dialog);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(format!(" {} ", label), theme::accent_bold()))
        .border_style(theme::border_focused());
    let inner = block.inner(dialog);
    f.render_widget(block, dialog);
    let (before_disp, after_disp) = fi.split_display_at_cursor();
    let cursor_ch: String = after_disp.chars().next().map(|c| c.to_string()).unwrap_or_else(|| " ".to_string());
    let after_cursor: String = if after_disp.is_empty() { String::new() } else { after_disp.chars().skip(1).collect() };
    let line = Line::from(vec![
        Span::raw(before_disp),
        Span::styled(cursor_ch, Style::default().bg(theme::ACCENT_BRIGHT).fg(Color::Black)),
        Span::raw(after_cursor),
    ]);
    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
        .split(inner);
    f.render_widget(Paragraph::new(line), chunks[0]);
    f.render_widget(Paragraph::new("enter: confirm   esc: cancel").style(theme::muted()), chunks[1]);
}

pub fn render_text_field(f: &mut Frame, field: &FieldInput, label: &str, focused: bool, area: Rect) {
    let border_style = if focused { theme::border_focused() } else { theme::border_normal() };
    let (before_disp, after_disp) = field.split_display_at_cursor();
    // For cursor char: take the first char of after_disp (1 or 3 bytes for •)
    let cursor_char: String = after_disp.chars().next()
        .map(|c| c.to_string())
        .unwrap_or_else(|| " ".to_string());
    let after_cursor: String = if after_disp.is_empty() {
        String::new()
    } else {
        after_disp.chars().skip(1).collect()
    };

    let content = if focused {
        Line::from(vec![
            Span::raw(before_disp),
            Span::styled(cursor_char, Style::default().bg(theme::ACCENT_BRIGHT).fg(Color::Black)),
            Span::raw(after_cursor),
        ])
    } else {
        Line::from(Span::styled(before_disp + &after_disp, theme::dim()))
    };

    let para = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title(label).border_style(border_style));
    f.render_widget(para, area);
}

// ─── standalone Secrets editor (for `s` key on Profiles screen) ─────────────

#[derive(Debug, Clone)]
pub struct SecretsEditState {
    pub profile_name: String,
    pub binding_rows: Vec<BindingRow>,
    pub cursor: usize,
    pub tree_picker: Option<(usize, SecretsTree)>,
    pub env_input: Option<(usize, FieldInput)>,
    pub literal_input: Option<(usize, FieldInput)>,
}

pub enum SecretsEditAction {
    None,
    Cancel,
    Save(Vec<ProfileSecretBinding>),
    TreeNeedsLoad { project_id: String, environment: String, path: String, folders_job: u64, names_job: u64 },
}

impl SecretsEditState {
    pub fn new(profile: &ProfileManifest, registry: &CapabilityRegistry) -> Self {
        use std::collections::HashMap;
        // Gather all credentials needed by capabilities in this profile
        let mut by_inject: HashMap<String, BindingRow> = HashMap::new();
        // Pre-populate existing bindings from the profile
        for binding in &profile.secret_bindings {
            by_inject.insert(binding.inject_as.clone(), BindingRow {
                inject_as: binding.inject_as.clone(),
                used_by: vec![],
                binding: Some(binding.source.clone()),
            });
        }
        // Scan capabilities to populate used_by and add missing rows
        for cap_name in &profile.capabilities {
            if let Some(cap) = registry.get(cap_name) {
                for cred in &cap.credentials {
                    let inject_as = inject_as_of(cred).to_string();
                    let row = by_inject.entry(inject_as.clone()).or_insert(BindingRow {
                        inject_as: inject_as.clone(),
                        used_by: vec![],
                        binding: match cred {
                            CredentialSource::Infisical { .. } => Some(cred.clone()),
                            _ => None,
                        },
                    });
                    row.used_by.push(cap_name.clone());
                }
            }
        }
        let mut rows: Vec<BindingRow> = by_inject.into_values().collect();
        rows.sort_by(|a, b| a.inject_as.cmp(&b.inject_as));
        Self {
            profile_name: profile.name.clone(),
            binding_rows: rows,
            cursor: 0,
            tree_picker: None,
            env_input: None,
            literal_input: None,
        }
    }

    pub fn deliver_tree_folders(&mut self, job_id: u64, folders: Vec<String>, error: Option<String>) {
        if let Some((_, ref mut tree)) = self.tree_picker {
            tree.deliver_folders(job_id, folders, error);
        }
    }

    pub fn deliver_tree_names(&mut self, job_id: u64, names: Vec<String>, error: Option<String>) {
        if let Some((_, ref mut tree)) = self.tree_picker {
            tree.deliver_names(job_id, names, error);
        }
    }

    pub fn handle_key(&mut self, code: KeyCode, infisical_cfg: Option<&InfisicalConfig>) -> SecretsEditAction {
        // Tree picker sub-overlay (takes priority)
        if let Some((row_idx, ref mut tree)) = self.tree_picker {
            let action = tree.handle_key(code, infisical_cfg);
            match action {
                SecretsTreeAction::Cancelled => { self.tree_picker = None; }
                SecretsTreeAction::NeedsLoad(path) => {
                    let project_id = tree.project_id.clone();
                    let environment = tree.environment.clone();
                    let (fid, nid) = tree.request_load(&path);
                    return SecretsEditAction::TreeNeedsLoad {
                        project_id, environment, path, folders_job: fid, names_job: nid,
                    };
                }
                SecretsTreeAction::Selected(iref) => {
                    let inject_as = self.binding_rows.get(row_idx).map(|r| r.inject_as.clone()).unwrap_or_default();
                    if let Some(row) = self.binding_rows.get_mut(row_idx) {
                        row.binding = Some(CredentialSource::Infisical { secret_ref: iref, inject_as });
                    }
                    self.tree_picker = None;
                }
                SecretsTreeAction::SelectedMany(refs) => {
                    if let Some(iref) = refs.into_iter().next() {
                        let inject_as = self.binding_rows.get(row_idx).map(|r| r.inject_as.clone()).unwrap_or_default();
                        if let Some(row) = self.binding_rows.get_mut(row_idx) {
                            row.binding = Some(CredentialSource::Infisical { secret_ref: iref, inject_as });
                        }
                    }
                    self.tree_picker = None;
                }
                SecretsTreeAction::SelectedFolder { .. } => {
                    self.tree_picker = None;
                }
                SecretsTreeAction::None => {}
            }
            return SecretsEditAction::None;
        }
        // Env input
        if let Some((row_idx, ref mut fi)) = self.env_input {
            match code {
                KeyCode::Esc => { self.env_input = None; }
                KeyCode::Enter => {
                    let val = fi.value.trim().to_string();
                    if !val.is_empty() {
                        if let Some(row) = self.binding_rows.get_mut(row_idx) {
                            row.binding = Some(CredentialSource::Env { env_var: val, inject_as: row.inject_as.clone() });
                        }
                    }
                    self.env_input = None;
                }
                _ => { fi.handle_key(code); }
            }
            return SecretsEditAction::None;
        }
        // Literal input
        if let Some((row_idx, ref mut fi)) = self.literal_input {
            match code {
                KeyCode::Esc => { self.literal_input = None; }
                KeyCode::Enter => {
                    let val = fi.value.clone();
                    if let Some(row) = self.binding_rows.get_mut(row_idx) {
                        row.binding = Some(CredentialSource::Literal { value: val, inject_as: row.inject_as.clone() });
                    }
                    self.literal_input = None;
                }
                _ => { fi.handle_key(code); }
            }
            return SecretsEditAction::None;
        }

        match code {
            KeyCode::Esc => return SecretsEditAction::Cancel,
            KeyCode::Enter => {
                let bindings = self.binding_rows.iter()
                    .filter_map(|r| r.binding.as_ref().map(|src| ProfileSecretBinding {
                        inject_as: r.inject_as.clone(),
                        source: src.clone(),
                    }))
                    .collect();
                return SecretsEditAction::Save(bindings);
            }
            KeyCode::Up => { if self.cursor > 0 { self.cursor -= 1; } }
            KeyCode::Down => {
                if self.cursor + 1 < self.binding_rows.len() { self.cursor += 1; }
            }
            KeyCode::Char('p') | KeyCode::Char('i') => {
                if let Some(row) = self.binding_rows.get(self.cursor) {
                    let (project_id, environment) = derive_project_env(row);
                    if let Some(cfg) = infisical_cfg {
                        let proj = if project_id.is_empty() {
                            cfg.default_project_id.clone().unwrap_or_default()
                        } else {
                            project_id.clone()
                        };
                        let env = if environment.is_empty() {
                            cfg.default_environment.clone()
                        } else {
                            environment.clone()
                        };
                        let mut tree = SecretsTree::new(&proj, &env, TreeMode::Bind);
                        let fid = crate::tui::work::next_job_id();
                        let nid = crate::tui::work::next_job_id();
                        tree.initial_load_ids(fid, nid);
                        let row_idx = self.cursor;
                        self.tree_picker = Some((row_idx, tree));
                        return SecretsEditAction::TreeNeedsLoad {
                            project_id: proj, environment: env,
                            path: "/".to_string(), folders_job: fid, names_job: nid,
                        };
                    } else {
                        let mut t = SecretsTree::new(&project_id, &environment, TreeMode::Bind);
                        t.error = Some("Infisical not configured in config.yaml".to_string());
                        self.tree_picker = Some((self.cursor, t));
                    }
                }
            }
            KeyCode::Char('e') => { self.env_input = Some((self.cursor, FieldInput::default())); }
            KeyCode::Char('l') => { self.literal_input = Some((self.cursor, FieldInput::default())); }
            KeyCode::Char('d') => {
                if let Some(row) = self.binding_rows.get_mut(self.cursor) { row.binding = None; }
            }
            _ => {}
        }
        SecretsEditAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let width = area.width.saturating_sub(4).max(60);
        let height = area.height.saturating_sub(2).max(14);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let dialog = Rect::new(x, y, width, height);
        f.render_widget(Clear, dialog);

        let title = format!(" Secrets · {} ", self.profile_name);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title, theme::accent_bold()))
            .border_style(theme::border_focused());
        let inner = block.inner(dialog);
        f.render_widget(block, dialog);

        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)])
            .split(inner);

        // Binding rows
        let items: Vec<ListItem> = self.binding_rows.iter().enumerate().map(|(i, row)| {
            let is_cursor = i == self.cursor;
            let name_style = if is_cursor { theme::selected() } else { theme::normal() };
            let binding_style = if row.binding.is_some() { theme::ok() } else { theme::muted() };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {:<24}", row.inject_as), name_style),
                Span::styled(format!("{:<28}  ", row.used_by.iter().take(2).cloned().collect::<Vec<_>>().join(", ")), theme::dim()),
                Span::styled(row.display(), binding_style),
            ]))
        }).collect();

        let header = if self.binding_rows.is_empty() {
            "No credentials required by this profile's capabilities"
        } else {
            "Credential bindings"
        };
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(header).border_style(theme::border_normal()))
            .highlight_style(theme::selected());
        let mut state = ListState::default();
        state.select(Some(self.cursor));
        f.render_stateful_widget(list, chunks[0], &mut state);

        f.render_widget(
            Paragraph::new("↑↓:move  p:Infisical  e:env  l:literal  d:clear  enter:save  esc:cancel").style(theme::muted()),
            chunks[1],
        );

        if let Some((_, ref tree)) = self.tree_picker {
            tree.render(f, area);
        }
        if let Some((row_idx, ref fi)) = self.env_input {
            render_inline_input(f, fi, &format!("Env var for {}", self.binding_rows.get(row_idx).map(|r| r.inject_as.as_str()).unwrap_or("?")), area);
        }
        if let Some((row_idx, ref fi)) = self.literal_input {
            render_inline_input(f, fi, &format!("Literal value for {}", self.binding_rows.get(row_idx).map(|r| r.inject_as.as_str()).unwrap_or("?")), area);
        }
    }
}
