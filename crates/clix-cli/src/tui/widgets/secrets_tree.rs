use std::collections::{HashMap, HashSet};
use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use clix_core::manifest::capability::InfisicalRef;
use clix_core::state::InfisicalConfig;
use crate::tui::theme;
use crate::tui::work::JobId;

// ── data model ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind { Folder, Secret }

/// Per-path state: what we know about a folder's contents.
#[derive(Default, Debug, Clone)]
pub struct PathState {
    pub folders: Vec<String>,
    pub secrets: Vec<String>,
    pub loaded: bool,
    pub folders_loading: bool,
    pub secrets_loading: bool,
}

impl PathState {
    pub fn is_loading(&self) -> bool { self.folders_loading || self.secrets_loading }
}

/// One entry in the flattened visible list (computed on each render pass).
#[derive(Clone, Debug)]
pub struct VisibleRow {
    pub depth: usize,
    pub kind: NodeKind,
    pub name: String,
    /// Full Infisical secret_path for this node's *parent* folder.
    pub parent_path: String,
    /// Full secret_path including this node (for folders: with trailing /).
    pub full_path: String,
}

// ── widget ───────────────────────────────────────────────────────────────────

/// Async tree browser for an Infisical (project_id, environment).
/// Uses the existing `LoadSecretFolders` / `LoadSecretNames` work jobs,
/// keyed by path. Supports both Browse and Bind modes.
#[derive(Debug, Clone)]
pub struct SecretsTree {
    pub project_id: String,
    pub environment: String,
    pub mode: TreeMode,
    // path → contents
    pub paths: HashMap<String, PathState>,
    // which folder paths are currently expanded
    pub expanded: HashSet<String>,
    // flattened visible rows (recomputed when expanded changes)
    pub visible: Vec<VisibleRow>,
    pub cursor: usize,
    // multi-select: indices into visible
    pub selected: HashSet<usize>,
    // pending job ids: path → (folders_job_id, names_job_id)
    pub pending_folders: HashMap<String, JobId>,
    pub pending_names: HashMap<String, JobId>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TreeMode {
    /// Read-only browse (b key on Secrets screen)
    Browse,
    /// Bind mode: Enter on secret returns Selected, F on folder returns SelectedFolder
    Bind,
}

impl SecretsTree {
    pub fn new(project_id: &str, environment: &str, mode: TreeMode) -> Self {
        let mut tree = Self {
            project_id: project_id.to_string(),
            environment: environment.to_string(),
            mode,
            paths: HashMap::new(),
            expanded: {
                let mut s = HashSet::new();
                s.insert("/".to_string());  // root always expanded
                s
            },
            visible: vec![],
            cursor: 0,
            selected: HashSet::new(),
            pending_folders: HashMap::new(),
            pending_names: HashMap::new(),
            error: None,
        };
        // Mark root as needing load
        tree.paths.insert("/".to_string(), PathState {
            folders_loading: true,
            secrets_loading: true,
            ..Default::default()
        });
        tree
    }

    /// Returns (folders_job_id, names_job_id) for the root path. Caller must
    /// dispatch WorkRequest::LoadSecretFolders + LoadSecretNames with these ids.
    pub fn initial_load_ids(&mut self, folders_job: JobId, names_job: JobId) {
        self.pending_folders.insert("/".to_string(), folders_job);
        self.pending_names.insert("/".to_string(), names_job);
    }

    /// Request a load for a specific path. Returns the job ids to dispatch.
    /// Caller dispatches LoadSecretFolders + LoadSecretNames with these ids.
    pub fn request_load(&mut self, path: &str) -> (JobId, JobId) {
        let fid = crate::tui::work::next_job_id();
        let nid = crate::tui::work::next_job_id();
        self.pending_folders.insert(path.to_string(), fid);
        self.pending_names.insert(path.to_string(), nid);
        let state = self.paths.entry(path.to_string()).or_default();
        state.folders_loading = true;
        state.secrets_loading = true;
        (fid, nid)
    }

    /// Called when SecretFoldersLoaded arrives. Returns the path if we accepted it
    /// (so caller can re-compute visible).
    pub fn deliver_folders(&mut self, job_id: JobId, folders: Vec<String>, error: Option<String>) -> bool {
        let path = self.pending_folders.iter()
            .find(|(_, &jid)| jid == job_id)
            .map(|(p, _)| p.clone());
        let Some(path) = path else { return false; };
        self.pending_folders.remove(&path);
        let state = self.paths.entry(path).or_default();
        state.folders_loading = false;
        if let Some(e) = error {
            self.error = Some(format!("Folder list error: {e}"));
        } else {
            state.folders = folders;
        }
        if !state.is_loading() { state.loaded = true; }
        self.rebuild_visible();
        true
    }

    /// Called when SecretNamesLoaded arrives.
    pub fn deliver_names(&mut self, job_id: JobId, names: Vec<String>, error: Option<String>) -> bool {
        let path = self.pending_names.iter()
            .find(|(_, &jid)| jid == job_id)
            .map(|(p, _)| p.clone());
        let Some(path) = path else { return false; };
        self.pending_names.remove(&path);
        let state = self.paths.entry(path).or_default();
        state.secrets_loading = false;
        if let Some(e) = error {
            if self.error.is_none() { self.error = Some(format!("Secret list error: {e}")); }
        } else {
            state.secrets = names;
        }
        if !state.is_loading() { state.loaded = true; }
        self.rebuild_visible();
        true
    }

    /// Recompute the flat visible list from current tree state.
    pub fn rebuild_visible(&mut self) {
        let mut rows = vec![];
        self.dfs_collect("/", 0, &mut rows);
        self.visible = rows;
        // Clamp cursor
        if !self.visible.is_empty() && self.cursor >= self.visible.len() {
            self.cursor = self.visible.len() - 1;
        }
    }

    fn dfs_collect(&self, path: &str, depth: usize, out: &mut Vec<VisibleRow>) {
        let Some(state) = self.paths.get(path) else { return; };

        for folder in &state.folders {
            let full_path = if path == "/" {
                format!("/{}/", folder)
            } else {
                format!("{}{}/", path, folder)
            };
            out.push(VisibleRow {
                depth,
                kind: NodeKind::Folder,
                name: folder.clone(),
                parent_path: path.to_string(),
                full_path: full_path.clone(),
            });
            if self.expanded.contains(&full_path) {
                self.dfs_collect(&full_path, depth + 1, out);
            }
        }

        for secret in &state.secrets {
            out.push(VisibleRow {
                depth,
                kind: NodeKind::Secret,
                name: secret.clone(),
                parent_path: path.to_string(),
                full_path: format!("{}{}", path, secret),
            });
        }
    }

    pub fn is_root_loading(&self) -> bool {
        self.paths.get("/").map(|s| s.is_loading()).unwrap_or(true)
    }

    /// Returns the parent folder path of the cursor row, or "/" if the list is empty.
    pub fn current_path(&self) -> &str {
        self.visible.get(self.cursor)
            .map(|r| r.parent_path.as_str())
            .unwrap_or("/")
    }

    // ── key handling ─────────────────────────────────────────────────────────

    pub fn handle_key(&mut self, code: KeyCode, cfg: Option<&InfisicalConfig>) -> SecretsTreeAction {
        match code {
            KeyCode::Esc => return SecretsTreeAction::Cancelled,

            KeyCode::Up => {
                if self.cursor > 0 { self.cursor -= 1; }
            }
            KeyCode::Down => {
                if self.cursor + 1 < self.visible.len() { self.cursor += 1; }
            }
            KeyCode::PageUp => {
                self.cursor = self.cursor.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.cursor = (self.cursor + 10).min(self.visible.len().saturating_sub(1));
            }

            KeyCode::Right | KeyCode::Enter => {
                if let Some(row) = self.visible.get(self.cursor).cloned() {
                    match row.kind {
                        NodeKind::Folder => {
                            return self.toggle_or_enter_folder(&row, cfg);
                        }
                        NodeKind::Secret => {
                            if let Some(multi) = self.confirm_multi_select() {
                                return multi;
                            }
                            if self.mode == TreeMode::Bind {
                                return SecretsTreeAction::Selected(self.make_ref(&row));
                            }
                        }
                    }
                }
            }

            KeyCode::Left => {
                if let Some(row) = self.visible.get(self.cursor).cloned() {
                    if row.kind == NodeKind::Folder && self.expanded.contains(&row.full_path) {
                        self.expanded.remove(&row.full_path);
                        self.rebuild_visible();
                    } else if row.depth > 0 {
                        // Move cursor to the parent folder row
                        let parent = row.parent_path.clone();
                        if let Some(idx) = self.visible.iter().position(|r| r.full_path == parent) {
                            self.cursor = idx;
                        }
                    }
                }
            }

            // space: toggle multi-select for current entry (secrets only)
            KeyCode::Char(' ') => {
                if let Some(row) = self.visible.get(self.cursor) {
                    if row.kind == NodeKind::Secret {
                        if self.selected.contains(&self.cursor) {
                            self.selected.remove(&self.cursor);
                        } else {
                            self.selected.insert(self.cursor);
                        }
                    }
                }
            }

            // a: select all visible secrets
            KeyCode::Char('a') => {
                for (i, row) in self.visible.iter().enumerate() {
                    if row.kind == NodeKind::Secret {
                        self.selected.insert(i);
                    }
                }
            }

            // F: folder-level bind (Bind mode only)
            // snapshot is left empty — callers sync lazily via the existing folder-binding refresh path
            KeyCode::Char('F') => {
                let _ = cfg;
                if self.mode == TreeMode::Bind {
                    if let Some(row) = self.visible.get(self.cursor).cloned() {
                        if row.kind == NodeKind::Folder {
                            return SecretsTreeAction::SelectedFolder {
                                project_id: self.project_id.clone(),
                                environment: self.environment.clone(),
                                secret_path: row.full_path.clone(),
                                inject_prefix: None,
                                snapshot: vec![],
                            };
                        }
                    }
                }
            }

            _ => {}
        }
        SecretsTreeAction::None
    }

    fn toggle_or_enter_folder(&mut self, row: &VisibleRow, cfg: Option<&InfisicalConfig>) -> SecretsTreeAction {
        let _ = cfg;
        if self.expanded.contains(&row.full_path) {
            self.expanded.remove(&row.full_path);
            self.rebuild_visible();
        } else {
            self.expanded.insert(row.full_path.clone());
            // Lazy-load if not yet loaded
            if !self.paths.get(&row.full_path).map(|s| s.loaded).unwrap_or(false)
                && !self.pending_folders.contains_key(&row.full_path) {
                return SecretsTreeAction::NeedsLoad(row.full_path.clone());
            }
            self.rebuild_visible();
        }
        SecretsTreeAction::None
    }

    fn confirm_multi_select(&mut self) -> Option<SecretsTreeAction> {
        if self.selected.is_empty() { return None; }
        let refs: Vec<InfisicalRef> = self.selected.iter()
            .filter_map(|&idx| self.visible.get(idx))
            .filter(|r| r.kind == NodeKind::Secret)
            .map(|r| self.make_ref(r))
            .collect();
        if refs.is_empty() { return None; }
        Some(SecretsTreeAction::SelectedMany(refs))
    }

    fn make_ref(&self, row: &VisibleRow) -> InfisicalRef {
        InfisicalRef {
            secret_name: row.name.clone(),
            project_id: Some(self.project_id.clone()),
            environment: self.environment.clone(),
            secret_path: row.parent_path.clone(),
            infisical_profile: None,
        }
    }

    // ── render ────────────────────────────────────────────────────────────────

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let width = area.width.saturating_sub(4).clamp(54, 90);
        let height = area.height.saturating_sub(4).clamp(14, 28);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let dialog = Rect::new(x, y, width, height);

        f.render_widget(Clear, dialog);

        let mode_badge = if self.mode == TreeMode::Bind { " [BIND]" } else { "" };
        let title = format!(
            " Infisical · {} · {}{} ",
            self.project_id, self.environment, mode_badge
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title, theme::accent_bold()))
            .border_style(theme::border_focused());
        let inner = block.inner(dialog);
        f.render_widget(block, dialog);

        let chunks = Layout::vertical([
                Constraint::Length(1),  // path breadcrumb
                Constraint::Min(0),     // tree list
                Constraint::Length(1),  // hint
            ])
            .split(inner);

        // Breadcrumb: shows current folder path
        let raw = self.current_path();
        let crumb = if raw == "/" {
            "/".to_string()
        } else {
            raw.trim_start_matches('/').trim_end_matches('/').replace('/', " › ")
        };
        f.render_widget(
            Paragraph::new(Span::styled(format!(" {}", crumb), theme::dim())),
            chunks[0],
        );

        self.render_tree(f, chunks[1]);
        self.render_hint(f, chunks[2]);
    }

    fn render_tree(&self, f: &mut Frame, area: Rect) {
        if let Some(err) = &self.error {
            f.render_widget(
                Paragraph::new(Span::styled(err.as_str(), theme::danger())),
                area,
            );
            return;
        }

        if self.is_root_loading() {
            let frame = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| (d.subsec_millis() / 250) as usize % 4)
                .unwrap_or(0);
            let spinner = ["⠋", "⠙", "⠹", "⠸"][frame];
            f.render_widget(
                Paragraph::new(Span::styled(format!("  {} Loading…", spinner), theme::muted())),
                area,
            );
            return;
        }

        if self.visible.is_empty() {
            f.render_widget(
                Paragraph::new(Span::styled("  (empty — no secrets or folders found)", theme::inactive())),
                area,
            );
            return;
        }

        let items: Vec<ListItem> = self.visible.iter().enumerate().map(|(i, row)| {
            let indent = "  ".repeat(row.depth);
            let is_cursor = i == self.cursor;
            let is_selected = self.selected.contains(&i);

            let (icon, base_style) = match row.kind {
                NodeKind::Folder => {
                    let expanded = self.expanded.contains(&row.full_path);
                    let loading = self.pending_folders.contains_key(&row.full_path)
                        || self.pending_names.contains_key(&row.full_path);
                    let icon = if loading { "⠸ " } else if expanded { "▾  " } else { "▸  " };
                    (icon, if is_cursor { theme::selected() } else { theme::accent() })
                }
                NodeKind::Secret => {
                    let icon = if is_selected { "[✓]" } else { "[ ]" };
                    let style = if is_cursor { theme::selected() } else if is_selected { theme::ok() } else { theme::normal() };
                    (icon, style)
                }
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("  {}{} {}", indent, icon, row.name), base_style),
            ]))
        }).collect();

        let list = List::new(items).highlight_style(theme::selected());
        let mut state = ListState::default();
        state.select(Some(self.cursor));
        f.render_stateful_widget(list, area, &mut state);
    }

    fn render_hint(&self, f: &mut Frame, area: Rect) {
        let sel_count = self.selected.len();
        let hint = if sel_count > 0 {
            let folder_hint = if self.mode == TreeMode::Bind { "  F:folder-bind" } else { "" };
            format!("{} selected  space:toggle  a:all  enter:confirm{}  esc:cancel", sel_count, folder_hint)
        } else if self.mode == TreeMode::Bind {
            "↑↓ move  →/enter:open  ←:close  space:toggle  a:all  F:folder-bind  esc:cancel".to_string()
        } else {
            "↑↓ move  →/enter:expand  ←:collapse  esc:close".to_string()
        };
        f.render_widget(
            Paragraph::new(hint).style(theme::muted()),
            area,
        );
    }
}

// ── action ────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub enum SecretsTreeAction {
    None,
    Cancelled,
    /// A folder path needs loading — caller dispatches the jobs
    NeedsLoad(String),
    /// Single secret selected (Bind mode)
    Selected(InfisicalRef),
    /// Multi-select confirmed
    SelectedMany(Vec<InfisicalRef>),
    /// Folder-level bind (F on a folder in Bind mode)
    SelectedFolder {
        project_id: String,
        environment: String,
        secret_path: String,
        inject_prefix: Option<String>,
        snapshot: Vec<String>,
    },
}
