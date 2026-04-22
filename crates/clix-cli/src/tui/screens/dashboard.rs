use ratatui::{prelude::*, widgets::*};
use crate::tui::app::{App, Focus};
use crate::tui::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let pending = app.pending_approval_ids.len();
    let (main_area, banner_area) = if pending > 0 {
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        (chunks[1], Some(chunks[0]))
    } else {
        (area, None)
    };

    if let Some(banner) = banner_area {
        let first_id = app.pending_approval_ids.first().map(|s| s.as_str()).unwrap_or("");
        let short_id = &first_id[..first_id.len().min(8)];
        let msg = format!(
            " ⚠ {pending} pending approval{} — run `clix approve {short_id}` or press A on Receipts screen ",
            if pending == 1 { "" } else { "s" }
        );
        let banner_widget = Paragraph::new(Line::from(Span::styled(msg, theme::warn())))
            .style(Style::default().bg(ratatui::style::Color::DarkGray));
        f.render_widget(banner_widget, banner);
    }

    let chunks = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(main_area);

    render_activity(f, app, chunks[0]);
    render_health(f, app, chunks[1]);
}

fn render_activity(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Recent Activity ", theme::accent_bold()))
        .border_style(theme::border_for(app.focus == Focus::Content));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.receipts_preview.is_empty() {
        // Onboarding state
        render_onboarding(f, app, inner);
    } else {
        let rows: Vec<Row> = app.receipts_preview.iter().map(|r| {
            let (icon, icon_style) = match r.outcome.as_str() {
                "allowed" => ("✓", theme::ok()),
                "denied"  => ("✗", theme::danger()),
                _         => ("⚠", theme::warn()),
            };
            Row::new(vec![
                Cell::from(Span::styled(icon, icon_style)),
                Cell::from(Span::styled(r.time.clone(), theme::muted())),
                Cell::from(Span::styled(r.capability.clone(), theme::normal())),
                Cell::from(Span::styled(r.profile.clone(), theme::dim())),
                Cell::from(Span::styled(r.latency.clone(), theme::muted())),
            ])
        }).collect();

        let table = Table::new(rows, [
            Constraint::Length(2),
            Constraint::Length(6),
            Constraint::Min(22),
            Constraint::Length(12),
            Constraint::Length(8),
        ])
        .header(Row::new(["", "time", "capability", "profile", "ms"])
            .style(theme::muted()))
        .column_spacing(1);

        f.render_widget(table, inner);
    }
}

fn render_onboarding(f: &mut Frame, app: &App, area: Rect) {
    let has_packs = !app.packs.is_empty();
    let has_profiles = !app.profiles.is_empty();
    let has_caps = app.registry.namespaces().len() > 0;

    let mut lines = vec![
        Line::from(Span::styled("  Welcome to clix", Style::default().fg(theme::ACCENT_BRIGHT).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled("  The agent gateway is ready. Get started:", theme::dim())),
        Line::from(""),
    ];

    let step_style = |done: bool| -> Style {
        if done { theme::ok() } else { theme::muted() }
    };
    let check = |done: bool| -> &'static str { if done { "✓" } else { "○" } };

    lines.push(Line::from(vec![
        Span::styled(format!("  {}  ", check(has_packs)), step_style(has_packs)),
        Span::raw("Press "),
        Span::styled("3", theme::accent()),
        Span::raw(" → "),
        Span::styled("n", theme::accent()),
        Span::raw(" to create your first pack"),
    ]));
    lines.push(Line::from(vec![
        Span::styled(format!("  {}  ", check(has_caps)), step_style(has_caps)),
        Span::raw("Capabilities will be auto-discovered"),
    ]));
    lines.push(Line::from(vec![
        Span::styled(format!("  {}  ", check(has_profiles)), step_style(has_profiles)),
        Span::raw("Press "),
        Span::styled("1", theme::accent()),
        Span::raw(" → "),
        Span::styled("n", theme::accent()),
        Span::raw(" to create a profile"),
    ]));
    let has_infisical = app.infisical_cfg.is_some();
    lines.push(Line::from(vec![
        Span::styled(format!("  {}  ", check(has_infisical)), step_style(has_infisical)),
        Span::raw("Press "),
        Span::styled("c", theme::accent()),
        Span::raw(" to configure Infisical secrets"),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  tip  ", theme::info()),
        Span::raw("Press "),
        Span::styled("?", theme::accent()),
        Span::raw(" for the full keymap"),
    ]));

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, area);
}

fn render_health(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Health ", theme::accent_bold()))
        .border_style(theme::border_for(app.focus == Focus::Content));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let cap_count: usize = app.registry.namespaces().iter().map(|ns| ns.count).sum();

    let dot = |ok: bool| -> Span<'static> {
        if ok {
            Span::styled("●", theme::ok())
        } else {
            Span::styled("●", theme::warn())
        }
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  packs        ", theme::muted()),
            dot(true),
            Span::styled(format!("  {} installed", app.packs.len()), theme::normal()),
        ]),
        Line::from(vec![
            Span::styled("  capabilities ", theme::muted()),
            dot(cap_count > 0),
            Span::styled(format!("  {} loaded", cap_count), theme::normal()),
        ]),
        Line::from(vec![
            Span::styled("  profiles     ", theme::muted()),
            dot(!app.profiles.is_empty()),
            Span::styled(format!("  {} defined", app.profiles.len()), theme::normal()),
        ]),
        Line::from(vec![
            Span::styled("  active       ", theme::muted()),
            dot(!app.active_profiles.is_empty()),
            Span::styled(
                format!("  {}", if app.active_profiles.is_empty() {
                    "none".to_string()
                } else {
                    app.active_profiles.join(", ")
                }),
                if app.active_profiles.is_empty() { theme::warn() } else { theme::ok() }
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  isolation    ", theme::muted()),
            dot(true),
            Span::styled("  warm_worker", theme::normal()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  clix home    ", theme::muted()),
            Span::styled(
                clix_core::state::home_dir()
                    .to_string_lossy()
                    .to_string(),
                theme::dim()
            ),
        ]),
    ];

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, inner);
}
