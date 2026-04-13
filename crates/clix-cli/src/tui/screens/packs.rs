use ratatui::{prelude::*, widgets::*};
use crate::tui::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    // Left: pack list
    let items: Vec<ListItem> = app.packs.iter().map(|p| {
        ListItem::new(format!("{} v{}", p.name, p.version))
    }).collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Packs"))
        .highlight_style(Style::default().bg(Color::DarkGray).bold())
        .highlight_symbol("> ");
    let mut state = ListState::default();
    state.select(Some(app.cursor));
    f.render_stateful_widget(list, chunks[0], &mut state);

    // Right: detail panel
    if let Some(pack) = app.packs.get(app.cursor) {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Name:    ", Style::default().bold()),
                Span::raw(pack.name.as_str()),
            ]),
            Line::from(vec![
                Span::styled("Version: ", Style::default().bold()),
                Span::raw(pack.version.to_string()),
            ]),
            Line::from(vec![
                Span::styled("Author:  ", Style::default().bold()),
                Span::raw(pack.author.as_deref().unwrap_or("(none)")),
            ]),
            Line::from(vec![
                Span::styled("Desc:    ", Style::default().bold()),
                Span::raw(pack.description.as_deref().unwrap_or("(none)")),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                format!("Capabilities ({}):", pack.capabilities.len()),
                Style::default().bold(),
            )),
        ];
        for cap in &pack.capabilities {
            lines.push(Line::from(format!("  • {}", cap)));
        }
        if !pack.profiles.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Profiles ({}):", pack.profiles.len()),
                Style::default().bold(),
            )));
            for profile in &pack.profiles {
                lines.push(Line::from(format!("  • {}", profile)));
            }
        }
        let para = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Detail"))
            .wrap(Wrap { trim: false });
        f.render_widget(para, chunks[1]);
    } else {
        let para = Paragraph::new("No packs loaded")
            .block(Block::default().borders(Borders::ALL).title("Detail"));
        f.render_widget(para, chunks[1]);
    }
}
