use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use clix_core::state::InfisicalConfig;
use crate::tui::theme;
use crate::tui::widgets::form::FieldInput;
use super::infisical_setup::{InfisicalSetupState, InfisicalSetupAction};

// ── state ────────────────────────────────────────────────────────────────────

pub enum AccountsMode {
    List,
    Adding {
        name_input: FieldInput,
        name_active: bool,  // true = focus on name field, false = on setup form
        setup: InfisicalSetupState,
    },
    Editing {
        profile_name: String,
        setup: InfisicalSetupState,
    },
    ConfirmRemove(String),
}

pub struct InfisicalAccountsState {
    pub cursor: usize,
    /// Ordered snapshot of (name, config) from ClixConfig
    pub profiles: Vec<(String, InfisicalConfig)>,
    pub active: Option<String>,
    pub mode: AccountsMode,
    pub status: Option<(String, bool)>,  // (message, is_error)
}

pub enum AccountsAction {
    None,
    Cancel,
    /// Switch active profile in-place (no reload needed for display, but do save)
    SetActive(String),
    /// Remove profile
    Remove(String),
    /// Save new or updated profile
    Save {
        name: String,
        site_url: String,
        client_id: String,
        client_secret: String,
        project_id: String,
        environment: String,
    },
    /// Test connectivity for a named profile
    Test(String),
}

impl InfisicalAccountsState {
    pub fn new(profiles: Vec<(String, InfisicalConfig)>, active: Option<String>) -> Self {
        Self {
            cursor: 0,
            profiles,
            active,
            mode: AccountsMode::List,
            status: None,
        }
    }

    #[allow(dead_code)]
    pub fn refresh(&mut self, profiles: Vec<(String, InfisicalConfig)>, active: Option<String>) {
        self.profiles = profiles;
        self.active = active;
        if !self.profiles.is_empty() && self.cursor >= self.profiles.len() {
            self.cursor = self.profiles.len() - 1;
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> AccountsAction {
        match self.mode {
            AccountsMode::List => self.handle_list(code),
            AccountsMode::Adding { .. } => self.handle_adding(code),
            AccountsMode::Editing { .. } => self.handle_editing(code),
            AccountsMode::ConfirmRemove(_) => self.handle_confirm_remove(code),
        }
    }

    fn handle_list(&mut self, code: KeyCode) -> AccountsAction {
        match code {
            KeyCode::Esc => return AccountsAction::Cancel,
            KeyCode::Up => {
                if self.cursor > 0 { self.cursor -= 1; }
            }
            KeyCode::Down => {
                if self.cursor + 1 < self.profiles.len() { self.cursor += 1; }
            }
            // a = add new profile
            KeyCode::Char('a') => {
                self.mode = AccountsMode::Adding {
                    name_input: FieldInput::default(),
                    name_active: true,
                    setup: InfisicalSetupState::new(None),
                };
                self.status = None;
            }
            // e = edit selected
            KeyCode::Char('e') | KeyCode::Enter => {
                if let Some((name, cfg)) = self.profiles.get(self.cursor) {
                    self.mode = AccountsMode::Editing {
                        profile_name: name.clone(),
                        setup: InfisicalSetupState::new(Some(cfg)),
                    };
                    self.status = None;
                }
            }
            // u = set active
            KeyCode::Char('u') => {
                if let Some((name, _)) = self.profiles.get(self.cursor) {
                    return AccountsAction::SetActive(name.clone());
                }
            }
            // d = remove
            KeyCode::Char('d') => {
                if let Some((name, _)) = self.profiles.get(self.cursor) {
                    self.mode = AccountsMode::ConfirmRemove(name.clone());
                    self.status = None;
                }
            }
            // t = test
            KeyCode::Char('t') => {
                if let Some((name, _)) = self.profiles.get(self.cursor) {
                    return AccountsAction::Test(name.clone());
                }
            }
            _ => {}
        }
        AccountsAction::None
    }

    fn handle_adding(&mut self, code: KeyCode) -> AccountsAction {
        let AccountsMode::Adding { ref mut name_input, ref mut name_active, ref mut setup } = self.mode else {
            return AccountsAction::None;
        };

        match code {
            KeyCode::Esc => {
                self.mode = AccountsMode::List;
                return AccountsAction::None;
            }
            // Tab moves from name field into the setup form
            KeyCode::Tab if *name_active => {
                *name_active = false;
            }
            KeyCode::BackTab if !*name_active => {
                *name_active = true;
            }
            KeyCode::Enter if *name_active => {
                let name = name_input.value.trim().to_string();
                if name.is_empty() {
                    self.status = Some(("Profile name is required".into(), true));
                    return AccountsAction::None;
                }
                if self.profiles.iter().any(|(n, _)| n == &name) {
                    self.status = Some((format!("Profile '{}' already exists", name), true));
                    return AccountsAction::None;
                }
                *name_active = false;
            }
            _ if *name_active => {
                name_input.handle_key(code);
            }
            _ => {
                let name = name_input.value.trim().to_string();
                let action = setup.handle_key(code);
                match action {
                    InfisicalSetupAction::Cancel => {
                        self.mode = AccountsMode::List;
                    }
                    InfisicalSetupAction::Save { site_url, client_id, client_secret, project_id, environment } => {
                        if name.is_empty() {
                            self.status = Some(("Enter a profile name first (press BackTab)".into(), true));
                        } else if self.profiles.iter().any(|(n, _)| n == &name) {
                            self.status = Some((format!("Profile '{}' already exists", name), true));
                        } else {
                            self.mode = AccountsMode::List;
                            return AccountsAction::Save { name, site_url, client_id, client_secret, project_id, environment };
                        }
                    }
                    InfisicalSetupAction::None => {}
                }
            }
        }
        AccountsAction::None
    }

    fn handle_editing(&mut self, code: KeyCode) -> AccountsAction {
        let AccountsMode::Editing { ref profile_name, ref mut setup } = self.mode else {
            return AccountsAction::None;
        };
        let action = setup.handle_key(code);
        let name = profile_name.clone();
        match action {
            InfisicalSetupAction::Cancel => {
                self.mode = AccountsMode::List;
            }
            InfisicalSetupAction::Save { site_url, client_id, client_secret, project_id, environment } => {
                self.mode = AccountsMode::List;
                return AccountsAction::Save { name, site_url, client_id, client_secret, project_id, environment };
            }
            InfisicalSetupAction::None => {}
        }
        AccountsAction::None
    }

    fn handle_confirm_remove(&mut self, code: KeyCode) -> AccountsAction {
        let AccountsMode::ConfirmRemove(ref name) = self.mode else {
            return AccountsAction::None;
        };
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let name = name.clone();
                self.mode = AccountsMode::List;
                return AccountsAction::Remove(name);
            }
            _ => {
                self.mode = AccountsMode::List;
            }
        }
        AccountsAction::None
    }

    // ── render ────────────────────────────────────────────────────────────────

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let width = area.width.saturating_sub(4).max(60).min(80);
        let height = area.height.saturating_sub(4).max(14).min(30);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let dialog = Rect::new(x, y, width, height);

        f.render_widget(Clear, dialog);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(" Infisical Accounts ", theme::accent_bold()))
            .border_style(theme::border_focused());
        let inner = block.inner(dialog);
        f.render_widget(block, dialog);

        match self.mode {
            AccountsMode::List => self.render_list(f, inner),
            AccountsMode::Adding { ref name_input, name_active, ref setup } => {
                self.render_add_form(f, inner, name_input, name_active, setup);
            }
            AccountsMode::Editing { ref profile_name, ref setup } => {
                self.render_edit_form(f, inner, profile_name, setup);
            }
            AccountsMode::ConfirmRemove(ref name) => {
                self.render_confirm_remove(f, inner, name);
            }
        }
    }

    fn render_list(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        if self.profiles.is_empty() {
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(""),
                    Line::from(Span::styled("  No profiles configured.", theme::muted())),
                    Line::from(""),
                    Line::from(Span::styled("  Press a to add one.", theme::dim())),
                ]),
                chunks[0],
            );
        } else {
            let items: Vec<ListItem> = self.profiles.iter().enumerate().map(|(i, (name, cfg))| {
                let is_active = self.active.as_deref() == Some(name.as_str());
                let is_cursor = i == self.cursor;
                let active_badge = if is_active { " *" } else { "" };
                let style = if is_cursor { theme::selected() } else if is_active { theme::ok() } else { theme::normal() };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {:<18}{}", format!("{}{}", name, active_badge), cfg.site_url), style),
                ]))
            }).collect();

            let list = List::new(items).highlight_style(theme::selected());
            let mut state = ListState::default();
            state.select(Some(self.cursor));
            f.render_stateful_widget(list, chunks[0], &mut state);
        }

        let status_line = if let Some((ref msg, is_err)) = self.status {
            Span::styled(msg.as_str(), if is_err { theme::danger() } else { theme::ok() })
        } else {
            Span::styled("a:add  e/enter:edit  u:use  d:remove  t:test  esc:close", theme::muted())
        };
        f.render_widget(Paragraph::new(status_line), chunks[1]);
    }

    fn render_add_form(&self, f: &mut Frame, area: Rect, name_input: &FieldInput, name_active: bool, setup: &InfisicalSetupState) {
        let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        // Name field
        use super::wizards::profile::render_text_field;
        render_text_field(f, name_input, "Profile Name", name_active, chunks[0]);

        // Reuse InfisicalSetupState render but in the remaining space
        setup.render_fields_only(f, chunks[1], !name_active);
    }

    fn render_edit_form(&self, f: &mut Frame, area: Rect, profile_name: &str, setup: &InfisicalSetupState) {
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        f.render_widget(
            Paragraph::new(Span::styled(
                format!("  Editing: {}", profile_name),
                theme::accent_bold(),
            )),
            chunks[0],
        );
        setup.render_fields_only(f, chunks[1], true);
    }

    fn render_confirm_remove(&self, f: &mut Frame, area: Rect, name: &str) {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  Remove profile '{}'?", name),
                theme::danger(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  This will delete its credentials from the keyring.",
                theme::muted(),
            )),
            Line::from(""),
            Line::from(Span::styled("  y: confirm   any other key: cancel", theme::dim())),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }
}
