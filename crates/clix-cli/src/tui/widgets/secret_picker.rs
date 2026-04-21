use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use clix_core::manifest::capability::InfisicalRef;
use clix_core::state::InfisicalConfig;
use crate::tui::theme;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    Folder,
    Secret,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: EntryKind,
    pub name: String,
}

/// A live-loading Infisical folder/secret browser.
/// Returns an `InfisicalRef` when the user selects a secret key.
#[derive(Debug, Clone)]
pub struct SecretPicker {
    pub project_id: String,
    pub environment: String,
    /// Stack of path segments; current path = "/".join(stack)
    pub path_stack: Vec<String>,
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub error: Option<String>,
    /// Set after a successful load attempt (even if empty)
    pub loaded: bool,
    /// True while async folder/secret load is in flight
    pub loading: bool,
    pub pending_folders_job: Option<crate::tui::work::JobId>,
    pub pending_names_job: Option<crate::tui::work::JobId>,
    /// Multi-selected entry indices
    pub selected: std::collections::HashSet<usize>,
}

impl SecretPicker {
    pub fn new(project_id: &str, environment: &str) -> Self {
        Self {
            project_id: project_id.to_string(),
            environment: environment.to_string(),
            path_stack: vec![],
            entries: vec![],
            cursor: 0,
            error: None,
            loaded: false,
            loading: false,
            pending_folders_job: None,
            pending_names_job: None,
            selected: std::collections::HashSet::new(),
        }
    }

    pub fn current_path(&self) -> String {
        if self.path_stack.is_empty() {
            "/".to_string()
        } else {
            format!("/{}/", self.path_stack.join("/"))
        }
    }

    /// Mark this picker as needing a load. Call from app.rs before showing the picker,
    /// then dispatch LoadSecretFolders + LoadSecretNames work jobs and store job ids via
    /// set_pending_jobs(). Results arrive via deliver_folders() / deliver_names() in tick().
    pub fn mark_loading(&mut self) {
        self.error = None;
        self.loaded = false;
        self.loading = true;
        self.entries.clear();
        self.pending_folders_job = None;
        self.pending_names_job = None;
    }

    /// Store the job IDs that were dispatched for this picker's current path.
    pub fn set_pending_jobs(&mut self, folders_job: crate::tui::work::JobId, names_job: crate::tui::work::JobId) {
        self.pending_folders_job = Some(folders_job);
        self.pending_names_job = Some(names_job);
    }

    /// Called by App::tick() when SecretFoldersLoaded arrives.
    pub fn deliver_folders(&mut self, job_id: crate::tui::work::JobId, folders: Vec<String>, error: Option<String>) {
        if self.pending_folders_job != Some(job_id) { return; }
        self.pending_folders_job = None;
        if let Some(e) = error {
            self.error = Some(format!("Folder list failed: {e}"));
            self.loading = self.pending_names_job.is_some();
            if !self.loading { self.loaded = true; }
            return;
        }
        let folder_entries: Vec<Entry> = folders.into_iter()
            .map(|name| Entry { kind: EntryKind::Folder, name })
            .collect();
        self.entries.retain(|e| e.kind != EntryKind::Folder);
        let mut combined = folder_entries;
        combined.extend(self.entries.drain(..));
        self.entries = combined;
        self.loading = self.pending_names_job.is_some();
        if !self.loading { self.loaded = true; self.cursor = 0; }
    }

    /// Called by App::tick() when SecretNamesLoaded arrives.
    pub fn deliver_names(&mut self, job_id: crate::tui::work::JobId, names: Vec<String>, error: Option<String>) {
        if self.pending_names_job != Some(job_id) { return; }
        self.pending_names_job = None;
        if let Some(e) = error {
            self.error = Some(format!("Secret list failed: {e}"));
            self.loading = self.pending_folders_job.is_some();
            if !self.loading { self.loaded = true; }
            return;
        }
        self.entries.retain(|e| e.kind != EntryKind::Secret);
        for name in names {
            self.entries.push(Entry { kind: EntryKind::Secret, name });
        }
        self.loading = self.pending_folders_job.is_some();
        if !self.loading { self.loaded = true; self.cursor = 0; }
    }

    /// Returns a bound `InfisicalRef` if the cursor is on a secret and Enter was pressed.
    pub fn handle_key(&mut self, code: KeyCode, cfg: Option<&InfisicalConfig>) -> SecretPickerAction {
        match code {
            KeyCode::Esc => return SecretPickerAction::Cancelled,
            KeyCode::Up => {
                if self.cursor > 0 { self.cursor -= 1; }
            }
            KeyCode::Down => {
                if self.cursor + 1 < self.entries.len() { self.cursor += 1; }
            }
            KeyCode::PageUp => {
                self.cursor = self.cursor.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.cursor = (self.cursor + 10).min(self.entries.len().saturating_sub(1));
            }
            KeyCode::Backspace => {
                if !self.path_stack.is_empty() {
                    self.path_stack.pop();
                    self.selected.clear();
                    self.mark_loading();
                    return SecretPickerAction::NeedsLoad;
                }
                let _ = cfg; // not needed for sync load anymore
            }
            // space: toggle multi-select for current entry
            KeyCode::Char(' ') => {
                let entry_idx = self.cursor;
                if entry_idx < self.entries.len() {
                    if self.selected.contains(&entry_idx) {
                        self.selected.remove(&entry_idx);
                    } else {
                        self.selected.insert(entry_idx);
                    }
                }
            }
            // a: select all secrets at current level
            KeyCode::Char('a') => {
                for (i, entry) in self.entries.iter().enumerate() {
                    if entry.kind == EntryKind::Secret {
                        self.selected.insert(i);
                    }
                }
            }
            // F (shift-f): folder-level bind
            KeyCode::Char('F') => {
                if let Some(entry) = self.entries.get(self.cursor) {
                    if entry.kind == EntryKind::Folder {
                        let folder_name = entry.name.clone();
                        let folder_path = if self.path_stack.is_empty() {
                            format!("/{}/", folder_name)
                        } else {
                            format!("/{}/{}/", self.path_stack.join("/"), folder_name)
                        };
                        if let Some(cfg) = cfg {
                            let snapshot = clix_core::secrets::list_infisical_secrets(
                                cfg, &self.project_id, &self.environment, &folder_path
                            ).unwrap_or_default();
                            return SecretPickerAction::SelectedFolder {
                                project_id: self.project_id.clone(),
                                environment: self.environment.clone(),
                                secret_path: folder_path,
                                inject_prefix: None,
                                snapshot,
                            };
                        } else {
                            self.error = Some("Infisical not configured".to_string());
                        }
                    }
                }
            }
            KeyCode::Enter => {
                // Multi-select confirm
                if !self.selected.is_empty() {
                    let refs: Vec<InfisicalRef> = self.selected.iter()
                        .filter_map(|&idx| self.entries.get(idx))
                        .filter(|e| e.kind == EntryKind::Secret)
                        .map(|e| {
                            let path = if self.path_stack.is_empty() {
                                "/".to_string()
                            } else {
                                format!("/{}", self.path_stack.join("/"))
                            };
                            InfisicalRef {
                                secret_name: e.name.clone(),
                                project_id: Some(self.project_id.clone()),
                                environment: self.environment.clone(),
                                secret_path: path,
                                infisical_profile: None,
                            }
                        })
                        .collect();
                    if !refs.is_empty() {
                        return SecretPickerAction::SelectedMany(refs);
                    }
                }

                if let Some(entry) = self.entries.get(self.cursor) {
                    match entry.kind {
                        EntryKind::Folder => {
                            self.path_stack.push(entry.name.clone());
                            self.selected.clear();
                            self.mark_loading();
                            return SecretPickerAction::NeedsLoad;
                        }
                        EntryKind::Secret => {
                            let path = if self.path_stack.is_empty() {
                                "/".to_string()
                            } else {
                                format!("/{}", self.path_stack.join("/"))
                            };
                            return SecretPickerAction::Selected(InfisicalRef {
                                secret_name: entry.name.clone(),
                                project_id: Some(self.project_id.clone()),
                                environment: self.environment.clone(),
                                secret_path: path,
                                infisical_profile: None,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
        SecretPickerAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let width = area.width.saturating_sub(4).max(50).min(80);
        let height = area.height.saturating_sub(4).max(12).min(24);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let dialog = Rect::new(x, y, width, height);

        f.render_widget(Clear, dialog);

        let title = format!(
            " Infisical · {} · {} ",
            self.project_id, self.environment
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title, theme::accent_bold()))
            .border_style(theme::border_focused());
        let inner = block.inner(dialog);
        f.render_widget(block, dialog);

        let chunks = Layout::vertical([
                Constraint::Length(1),  // path breadcrumb
                Constraint::Min(0),     // entry list
                Constraint::Length(1),  // hint
            ])
            .split(inner);

        // Breadcrumb
        f.render_widget(
            Paragraph::new(Span::styled(
                format!(" Path: {}", self.current_path()),
                theme::dim(),
            )),
            chunks[0],
        );

        // Entry list
        if let Some(err) = &self.error {
            f.render_widget(
                Paragraph::new(Span::styled(err.as_str(), theme::danger())),
                chunks[1],
            );
        } else if self.loading || !self.loaded {
            let frame = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| (d.subsec_millis() / 250) as usize % 4)
                .unwrap_or(0);
            let spinner = ["⠋", "⠙", "⠹", "⠸"][frame];
            f.render_widget(
                Paragraph::new(Span::styled(format!("  {} Loading…", spinner), theme::muted())),
                chunks[1],
            );
        } else {
            // ".." go-up row if not at root
            let show_up = !self.path_stack.is_empty();
            let entry_offset = if show_up { 1 } else { 0 };
            let total_rows = self.entries.len() + entry_offset;

            let items: Vec<ListItem> = (0..total_rows).map(|row| {
                if show_up && row == 0 {
                    let style = if self.cursor == 0 { theme::selected() } else { theme::muted() };
                    ListItem::new(Line::from(vec![
                        Span::styled("  ", style),
                        Span::styled("↑  ..", style),
                    ]))
                } else {
                    let entry = &self.entries[row - entry_offset];
                    let is_cursor = row == self.cursor;
                    let entry_idx = row - entry_offset;
                    let is_selected = self.selected.contains(&entry_idx);
                    let (icon, style) = match entry.kind {
                        EntryKind::Folder => (
                            "▸  ",
                            if is_cursor { theme::selected() } else { theme::accent() },
                        ),
                        EntryKind::Secret => (
                            if is_selected { "[✓]" } else { "[ ]" },
                            if is_cursor { theme::selected() } else if is_selected { theme::ok() } else { theme::normal() },
                        ),
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  {} {}", icon, entry.name), style),
                    ]))
                }
            }).collect();

            if items.is_empty() {
                f.render_widget(
                    Paragraph::new(Span::styled("  (empty)", theme::inactive())),
                    chunks[1],
                );
            } else {
                let list = List::new(items)
                    .highlight_style(theme::selected());
                let mut state = ListState::default();
                state.select(Some(self.cursor));
                f.render_stateful_widget(list, chunks[1], &mut state);
            }
        }

        // Hint
        let sel_count = self.selected.len();
        let hint = if sel_count > 0 {
            format!("{}selected  space:toggle  a:all  enter:confirm  F:folder-bind  esc:cancel",
                format!("{} ", sel_count))
        } else {
            "↑↓ move  space:toggle  a:all  enter:open/bind  F:folder-bind  backspace:up  esc:cancel".to_string()
        };
        f.render_widget(
            Paragraph::new(hint).style(theme::muted()),
            chunks[2],
        );
    }
}

pub enum SecretPickerAction {
    None,
    /// Legacy: cancel / esc
    Cancelled,
    /// Path changed — caller must dispatch LoadSecretFolders + LoadSecretNames for current_path()
    NeedsLoad,
    /// Single secret selected
    Selected(InfisicalRef),
    /// Multiple secrets selected via space+enter
    #[allow(dead_code)]
    SelectedMany(Vec<InfisicalRef>),
    /// Folder-level bind (F key on a folder entry)
    SelectedFolder {
        project_id: String,
        environment: String,
        secret_path: String,
        inject_prefix: Option<String>,
        snapshot: Vec<String>,
    },
}
