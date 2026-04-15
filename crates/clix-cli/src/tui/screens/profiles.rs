use ratatui::{prelude::*, widgets::*};
use crate::tui::app::App;
use crate::tui::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    render_list(f, app, chunks[0]);
    render_detail(f, app, chunks[1]);
}

fn render_list(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Profiles ", theme::accent_bold()))
        .border_style(theme::border_normal());

    if app.profiles.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled("  No profiles yet.", theme::muted())),
            Line::from(""),
            Line::from(vec![
                Span::raw("  Press "),
                Span::styled("n", theme::accent()),
                Span::raw(" to create your first profile."),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let items: Vec<ListItem> = app.profiles.iter().enumerate().map(|(i, p)| {
        let is_active = app.active_profiles.contains(&p.name);
        let is_cursor = i == app.profiles_cursor;
        let (bullet, bullet_style) = if is_active {
            ("●", theme::ok())
        } else {
            ("○", theme::muted())
        };

        let name_style = if is_cursor { theme::selected() } else if is_active { theme::normal() } else { theme::dim() };

        ListItem::new(Line::from(vec![
            Span::styled(format!(" {} ", bullet), bullet_style),
            Span::styled(format!("{:<20}", p.name), name_style),
        ]))
    }).collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected());

    let mut state = ListState::default();
    state.select(Some(app.profiles_cursor));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_detail(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Detail ", theme::accent_bold()))
        .border_style(theme::border_dim());

    if let Some(profile) = app.profiles.get(app.profiles_cursor) {
        let is_active = app.active_profiles.contains(&profile.name);
        let status_style = if is_active { theme::ok() } else { theme::muted() };
        let status_str = if is_active { "● active" } else { "○ inactive" };

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Name        ", theme::muted()),
                Span::styled(profile.name.as_str(), theme::normal()),
            ]),
            Line::from(vec![
                Span::styled("  Status      ", theme::muted()),
                Span::styled(status_str, status_style),
            ]),
        ];

        if let Some(desc) = &profile.description {
            lines.push(Line::from(vec![
                Span::styled("  Description ", theme::muted()),
                Span::styled(desc.as_str(), theme::dim()),
            ]));
        }

        lines.push(Line::from(""));
        let cap_count = profile.capabilities.len();
        lines.push(Line::from(vec![
            Span::styled("  Capabilities", theme::muted()),
            Span::styled(format!("  {} granted", cap_count), if cap_count > 0 { theme::normal() } else { theme::inactive() }),
        ]));
        for cap in profile.capabilities.iter().take(10) {
            lines.push(Line::from(Span::styled(format!("    · {}", cap), theme::dim())));
        }
        if cap_count > 10 {
            lines.push(Line::from(Span::styled(format!("    … {} more", cap_count - 10), theme::muted())));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  enter", theme::accent()),
            Span::raw(if is_active { " deactivate" } else { " activate" }),
        ]));

        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    } else {
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(Span::styled("  Select a profile to see details", theme::inactive())),
            inner,
        );
    }
}
