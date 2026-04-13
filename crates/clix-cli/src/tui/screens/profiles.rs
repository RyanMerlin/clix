use ratatui::{prelude::*, widgets::*};
use crate::tui::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app.profiles.iter().map(|p| {
        let marker = if app.active_profiles.contains(&p.name) { "● " } else { "○ " };
        let desc = p.description.as_deref().unwrap_or("");
        ListItem::new(format!("{}{:<20} {}", marker, p.name, desc))
    }).collect();

    let is_empty = items.is_empty();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Profiles — Enter to toggle, q to quit"))
        .highlight_style(Style::default().bg(Color::DarkGray).bold())
        .highlight_symbol("> ");

    let mut state = ListState::default();
    state.select(if is_empty { None } else { Some(app.cursor) });
    f.render_stateful_widget(list, area, &mut state);
}
