use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use clix_core::state::InfisicalConfig;
use crate::tui::theme;
use crate::tui::widgets::form::FieldInput;
use crate::tui::work::JobId;
use super::wizards::profile::render_text_field;

pub enum SubmitState {
    Idle,
    Saving { job_id: JobId },
    Err(String),
}

pub struct InfisicalSetupState {
    pub site_url: FieldInput,
    pub client_id: FieldInput,
    pub client_secret: FieldInput,
    pub service_token: FieldInput,
    pub project_id: FieldInput,
    pub environment: FieldInput,
    pub show_advanced: bool,
    pub active_field: usize,
    pub status: Option<String>,
    pub status_is_error: bool,
    pub submit_state: SubmitState,
}

pub enum InfisicalSetupAction {
    None,
    Cancel,
    Save {
        site_url: String,
        client_id: String,
        client_secret: String,
        service_token: String,   // empty = keep existing
        project_id: String,
        environment: String,
    },
}

impl InfisicalSetupState {
    pub fn new(existing: Option<&InfisicalConfig>) -> Self {
        let (site_url, project_id, environment) = existing.map(|c| (
            c.site_url.clone(),
            c.default_project_id.clone().unwrap_or_default(),
            c.default_environment.clone(),
        )).unwrap_or_else(|| (
            "https://app.infisical.com".to_string(),
            String::new(),
            "dev".to_string(),
        ));

        let client_id = existing
            .and_then(|c| c.client_id.as_ref())
            .map(|v| FieldInput::new(v))
            .unwrap_or_else(FieldInput::default);
        let client_secret = if existing.and_then(|c| c.client_secret.as_ref()).is_some() {
            FieldInput { value: "(already set)".to_string(), cursor: 14, masked: true }
        } else {
            FieldInput::masked()
        };
        let service_token = if existing.and_then(|c| c.service_token.as_ref()).is_some() {
            FieldInput { value: "(already set)".to_string(), cursor: 14, masked: true }
        } else {
            FieldInput::masked()
        };

        let show_advanced = existing.and_then(|c| c.service_token.as_ref()).is_some()
            && !(existing.and_then(|c| c.client_id.as_ref()).is_some()
                && existing.and_then(|c| c.client_secret.as_ref()).is_some());

        Self {
            site_url: FieldInput::new(&site_url),
            client_id,
            client_secret,
            service_token,
            project_id: FieldInput::new(&project_id),
            environment: FieldInput::new(&environment),
            show_advanced,
            active_field: 0,
            status: None,
            status_is_error: false,
            submit_state: SubmitState::Idle,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.site_url.value.trim().is_empty() {
            return Err("Site URL is required".to_string());
        }
        let client_id = self.client_id.value.trim();
        let client_secret = self.client_secret.value.trim();
        let service_token = self.service_token.value.trim();
        let universal_auth_set = !client_id.is_empty() && client_id != "(already set)"
            && !client_secret.is_empty() && client_secret != "(already set)";
        let service_token_set = !service_token.is_empty() && service_token != "(already set)";
        if !universal_auth_set && !service_token_set {
            return Err("Provide universal auth client_id/client_secret".to_string());
        }
        if service_token_set && !service_token.starts_with("st.") {
            return Err("Service token should start with 'st.' — check your Infisical project".to_string());
        }
        if self.environment.value.trim().is_empty() {
            return Err("Environment is required".to_string());
        }
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        !self.site_url.value.is_empty()
            || !self.client_id.value.is_empty()
            || (!self.client_secret.value.is_empty() && self.client_secret.value != "(already set)")
            || (!self.service_token.value.is_empty() && self.service_token.value != "(already set)")
            || !self.project_id.value.is_empty()
    }

    pub fn handle_key(&mut self, code: KeyCode) -> InfisicalSetupAction {
        if matches!(self.submit_state, SubmitState::Saving { .. }) {
            if code == KeyCode::Esc {
                return InfisicalSetupAction::Cancel;
            }
            return InfisicalSetupAction::None;
        }
        match code {
            KeyCode::Esc => return InfisicalSetupAction::Cancel,
            KeyCode::Char('v') | KeyCode::Char('V') => {
                let was_advanced = self.show_advanced;
                self.show_advanced = !self.show_advanced;
                if was_advanced && !self.show_advanced && self.active_field > 3 {
                    self.active_field -= 1;
                } else if !was_advanced && self.show_advanced && self.active_field >= 3 {
                    self.active_field += 1;
                }
            }
            KeyCode::Tab => {
                self.active_field = (self.active_field + 1) % self.field_count();
            }
            KeyCode::BackTab => {
                self.active_field = self.active_field.checked_sub(1).unwrap_or(self.field_count().saturating_sub(1));
            }
            KeyCode::Enter => {
                if let Err(msg) = self.validate() {
                    self.status = Some(msg);
                    self.status_is_error = true;
                    return InfisicalSetupAction::None;
                }
                let client_id = if self.client_id.value == "(already set)" {
                    String::new()
                } else {
                    self.client_id.value.clone()
                };
                let client_secret = if self.client_secret.value == "(already set)" {
                    String::new()
                } else {
                    self.client_secret.value.clone()
                };
                let service_token = if self.service_token.value == "(already set)" {
                    String::new() // empty = keep existing
                } else {
                    self.service_token.value.clone()
                };
                return InfisicalSetupAction::Save {
                    site_url: self.site_url.value.clone(),
                    client_id,
                    client_secret,
                    service_token,
                    project_id: self.project_id.value.clone(),
                    environment: self.environment.value.clone(),
                };
            }
            code => {
                let field = self.active_field_mut();
                if field.value == "(already set)" {
                    field.value.clear();
                    field.cursor = 0;
                    field.masked = true;
                }
                field.handle_key(code);
            }
        }
        InfisicalSetupAction::None
    }

    fn active_field_mut(&mut self) -> &mut FieldInput {
        match self.active_field {
            0 => &mut self.site_url,
            1 => &mut self.client_id,
            2 => &mut self.client_secret,
            3 if self.show_advanced => &mut self.service_token,
            3 => &mut self.project_id,
            4 if self.show_advanced => &mut self.project_id,
            4 => &mut self.environment,
            5 if self.show_advanced => &mut self.environment,
            _ => &mut self.site_url,
        }
    }

    fn field_count(&self) -> usize {
        if self.show_advanced { 6 } else { 5 }
    }

    /// Render just the form fields (no outer dialog box). Used when embedded in another overlay.
    pub fn render_fields_only(&self, f: &mut Frame, area: Rect, focused: bool) {
        let mut constraints = vec![
            Constraint::Length(3), // site_url
            Constraint::Length(3), // client_id
            Constraint::Length(3), // client_secret
        ];
        if self.show_advanced {
            constraints.push(Constraint::Length(3)); // service_token
        }
        constraints.push(Constraint::Length(3)); // project_id
        constraints.push(Constraint::Length(3)); // environment
        constraints.push(Constraint::Length(1)); // status / hint
        constraints.push(Constraint::Min(0));
        let chunks = Layout::vertical(constraints).split(area);

        let fa = if focused { self.active_field } else { usize::MAX };
        render_text_field(f, &self.site_url, "Site URL", fa == 0, chunks[0]);
        render_text_field(f, &self.client_id, "Universal Auth Client ID", fa == 1, chunks[1]);
        render_text_field(f, &self.client_secret, "Universal Auth Client Secret", fa == 2, chunks[2]);
        let mut idx = 3;
        if self.show_advanced {
            render_text_field(f, &self.service_token, "Service Token (advanced fallback)", fa == 3, chunks[idx]);
            idx += 1;
        }
        render_text_field(f, &self.project_id, "Project ID (optional — scopes tree browser)", fa == idx, chunks[idx]);
        idx += 1;
        render_text_field(f, &self.environment, "Default Environment", fa == idx, chunks[idx]);

        if let Some(ref msg) = self.status {
            let style = if self.status_is_error { theme::danger() } else { theme::ok() };
            f.render_widget(Paragraph::new(Span::styled(msg.clone(), style)), chunks[idx + 1]);
        } else {
            let hint = if focused {
                "tab:next  v:advanced  enter:save  esc:cancel"
            } else {
                "tab:focus form  esc:cancel"
            };
            f.render_widget(Paragraph::new(Span::styled(hint, theme::muted())), chunks[idx + 1]);
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let width = 68u16.min(area.width.saturating_sub(4));
        let height = 20u16.min(area.height.saturating_sub(2));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let dialog = Rect::new(x, y, width, height);

        f.render_widget(Clear, dialog);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(" Configure Infisical ", theme::accent_bold()))
            .border_style(theme::border_focused());
        let inner = block.inner(dialog);
        f.render_widget(block, dialog);

        let mut constraints = vec![
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ];
        if self.show_advanced {
            constraints.push(Constraint::Length(3));
        }
        constraints.push(Constraint::Length(3));
        constraints.push(Constraint::Length(3));
        constraints.push(Constraint::Length(1));
        constraints.push(Constraint::Min(0));
        let chunks = Layout::vertical(constraints).split(inner);

        render_text_field(f, &self.site_url, "Site URL", self.active_field == 0, chunks[0]);
        render_text_field(f, &self.client_id, "Universal Auth Client ID", self.active_field == 1, chunks[1]);
        render_text_field(f, &self.client_secret, "Universal Auth Client Secret", self.active_field == 2, chunks[2]);
        let mut idx = 3;
        if self.show_advanced {
            render_text_field(f, &self.service_token, "Service Token (advanced fallback)", self.active_field == 3, chunks[idx]);
            idx += 1;
        }
        render_text_field(f, &self.project_id, "Project ID (optional)", self.active_field == idx, chunks[idx]);
        idx += 1;
        render_text_field(f, &self.environment, "Default Environment", self.active_field == idx, chunks[idx]);

        match &self.submit_state {
            SubmitState::Saving { .. } => {
                let frame = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| (d.subsec_millis() / 250) as usize % 4)
                    .unwrap_or(0);
                let spinner = ["⠋", "⠙", "⠹", "⠸"][frame];
                f.render_widget(
                    Paragraph::new(Span::styled(
                        format!("{spinner} Testing connection… (esc to cancel)"),
                        theme::accent_bold(),
                    )),
                    chunks[idx + 1],
                );
            }
            SubmitState::Err(ref msg) => {
                f.render_widget(
                    Paragraph::new(Span::styled(
                        format!("✗ {msg} — edit and retry, or esc to cancel"),
                        theme::danger(),
                    )),
                    chunks[idx + 1],
                );
            }
            SubmitState::Idle => {
                if let Some(ref msg) = self.status {
                    let style = if self.status_is_error { theme::danger() } else { theme::ok() };
                    f.render_widget(Paragraph::new(Span::styled(msg.clone(), style)), chunks[idx + 1]);
                } else {
                    f.render_widget(
                        Paragraph::new(Span::styled(
                            "tab:next  v:advanced  enter:save  esc:cancel  (auth stored in keyring if available)",
                            theme::muted(),
                        )),
                        chunks[idx + 1],
                    );
                }
            }
        }
    }
}
