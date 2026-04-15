use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};
use clix_core::registry::CapabilityRegistry;
use clix_core::manifest::capability::{RiskLevel, SideEffectClass};
use crate::tui::theme;
use crate::tui::widgets::checklist::{Checklist, ChecklistItem};
use crate::tui::widgets::form::FieldInput;

#[derive(Debug, Clone, PartialEq)]
pub enum ProfileWizardStep {
    Identity,   // step 0: name + description
    Capabilities, // step 1: capability checklist
    Confirm,    // step 2: summary
}

#[derive(Debug, Clone)]
pub struct ProfileWizard {
    pub step: ProfileWizardStep,
    // Step 0
    pub name: FieldInput,
    pub description: FieldInput,
    pub active_field: usize,  // 0=name, 1=description
    // Step 1
    pub checklist: Checklist,
    // Validation error
    pub error: Option<String>,
}

pub enum ProfileWizardAction {
    None,
    Cancel,
    Submit { name: String, description: String, capabilities: Vec<String> },
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
            error: None,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> ProfileWizardAction {
        match &self.step {
            ProfileWizardStep::Identity => self.handle_identity(code),
            ProfileWizardStep::Capabilities => self.handle_capabilities(code),
            ProfileWizardStep::Confirm => self.handle_confirm(code),
        }
    }

    fn handle_identity(&mut self, code: KeyCode) -> ProfileWizardAction {
        match code {
            KeyCode::Esc => return ProfileWizardAction::Cancel,
            KeyCode::Tab => {
                self.active_field = (self.active_field + 1) % 2;
            }
            KeyCode::BackTab => {
                self.active_field = if self.active_field == 0 { 1 } else { 0 };
            }
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
                if self.active_field == 0 {
                    self.name.handle_key(code);
                } else {
                    self.description.handle_key(code);
                }
            }
        }
        ProfileWizardAction::None
    }

    fn handle_capabilities(&mut self, code: KeyCode) -> ProfileWizardAction {
        match code {
            KeyCode::Esc => {
                self.step = ProfileWizardStep::Identity;
            }
            KeyCode::Enter => {
                self.step = ProfileWizardStep::Confirm;
            }
            _ => { self.checklist.handle_key(code); }
        }
        ProfileWizardAction::None
    }

    fn handle_confirm(&mut self, code: KeyCode) -> ProfileWizardAction {
        match code {
            KeyCode::Esc => {
                self.step = ProfileWizardStep::Capabilities;
            }
            KeyCode::Enter | KeyCode::Char('w') => {
                return ProfileWizardAction::Submit {
                    name: self.name.value.trim().to_string(),
                    description: self.description.value.trim().to_string(),
                    capabilities: self.checklist.selected_ids(),
                };
            }
            _ => {}
        }
        ProfileWizardAction::None
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        // Center a dialog
        let width = area.width.saturating_sub(4).max(50);
        let height = area.height.saturating_sub(2).max(12);
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let dialog = Rect::new(x, y, width, height);

        f.render_widget(Clear, dialog);

        let step_n = match self.step {
            ProfileWizardStep::Identity => 0,
            ProfileWizardStep::Capabilities => 1,
            ProfileWizardStep::Confirm => 2,
        };
        let title = format!(" New Profile · {}", step_dots(step_n, 3));
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title, theme::accent_bold()))
            .border_style(theme::border_focused());
        let inner = block.inner(dialog);
        f.render_widget(block, dialog);

        match &self.step {
            ProfileWizardStep::Identity => self.render_identity(f, inner),
            ProfileWizardStep::Capabilities => self.render_capabilities(f, inner),
            ProfileWizardStep::Confirm => self.render_confirm(f, inner),
        }
    }

    fn render_identity(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(area);

        render_text_field(f, &self.name, "Name *", self.active_field == 0, chunks[0]);
        render_text_field(f, &self.description, "Description", self.active_field == 1, chunks[1]);

        if let Some(err) = &self.error {
            let msg = Paragraph::new(Span::styled(err.as_str(), theme::danger()));
            f.render_widget(msg, chunks[2]);
        }

        let hint = Paragraph::new("tab: next field   enter: continue   esc: cancel")
            .style(theme::muted());
        f.render_widget(hint, chunks[3]);
    }

    fn render_capabilities(&self, f: &mut Frame, area: Rect) {
        if self.checklist.items.is_empty() {
            let msg = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled("  No capabilities loaded yet.", theme::muted())),
                Line::from(""),
                Line::from(vec![
                    Span::raw("  Install a pack first (press "),
                    Span::styled("3", theme::accent()),
                    Span::raw("), then create your profile."),
                ]),
                Line::from(""),
                Line::from("  enter: continue (0 caps)   esc: back"),
            ]);
            f.render_widget(msg, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let title = format!("Capabilities ({} selected)", self.checklist.selected_count());
        self.checklist.render(f, chunks[0], &title, true);

        let hint = Paragraph::new("enter: continue   esc: back")
            .style(theme::muted());
        f.render_widget(hint, chunks[1]);
    }

    fn render_confirm(&self, f: &mut Frame, area: Rect) {
        let cap_ids = self.checklist.selected_ids();
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
        for id in cap_ids.iter().take(8) {
            lines.push(Line::from(Span::styled(format!("    • {}", id), theme::dim())));
        }
        if cap_ids.len() > 8 {
            lines.push(Line::from(Span::styled(format!("    … and {} more", cap_ids.len() - 8), theme::muted())));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  enter", theme::accent()),
            Span::raw(" write profile   "),
            Span::styled("esc", theme::muted()),
            Span::raw(" back"),
        ]));

        let para = Paragraph::new(lines).wrap(Wrap { trim: false });
        f.render_widget(para, area);
    }
}

// ─── helpers ────────────────────────────────────────────────────────────────

fn step_dots(current: usize, total: usize) -> String {
    (0..total).map(|i| if i == current { "●" } else { "○" }).collect::<Vec<_>>().join("")
}

pub fn render_text_field(f: &mut Frame, field: &FieldInput, label: &str, focused: bool, area: Rect) {
    let border_style = if focused { theme::border_focused() } else { theme::border_normal() };
    let (before, after) = field.split_at_cursor();
    let cursor_char = if after.is_empty() { " " } else { &after[..after.chars().next().map(|c| c.len_utf8()).unwrap_or(1)] };
    let after_cursor = if after.is_empty() { "" } else { &after[cursor_char.len()..] };

    let content = if focused {
        Line::from(vec![
            Span::raw(before),
            Span::styled(cursor_char, Style::default().bg(theme::ACCENT_BRIGHT).fg(Color::Black)),
            Span::raw(after_cursor),
        ])
    } else {
        Line::from(Span::styled(before.to_string() + after, theme::dim()))
    };

    let para = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title(label).border_style(border_style));
    f.render_widget(para, area);
}
