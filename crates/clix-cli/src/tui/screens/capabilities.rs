use ratatui::{prelude::*, widgets::*};
use clix_core::manifest::capability::{Backend, RiskLevel, SideEffectClass};
use crate::tui::app::{App, CapView};
use crate::tui::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    match &app.caps_view {
        CapView::Namespaces => render_namespaces(f, app, area),
        CapView::Listing(ns) => render_listing(f, app, area, &ns.clone()),
        CapView::Detail(name) => render_detail(f, app, area, &name.clone()),
    }
}

fn render_namespaces(f: &mut Frame, app: &App, area: Rect) {
    let namespaces = app.registry.namespaces();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Capabilities ", theme::accent_bold()))
        .border_style(theme::border_normal());

    if namespaces.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled("  No capabilities loaded.", theme::muted())),
            Line::from(""),
            Line::from(vec![
                Span::raw("  Press "),
                Span::styled("3", theme::accent()),
                Span::raw(" to manage packs, or "),
                Span::styled("n", theme::accent()),
                Span::raw(" to create one."),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    // Namespace list
    let items: Vec<ListItem> = namespaces.iter().enumerate().map(|(i, ns)| {
        let is_cursor = i == app.caps_cursor;
        let count_style = theme::muted();
        let name_style = if is_cursor { theme::selected() } else { theme::normal() };
        ListItem::new(Line::from(vec![
            Span::styled(format!(" {:<22}", ns.key), name_style),
            Span::styled(format!("{:>3}", ns.count), count_style),
        ]))
    }).collect();

    let ns_list = List::new(items)
        .block(block)
        .highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(app.caps_cursor));
    f.render_stateful_widget(ns_list, chunks[0], &mut state);

    // Detail: show capabilities in hovered namespace
    let detail_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Preview ", theme::muted()))
        .border_style(theme::border_dim());

    if let Some(ns) = namespaces.get(app.caps_cursor) {
        let caps = app.registry.by_namespace(&ns.key);
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(format!("  {} — {} capabilities", ns.key, ns.count), theme::accent())),
            Line::from(""),
        ];
        for cap in caps.iter().take(20) {
            let risk_col = match cap.risk {
                RiskLevel::Low => theme::OK,
                RiskLevel::Medium => theme::WARN,
                RiskLevel::High | RiskLevel::Critical => theme::DANGER,
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<36}", cap.name), theme::dim()),
                Span::styled(risk_label(&cap.risk), Style::default().fg(risk_col)),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  enter", theme::accent()),
            Span::raw(" to browse"),
        ]));
        let inner = detail_block.inner(chunks[1]);
        f.render_widget(detail_block, chunks[1]);
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    } else {
        f.render_widget(detail_block, chunks[1]);
    }
}

fn render_listing(f: &mut Frame, app: &App, area: Rect, ns: &str) {
    let caps = app.registry.by_namespace(ns);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let title = format!(" {} ({}) ", ns, caps.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, theme::accent_bold()))
        .border_style(theme::border_normal());

    let items: Vec<ListItem> = caps.iter().enumerate().map(|(i, cap)| {
        let is_cursor = i == app.caps_cursor;
        let risk_col = match cap.risk {
            RiskLevel::Low => theme::OK,
            RiskLevel::Medium => theme::WARN,
            RiskLevel::High | RiskLevel::Critical => theme::DANGER,
        };
        let name_style = if is_cursor { theme::selected() } else { theme::normal() };
        ListItem::new(Line::from(vec![
            Span::styled(format!(" {:<30}", cap.name), name_style),
            Span::styled(risk_label(&cap.risk), Style::default().fg(risk_col)),
        ]))
    }).collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(app.caps_cursor));
    f.render_stateful_widget(list, chunks[0], &mut state);

    // Detail panel
    let detail_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Detail ", theme::muted()))
        .border_style(theme::border_dim());

    if let Some(cap) = caps.get(app.caps_cursor) {
        let backend_str = match &cap.backend {
            Backend::Subprocess { command, args, .. } => {
                if args.is_empty() { command.clone() }
                else { format!("{} {}", command, args.join(" ")) }
            }
            Backend::Builtin { name } => format!("builtin:{}", name),
            Backend::Remote { url } => format!("remote:{}", url),
        };
        let risk_col = match cap.risk {
            RiskLevel::Low => theme::OK,
            RiskLevel::Medium => theme::WARN,
            RiskLevel::High | RiskLevel::Critical => theme::DANGER,
        };
        let se_col = match cap.side_effect_class {
            SideEffectClass::None => theme::TEXT_DIM,
            SideEffectClass::ReadOnly => theme::OK,
            SideEffectClass::Additive => theme::INFO,
            SideEffectClass::Mutating => theme::WARN,
            SideEffectClass::Destructive => theme::DANGER,
        };
        let lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled("  Name       ", theme::muted()), Span::styled(cap.name.as_str(), theme::normal())]),
            Line::from(vec![Span::styled("  Backend    ", theme::muted()), Span::styled(backend_str, theme::dim())]),
            Line::from(vec![
                Span::styled("  Risk       ", theme::muted()),
                Span::styled(risk_label(&cap.risk), Style::default().fg(risk_col)),
            ]),
            Line::from(vec![
                Span::styled("  Side effect", theme::muted()),
                Span::styled(format!("  {}", se_label(&cap.side_effect_class)), Style::default().fg(se_col)),
            ]),
            Line::from(vec![Span::styled("  Desc       ", theme::muted()), Span::styled(cap.description.as_deref().unwrap_or("—"), theme::dim())]),
            Line::from(""),
            Line::from(vec![Span::styled("  esc", theme::accent()), Span::raw(" back")]),
        ];
        let inner = detail_block.inner(chunks[1]);
        f.render_widget(detail_block, chunks[1]);
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    } else {
        f.render_widget(detail_block, chunks[1]);
    }
}

fn render_detail(f: &mut Frame, app: &App, area: Rect, name: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(format!(" {} ", name), theme::accent_bold()))
        .border_style(theme::border_focused());

    if let Some(cap) = app.registry.get(name) {
        let backend_str = match &cap.backend {
            Backend::Subprocess { command, args, .. } => {
                if args.is_empty() { format!("subprocess  {}", command) }
                else { format!("subprocess  {} {}", command, args.join(" ")) }
            }
            Backend::Builtin { name } => format!("builtin  {}", name),
            Backend::Remote { url } => format!("remote  {}", url),
        };
        let risk_col = match cap.risk {
            RiskLevel::Low => theme::OK,
            RiskLevel::Medium => theme::WARN,
            RiskLevel::High | RiskLevel::Critical => theme::DANGER,
        };

        let schema_str = serde_json::to_string_pretty(&cap.input_schema)
            .unwrap_or_else(|_| "(none)".into());

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled("  Name         ", theme::muted()), Span::styled(cap.name.as_str(), theme::accent())]),
            Line::from(vec![Span::styled("  Description  ", theme::muted()), Span::styled(cap.description.as_deref().unwrap_or("—"), theme::dim())]),
            Line::from(vec![Span::styled("  Backend      ", theme::muted()), Span::styled(backend_str, theme::normal())]),
            Line::from(vec![
                Span::styled("  Risk         ", theme::muted()),
                Span::styled(risk_label(&cap.risk), Style::default().fg(risk_col)),
            ]),
            Line::from(""),
            Line::from(Span::styled("  Input schema:", theme::muted())),
        ];
        for sch_line in schema_str.lines() {
            lines.push(Line::from(Span::styled(format!("    {}", sch_line), theme::dim())));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled("  esc", theme::accent()), Span::raw(" back")]));

        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    } else {
        f.render_widget(block, area);
    }
}

fn risk_label(r: &RiskLevel) -> String {
    match r {
        RiskLevel::Low => "low".into(),
        RiskLevel::Medium => "medium".into(),
        RiskLevel::High => "high".into(),
        RiskLevel::Critical => "critical".into(),
    }
}

fn se_label(s: &SideEffectClass) -> String {
    match s {
        SideEffectClass::None => "—".into(),
        SideEffectClass::ReadOnly => "read-only".into(),
        SideEffectClass::Additive => "additive".into(),
        SideEffectClass::Mutating => "mutating".into(),
        SideEffectClass::Destructive => "destructive".into(),
    }
}
