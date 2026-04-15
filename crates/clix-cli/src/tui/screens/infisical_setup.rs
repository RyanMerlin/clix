use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use clix_core::state::InfisicalConfig;
use crate::tui::theme;
use crate::tui::widgets::form::FieldInput;
use super::wizards::profile::render_text_field;

pub struct InfisicalSetupState {
    pub site_url: FieldInput,
    pub client_id: FieldInput,
    pub client_secret: FieldInput,
    pub project_id: FieldInput,
    pub environment: FieldInput,
    pub active_field: usize,
    pub status: Option<String>,
    pub status_is_error: bool,
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
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> InfisicalSetupAction {
        match code {
            KeyCode::Esc => return InfisicalSetupAction::Cancel,
            KeyCode::Tab => {
                self.active_field = (self.active_field + 1) % 5;
            }
            KeyCode::BackTab => {
                self.active_field = self.active_field.checked_sub(1).unwrap_or(4);
            }
            KeyCode::Enter => {
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

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
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
        if let Some(ref msg) = self.status {
            let style = if self.status_is_error { theme::danger() } else { theme::ok() };
            f.render_widget(Paragraph::new(Span::styled(msg.clone(), style)), chunks[5]);
        } else {
            let hint = "tab:next  enter:save  esc:cancel  (credentials stored in keyring if available)";
            f.render_widget(Paragraph::new(Span::styled(hint, theme::muted())), chunks[5]);
        }
    }
}
