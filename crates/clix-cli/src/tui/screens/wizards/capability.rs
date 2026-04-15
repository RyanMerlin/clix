use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use crate::tui::theme;
use crate::tui::widgets::form::{FieldInput, SelectField};
use super::profile::render_text_field;

#[derive(Debug, Clone, PartialEq)]
pub enum CapWizardStep {
    Form,     // fill in the fields
    Confirm,  // review before writing
}

#[derive(Debug, Clone)]
pub struct CapabilityWizard {
    pub step: CapWizardStep,
    pub name: FieldInput,
    pub description: FieldInput,
    pub command: FieldInput,
    pub args: FieldInput,
    pub risk: SelectField,
    pub side_effect: SelectField,
    pub active_field: usize,
    pub error: Option<String>,
}

pub enum CapWizardAction {
    None,
    Cancel,
    Submit {
        name: String,
        description: String,
        command: String,
        args: Vec<String>,
        risk: String,
        side_effect: String,
    },
}

impl CapabilityWizard {
    pub fn new() -> Self {
        Self {
            step: CapWizardStep::Form,
            name: FieldInput::default(),
            description: FieldInput::default(),
            command: FieldInput::default(),
            args: FieldInput::default(),
            risk: SelectField::new(vec!["low", "medium", "high", "critical"]),
            side_effect: SelectField::new(vec!["none", "readOnly", "additive", "mutating", "destructive"]),
            active_field: 0,
            error: None,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> CapWizardAction {
        match &self.step {
            CapWizardStep::Form => self.handle_form(code),
            CapWizardStep::Confirm => self.handle_confirm(code),
        }
    }

    fn handle_form(&mut self, code: KeyCode) -> CapWizardAction {
        const FIELD_COUNT: usize = 6;  // name, desc, command, args, risk, side_effect
        match code {
            KeyCode::Esc => return CapWizardAction::Cancel,
            KeyCode::Tab => self.active_field = (self.active_field + 1) % FIELD_COUNT,
            KeyCode::BackTab => self.active_field = self.active_field.checked_sub(1).unwrap_or(FIELD_COUNT - 1),
            KeyCode::Enter => {
                self.error = None;
                let name = self.name.value.trim().to_string();
                let cmd = self.command.value.trim().to_string();
                if name.is_empty() {
                    self.error = Some("Name is required".into());
                    return CapWizardAction::None;
                }
                if !validate_cap_name(&name) {
                    self.error = Some("Name must be dot-namespaced lowercase (e.g. gh.pr.list)".into());
                    return CapWizardAction::None;
                }
                if cmd.is_empty() {
                    self.error = Some("Command is required".into());
                    return CapWizardAction::None;
                }
                self.step = CapWizardStep::Confirm;
            }
            _ => {
                self.error = None;
                match self.active_field {
                    0 => self.name.handle_key(code),
                    1 => self.description.handle_key(code),
                    2 => self.command.handle_key(code),
                    3 => self.args.handle_key(code),
                    4 => self.risk.handle_key(code),
                    5 => self.side_effect.handle_key(code),
                    _ => {}
                }
            }
        }
        CapWizardAction::None
    }

    fn handle_confirm(&mut self, code: KeyCode) -> CapWizardAction {
        match code {
            KeyCode::Esc => self.step = CapWizardStep::Form,
            KeyCode::Enter | KeyCode::Char('w') => {
                let args: Vec<String> = self.args.value
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                return CapWizardAction::Submit {
                    name: self.name.value.trim().to_string(),
                    description: self.description.value.trim().to_string(),
                    command: self.command.value.trim().to_string(),
                    args,
                    risk: self.risk.current().to_string(),
                    side_effect: self.side_effect.current().to_string(),
                };
            }
            _ => {}
        }
        CapWizardAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let width = area.width.saturating_sub(4).max(50);
        let height = area.height.saturating_sub(2).max(10);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let dialog = Rect::new(x, y, width, height);

        f.render_widget(Clear, dialog);

        let (step_n, step_total) = match self.step {
            CapWizardStep::Form => (0, 2),
            CapWizardStep::Confirm => (1, 2),
        };
        let dots = (0..step_total).map(|i| if i == step_n { "●" } else { "○" }).collect::<Vec<_>>().join("");
        let title = format!(" New Capability · {} ", dots);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title, theme::accent_bold()))
            .border_style(theme::border_focused());
        let inner = block.inner(dialog);
        f.render_widget(block, dialog);

        match &self.step {
            CapWizardStep::Form => self.render_form(f, inner),
            CapWizardStep::Confirm => self.render_confirm(f, inner),
        }
    }

    fn render_form(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),  // name
                Constraint::Length(3),  // description
                Constraint::Length(3),  // command
                Constraint::Length(3),  // args
                Constraint::Length(3),  // risk + side_effect
                Constraint::Length(1),  // error / hint
                Constraint::Min(0),
            ])
            .split(area);

        render_text_field(f, &self.name, "Name * (dot-namespaced, e.g. gh.pr.list)", self.active_field == 0, chunks[0]);
        render_text_field(f, &self.description, "Description", self.active_field == 1, chunks[1]);
        render_text_field(f, &self.command, "Command * (e.g. gh)", self.active_field == 2, chunks[2]);
        render_text_field(f, &self.args, "Args (space-separated, ${var} ok)", self.active_field == 3, chunks[3]);

        let sel_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[4]);
        render_select_field(f, &self.risk, "Risk", self.active_field == 4, sel_row[0]);
        render_select_field(f, &self.side_effect, "Side Effect", self.active_field == 5, sel_row[1]);

        if let Some(err) = &self.error {
            f.render_widget(Paragraph::new(Span::styled(err.as_str(), theme::danger())), chunks[5]);
        } else {
            f.render_widget(
                Paragraph::new("tab: next field   ← →: change option   enter: continue   esc: cancel")
                    .style(theme::muted()),
                chunks[5]
            );
        }
    }

    fn render_confirm(&self, f: &mut Frame, area: Rect) {
        let args_display = if self.args.value.is_empty() { "(none)".to_string() } else { self.args.value.clone() };
        let lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled("  Name        ", theme::muted()), Span::raw(self.name.value.trim())]),
            Line::from(vec![Span::styled("  Command     ", theme::muted()), Span::raw(self.command.value.trim())]),
            Line::from(vec![Span::styled("  Args        ", theme::muted()), Span::styled(args_display, theme::dim())]),
            Line::from(vec![
                Span::styled("  Risk        ", theme::muted()),
                Span::styled(self.risk.current(), Style::default().fg(theme::risk_color(self.risk.current()))),
            ]),
            Line::from(vec![
                Span::styled("  Side effect ", theme::muted()),
                Span::styled(self.side_effect.current(), Style::default().fg(theme::side_effect_color(self.side_effect.current()))),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  enter", theme::accent()),
                Span::raw(" write capability   "),
                Span::styled("esc", theme::muted()),
                Span::raw(" back"),
            ]),
        ];
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }
}

fn render_select_field(f: &mut Frame, field: &SelectField, label: &str, focused: bool, area: Rect) {
    let border_style = if focused { theme::border_focused() } else { theme::border_normal() };
    let opts: Vec<Span> = field.options.iter().enumerate().flat_map(|(i, opt)| {
        let s = if i == field.idx {
            Span::styled(format!(" {} ", opt), if focused { theme::selected() } else { theme::accent() })
        } else {
            Span::styled(format!(" {} ", opt), theme::inactive())
        };
        [s, Span::styled("|", theme::muted())]
    }).collect::<Vec<_>>();
    let mut spans = vec![Span::styled("← ", theme::muted())];
    let mut opts = opts;
    if !opts.is_empty() { opts.pop(); }  // remove trailing |
    spans.extend(opts);
    spans.push(Span::styled(" →", theme::muted()));

    let para = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL).title(label).border_style(border_style));
    f.render_widget(para, area);
}

fn validate_cap_name(name: &str) -> bool {
    if name.is_empty() { return false; }
    if !name.contains('.') { return false; }
    name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-')
}
