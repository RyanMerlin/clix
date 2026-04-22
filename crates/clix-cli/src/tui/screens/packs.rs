use ratatui::{prelude::*, widgets::*};
use crate::tui::app::{App, Focus};
use crate::tui::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    render_list(f, app, chunks[0]);
    render_detail(f, app, chunks[1]);
}

fn render_list(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Packs ", theme::accent_bold()))
        .border_style(theme::border_for(app.focus == Focus::Content));

    if app.packs.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled("  No packs installed.", theme::muted())),
            Line::from(""),
            Line::from(vec![
                Span::raw("  Press "),
                Span::styled("n", theme::accent()),
                Span::raw(" to create or "),
                Span::styled("i", theme::accent()),
                Span::raw(" to install."),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let items: Vec<ListItem> = app.packs.iter().enumerate().map(|(i, p)| {
        let is_cursor = i == app.packs_cursor;
        let name_style = if is_cursor { theme::selected() } else { theme::normal() };
        let ver_style = theme::muted();
        ListItem::new(Line::from(vec![
            Span::styled(format!(" {:<22}", p.name), name_style),
            Span::styled(format!("v{}", p.version), ver_style),
        ]))
    }).collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(app.packs_cursor));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_detail(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Detail ", theme::accent_bold()))
        .border_style(theme::border_dim());

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(pack) = app.packs.get(app.packs_cursor) {
        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Name     ", theme::muted()),
                Span::styled(pack.name.as_str(), theme::accent()),
            ]),
            Line::from(vec![
                Span::styled("  Version  ", theme::muted()),
                Span::styled(format!("v{}", pack.version), theme::normal()),
            ]),
            Line::from(vec![
                Span::styled("  Author   ", theme::muted()),
                Span::styled(pack.author.as_deref().unwrap_or("—"), theme::dim()),
            ]),
        ];
        if let Some(desc) = &pack.description {
            lines.push(Line::from(vec![
                Span::styled("  Desc     ", theme::muted()),
                Span::styled(desc.as_str(), theme::dim()),
            ]));
        }
        lines.push(Line::from(""));

        let cap_count = pack.capabilities.len();
        lines.push(Line::from(vec![
            Span::styled(format!("  Capabilities ({}):", cap_count), theme::muted()),
        ]));
        for cap in pack.capabilities.iter().take(8) {
            lines.push(Line::from(Span::styled(format!("    · {}", cap), theme::dim())));
        }
        if cap_count > 8 {
            lines.push(Line::from(Span::styled(format!("    … {} more", cap_count - 8), theme::muted())));
        }

        if !pack.profiles.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(format!("  Profiles ({}):", pack.profiles.len()), theme::muted()),
            ]));
            for p in &pack.profiles {
                lines.push(Line::from(Span::styled(format!("    · {}", p), theme::dim())));
            }
        }

        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    } else {
        f.render_widget(
            Paragraph::new(Span::styled("  Select a pack to see details", theme::inactive())),
            inner,
        );
    }
}
