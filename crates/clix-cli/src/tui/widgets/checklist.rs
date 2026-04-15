use crossterm::event::KeyCode;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use crate::tui::theme;

#[derive(Debug, Clone)]
pub struct ChecklistItem {
    pub id: String,       // unique key
    pub label: String,    // primary text (left column)
    pub detail: String,   // secondary text (middle column)
    pub tag: String,      // tertiary tag (right, e.g. risk level or pack name)
    pub tag_color: Color,
    pub selected: bool,
    pub group: String,    // grouping key (e.g. pack name) — empty = no group
}

impl ChecklistItem {
    pub fn new(id: &str, label: &str, detail: &str, tag: &str, tag_color: Color, group: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            detail: detail.to_string(),
            tag: tag.to_string(),
            tag_color,
            selected: false,
            group: group.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Checklist {
    pub items: Vec<ChecklistItem>,
    pub cursor: usize,
    pub filter: String,
    pub filter_mode: bool,
}

impl Checklist {
    pub fn new(items: Vec<ChecklistItem>) -> Self {
        Self { items, cursor: 0, filter: String::new(), filter_mode: false }
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        let filter = self.filter.to_lowercase();
        self.items.iter().enumerate()
            .filter(|(_, item)| {
                if filter.is_empty() { return true; }
                item.label.to_lowercase().contains(&filter)
                    || item.detail.to_lowercase().contains(&filter)
                    || item.group.to_lowercase().contains(&filter)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_count(&self) -> usize {
        self.items.iter().filter(|i| i.selected).count()
    }

    pub fn selected_ids(&self) -> Vec<String> {
        self.items.iter().filter(|i| i.selected).map(|i| i.id.clone()).collect()
    }

    pub fn toggle_current(&mut self) {
        let visible = self.visible_indices();
        if let Some(&real_idx) = visible.get(self.cursor) {
            self.items[real_idx].selected = !self.items[real_idx].selected;
        }
    }

    pub fn select_all_visible(&mut self) {
        for &idx in &self.visible_indices() {
            self.items[idx].selected = true;
        }
    }

    pub fn deselect_all_visible(&mut self) {
        for &idx in &self.visible_indices() {
            self.items[idx].selected = false;
        }
    }

    /// Returns true if key was consumed; false if caller should handle (e.g. Enter/Esc with empty filter).
    pub fn handle_key(&mut self, code: KeyCode) -> bool {
        if self.filter_mode {
            match code {
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.cursor = 0;
                    return true;
                }
                KeyCode::Backspace => {
                    if self.filter.pop().is_none() {
                        // Empty filter — exit filter mode
                        self.filter_mode = false;
                    }
                    self.cursor = 0;
                    return true;
                }
                KeyCode::Esc | KeyCode::Enter => {
                    self.filter_mode = false;
                    return true;
                }
                _ => {}
            }
        }

        match code {
            KeyCode::Char('/') => {
                self.filter_mode = true;
                return true;
            }
            KeyCode::Down => {
                let len = self.visible_indices().len();
                if len > 0 { self.cursor = (self.cursor + 1).min(len - 1); }
                return true;
            }
            KeyCode::Up => {
                if self.cursor > 0 { self.cursor -= 1; }
                return true;
            }
            KeyCode::Char(' ') => {
                self.toggle_current();
                return true;
            }
            KeyCode::Char('a') => {
                self.select_all_visible();
                return true;
            }
            KeyCode::Char('A') => {
                self.deselect_all_visible();
                return true;
            }
            _ => {}
        }
        false
    }

    pub fn render(&self, f: &mut Frame, area: Rect, title: &str, focused: bool) {
        let border_style = if focused { theme::border_focused() } else { theme::border_normal() };
        let visible = self.visible_indices();
        let count = self.selected_count();
        let total = visible.len();

        let full_title = if self.filter_mode {
            format!(" {} · filter: {}▌ ", title, self.filter)
        } else if !self.filter.is_empty() {
            format!(" {} · /{} ", title, self.filter)
        } else {
            format!(" {} ", title)
        };

        // Divide available width: 2 check + label (40%) + detail (fills) + tag (12)
        let usable = area.width.saturating_sub(4) as usize;  // minus borders + check
        let label_width = (usable * 38 / 100).max(24).min(48);
        let tag_width = 12usize;
        let detail_width = usable.saturating_sub(label_width + tag_width + 4).max(20);

        let items: Vec<ListItem> = visible.iter().enumerate().map(|(display_idx, &real_idx)| {
            let item = &self.items[real_idx];
            let check = if item.selected { "■" } else { "□" };
            let check_style = if item.selected { theme::accent_bold() } else { theme::muted() };

            let label_style = if display_idx == self.cursor {
                theme::selected()
            } else {
                theme::normal()
            };

            let label_padded = format!("{:<width$}", item.label, width = label_width);
            let detail_padded = format!("{:<width$}", item.detail, width = detail_width);

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", check), check_style),
                Span::styled(label_padded, label_style),
                Span::styled(format!("  {}", detail_padded), theme::dim()),
                Span::styled(format!("  {}", item.tag), Style::default().fg(item.tag_color)),
            ]))
        }).collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(full_title)
                    .border_style(border_style)
            )
            .highlight_style(theme::selected());

        let mut state = ListState::default();
        state.select(if total > 0 { Some(self.cursor.min(total - 1)) } else { None });
        f.render_stateful_widget(list, area, &mut state);

        // Status line overlay at bottom of area
        let status_area = Rect::new(area.x + 2, area.y + area.height - 1, area.width.saturating_sub(4), 1);
        let status = if self.filter_mode {
            Paragraph::new("esc: exit filter  enter: confirm")
                .style(theme::muted())
        } else {
            Paragraph::new(format!("{} of {} selected  ·  space:toggle  /:filter  a:all  A:none",
                count, self.items.len()))
                .style(theme::muted())
        };
        f.render_widget(status, status_area);
    }
}
