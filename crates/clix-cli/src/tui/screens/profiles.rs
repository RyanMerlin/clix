use ratatui::{prelude::*, widgets::*};
use crate::tui::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app.profiles.iter().map(|p| {
        let marker = if app.active_profiles.contains(&p.name) { "● " } else { "○ " };
        let desc = p.description.as_deref().unwrap_or("");
        ListItem::new(format!("{}{:<20} {}", marker, p.name, desc))
    }).collect();

    if items.is_empty() {
        let msg = Paragraph::new("No profiles installed — press n to create one")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(msg, area);
        return;
    }

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Profiles — Enter to toggle, q to quit"))
        .highlight_style(Style::default().bg(Color::DarkGray).bold())
        .highlight_symbol("> ");

    let mut state = ListState::default();
    state.select(Some(app.cursor));
    f.render_stateful_widget(list, area, &mut state);
}
