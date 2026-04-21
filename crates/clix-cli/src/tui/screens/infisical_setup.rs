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
    pub service_token: FieldInput,
    pub project_id: FieldInput,
    pub environment: FieldInput,
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

        let service_token = if existing.and_then(|c| c.service_token.as_ref()).is_some() {
            FieldInput { value: "(already set)".to_string(), cursor: 14, masked: false }
        } else {
            FieldInput::masked()
        };

        Self {
            site_url: FieldInput::new(&site_url),
            service_token,
            project_id: FieldInput::new(&project_id),
            environment: FieldInput::new(&environment),
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
        let tok = &self.service_token.value;
        if !tok.trim().is_empty() && tok != "(already set)" && !tok.trim().starts_with("st.") {
            return Err("Service token should start with 'st.' — check your Infisical project".to_string());
        }
        if self.environment.value.trim().is_empty() {
            return Err("Environment is required".to_string());
        }
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        !self.site_url.value.is_empty()
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
            KeyCode::Tab => {
                self.active_field = (self.active_field + 1) % 4;
            }
            KeyCode::BackTab => {
                self.active_field = self.active_field.checked_sub(1).unwrap_or(3);
            }
            KeyCode::Enter => {
                if let Err(msg) = self.validate() {
                    self.status = Some(msg);
                    self.status_is_error = true;
                    return InfisicalSetupAction::None;
                }
                let service_token = if self.service_token.value == "(already set)" {
                    String::new() // empty = keep existing
                } else {
                    self.service_token.value.clone()
                };
                return InfisicalSetupAction::Save {
                    site_url: self.site_url.value.clone(),
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
            1 => &mut self.service_token,
            2 => &mut self.project_id,
            3 => &mut self.environment,
            _ => &mut self.site_url,
        }
    }

    /// Render just the form fields (no outer dialog box). Used when embedded in another overlay.
    pub fn render_fields_only(&self, f: &mut Frame, area: Rect, focused: bool) {
        let chunks = Layout::vertical([
            Constraint::Length(3), // site_url
            Constraint::Length(3), // service_token
            Constraint::Length(3), // project_id
            Constraint::Length(3), // environment
            Constraint::Length(1), // status / hint
            Constraint::Min(0),
        ]).split(area);

        let fa = if focused { self.active_field } else { usize::MAX };
        render_text_field(f, &self.site_url, "Site URL", fa == 0, chunks[0]);
        render_text_field(f, &self.service_token, "Service Token (st.xxx)", fa == 1, chunks[1]);
        render_text_field(f, &self.project_id, "Project ID (optional — scopes tree browser)", fa == 2, chunks[2]);
        render_text_field(f, &self.environment, "Default Environment", fa == 3, chunks[3]);

        if let Some(ref msg) = self.status {
            let style = if self.status_is_error { theme::danger() } else { theme::ok() };
            f.render_widget(Paragraph::new(Span::styled(msg.clone(), style)), chunks[4]);
        } else {
            let hint = if focused { "tab:next  enter:save  esc:cancel" } else { "tab:focus form  esc:cancel" };
            f.render_widget(Paragraph::new(Span::styled(hint, theme::muted())), chunks[4]);
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

        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(0),
        ]).split(inner);

        render_text_field(f, &self.site_url, "Site URL", self.active_field == 0, chunks[0]);
        render_text_field(f, &self.service_token, "Service Token (st.xxx)", self.active_field == 1, chunks[1]);
        render_text_field(f, &self.project_id, "Project ID (optional)", self.active_field == 2, chunks[2]);
        render_text_field(f, &self.environment, "Default Environment", self.active_field == 3, chunks[3]);

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
                    chunks[4],
                );
            }
            SubmitState::Err(ref msg) => {
                f.render_widget(
                    Paragraph::new(Span::styled(
                        format!("✗ {msg} — edit and retry, or esc to cancel"),
                        theme::danger(),
                    )),
                    chunks[4],
                );
            }
            SubmitState::Idle => {
                if let Some(ref msg) = self.status {
                    let style = if self.status_is_error { theme::danger() } else { theme::ok() };
                    f.render_widget(Paragraph::new(Span::styled(msg.clone(), style)), chunks[4]);
                } else {
                    f.render_widget(
                        Paragraph::new(Span::styled(
                            "tab:next  enter:save  esc:cancel  (token stored in keyring if available)",
                            theme::muted(),
                        )),
                        chunks[4],
                    );
                }
            }
        }
    }
}
