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
    pub project_id: FieldInput,
    pub environment: FieldInput,
    pub active_field: usize,
    pub status: Option<String>,
    pub status_is_error: bool,
    pub submit_state: SubmitState,
    #[allow(dead_code)]
    pub keyring_used: bool,
}

pub enum InfisicalSetupAction {
    None,
    Cancel,
    Save {
        site_url: String,
        client_id: String,
        client_secret: String,
        project_id: String,
        environment: String,
    },
}

impl InfisicalSetupState {
    pub fn new(existing: Option<&InfisicalConfig>) -> Self {
        let (site_url, project_id, environment) = existing.map(|c| {
            (
                c.site_url.clone(),
                c.default_project_id.clone().unwrap_or_default(),
                c.default_environment.clone(),
            )
        }).unwrap_or_else(|| (
            "https://app.infisical.com".to_string(),
            String::new(),
            "dev".to_string(),
        ));

        // Don't leak secrets back into UI — show placeholder if already set
        let cid = if existing.and_then(|c| c.client_id.as_ref()).is_some() {
            FieldInput { value: "(already set)".to_string(), cursor: 14, masked: false }
        } else {
            FieldInput::masked()
        };
        let csecret = if existing.and_then(|c| c.client_secret.as_ref()).is_some() {
            FieldInput { value: "(already set)".to_string(), cursor: 14, masked: false }
        } else {
            FieldInput::masked()
        };

        Self {
            site_url: FieldInput::new(&site_url),
            client_id: cid,
            client_secret: csecret,
            project_id: FieldInput::new(&project_id),
            environment: FieldInput::new(&environment),
            active_field: 0,
            status: None,
            status_is_error: false,
            submit_state: SubmitState::Idle,
            keyring_used: false,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.site_url.value.trim().is_empty() {
            return Err("site URL is required".to_string());
        }
        if self.client_id.value.trim().is_empty() {
            return Err("client ID is required".to_string());
        }
        if self.client_secret.value.trim().is_empty() {
            return Err("client secret is required".to_string());
        }
        if self.environment.value.trim().is_empty() {
            return Err("environment is required".to_string());
        }
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        !self.site_url.value.is_empty()
            || (!self.client_id.value.is_empty() && self.client_id.value != "(already set)")
            || (!self.client_secret.value.is_empty() && self.client_secret.value != "(already set)")
            || !self.project_id.value.is_empty()
    }

    pub fn handle_key(&mut self, code: KeyCode) -> InfisicalSetupAction {
        // Block all input except Esc while async save is in flight
        if matches!(self.submit_state, SubmitState::Saving { .. }) {
            if code == KeyCode::Esc {
                return InfisicalSetupAction::Cancel;
            }
            return InfisicalSetupAction::None;
        }
        match code {
            KeyCode::Esc => return InfisicalSetupAction::Cancel,
            KeyCode::Tab => {
                self.active_field = (self.active_field + 1) % 5;
            }
            KeyCode::BackTab => {
                self.active_field = self.active_field.checked_sub(1).unwrap_or(4);
            }
            KeyCode::Enter => {
                if let Err(msg) = self.validate() {
                    self.status = Some(msg);
                    self.status_is_error = true;
                    return InfisicalSetupAction::None;
                }
                let client_id = if self.client_id.value == "(already set)" {
                    String::new() // means keep existing
                } else {
                    self.client_id.value.clone()
                };
                let client_secret = if self.client_secret.value == "(already set)" {
                    String::new()
                } else {
                    self.client_secret.value.clone()
                };
                return InfisicalSetupAction::Save {
                    site_url: self.site_url.value.clone(),
                    client_id,
                    client_secret,
                    project_id: self.project_id.value.clone(),
                    environment: self.environment.value.clone(),
                };
            }
            code => {
                // Clear "(already set)" placeholder on first keystroke
                let field = self.active_field_mut();
                if field.value == "(already set)" {
                    field.value.clear();
                    field.cursor = 0;
                    // Restore masking
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
            3 => &mut self.project_id,
            4 => &mut self.environment,
            _ => &mut self.site_url,
        }
    }

    /// Render just the form fields (no outer dialog box). Used when embedded in another overlay.
    pub fn render_fields_only(&self, f: &mut Frame, area: Rect, focused: bool) {
        use super::wizards::profile::render_text_field;
        let chunks = Layout::vertical([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(area);

        let field_active = if focused { self.active_field } else { usize::MAX };
        render_text_field(f, &self.site_url, "Site URL", field_active == 0, chunks[0]);
        render_text_field(f, &self.client_id, "Client ID", field_active == 1, chunks[1]);
        render_text_field(f, &self.client_secret, "Client Secret", field_active == 2, chunks[2]);
        render_text_field(f, &self.project_id, "Default Project ID (optional)", field_active == 3, chunks[3]);
        render_text_field(f, &self.environment, "Default Environment", field_active == 4, chunks[4]);

        if let Some(ref msg) = self.status {
            let style = if self.status_is_error { theme::danger() } else { theme::ok() };
            f.render_widget(Paragraph::new(Span::styled(msg.clone(), style)), chunks[5]);
        } else {
            let hint = if focused { "tab:next  enter:save  esc:cancel" } else { "tab:focus form  esc:cancel" };
            f.render_widget(Paragraph::new(Span::styled(hint, theme::muted())), chunks[5]);
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let width = 64u16.min(area.width.saturating_sub(4));
        let height = 22u16.min(area.height.saturating_sub(2));
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
                Constraint::Length(3), // site_url
                Constraint::Length(3), // client_id
                Constraint::Length(3), // client_secret
                Constraint::Length(3), // project_id
                Constraint::Length(3), // environment
                Constraint::Length(1), // status / hint
                Constraint::Min(0),
            ])
            .split(inner);

        render_text_field(f, &self.site_url, "Site URL", self.active_field == 0, chunks[0]);
        render_text_field(f, &self.client_id, "Client ID", self.active_field == 1, chunks[1]);
        render_text_field(f, &self.client_secret, "Client Secret", self.active_field == 2, chunks[2]);
        render_text_field(f, &self.project_id, "Default Project ID (optional)", self.active_field == 3, chunks[3]);
        render_text_field(f, &self.environment, "Default Environment", self.active_field == 4, chunks[4]);

        // Status / hint line
        match &self.submit_state {
            SubmitState::Saving { .. } => {
                let frame = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| (d.subsec_millis() / 250) as usize % 4)
                    .unwrap_or(0);
                let spinner = ["⠋", "⠙", "⠹", "⠸"][frame];
                let msg = format!("{} Testing connection… (esc to cancel)", spinner);
                f.render_widget(Paragraph::new(Span::styled(msg, theme::accent_bold())), chunks[5]);
            }
            SubmitState::Err(ref msg) => {
                let text = format!("✗ {msg} — edit and retry, or esc to cancel");
                f.render_widget(Paragraph::new(Span::styled(text, theme::danger())), chunks[5]);
            }
            SubmitState::Idle => {
                if let Some(ref msg) = self.status {
                    let style = if self.status_is_error { theme::danger() } else { theme::ok() };
                    f.render_widget(Paragraph::new(Span::styled(msg.clone(), style)), chunks[5]);
                } else {
                    let hint = "tab:next  enter:save  esc:cancel  (credentials stored in keyring if available)";
                    f.render_widget(Paragraph::new(Span::styled(hint, theme::muted())), chunks[5]);
                }
            }
        }
    }
}
