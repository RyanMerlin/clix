use ratatui::{prelude::*, widgets::*};
use clix_core::manifest::capability::{Backend, RiskLevel};
use crate::tui::app::{App, CapView};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    match &app.cap_view {
        CapView::Namespaces => render_namespaces(f, app, area),
        CapView::Listing(ns) => render_listing(f, app, area, &ns.clone()),
        CapView::Detail(name) => render_detail(f, app, area, &name.clone()),
    }
}

fn render_namespaces(f: &mut Frame, app: &App, area: Rect) {
    let namespaces = app.registry.namespaces();
    let items: Vec<ListItem> = namespaces.iter().map(|ns| {
        ListItem::new(format!("{:<30} ({} capabilities)", ns.key, ns.count))
    }).collect();
    let is_empty = items.is_empty();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Namespaces — Enter to drill in, Esc to quit"))
        .highlight_style(Style::default().bg(Color::DarkGray).bold())
        .highlight_symbol("> ");
    let mut state = ListState::default();
    state.select(if is_empty { None } else { Some(app.cursor) });
    f.render_stateful_widget(list, area, &mut state);
}

fn render_listing(f: &mut Frame, app: &App, area: Rect, ns: &str) {
    let caps = app.registry.by_namespace(ns);
    let items: Vec<ListItem> = caps.iter().map(|cap| {
        let desc = cap.description.as_deref().unwrap_or("");
        ListItem::new(format!("{:<40} {}", cap.name, desc))
    }).collect();
    let is_empty = items.is_empty();
    let title = format!("{} — {} capabilities | Enter for detail, Esc to go back", ns, caps.len());
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray).bold())
        .highlight_symbol("> ");
    let mut state = ListState::default();
    state.select(if is_empty { None } else { Some(app.cursor) });
    f.render_stateful_widget(list, area, &mut state);
}

fn render_detail(f: &mut Frame, app: &App, area: Rect, name: &str) {
    if let Some(cap) = app.registry.get(name) {
        let backend_str = match &cap.backend {
            Backend::Subprocess { command, args, .. } => {
                if args.is_empty() {
                    format!("subprocess: {}", command)
                } else {
                    format!("subprocess: {} {}", command, args.join(" "))
                }
            }
            Backend::Builtin { name } => format!("builtin: {}", name),
            Backend::Remote { url } => format!("remote: {}", url),
        };
        let risk_str = match cap.risk {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
        };
        let lines = vec![
            Line::from(vec![
                Span::styled("Name:    ", Style::default().bold()),
                Span::raw(cap.name.as_str()),
            ]),
            Line::from(vec![
                Span::styled("Desc:    ", Style::default().bold()),
                Span::raw(cap.description.as_deref().unwrap_or("(none)")),
            ]),
            Line::from(vec![
                Span::styled("Backend: ", Style::default().bold()),
                Span::raw(backend_str),
            ]),
            Line::from(vec![
                Span::styled("Risk:    ", Style::default().bold()),
                Span::raw(risk_str),
            ]),
        ];
        let para = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(format!("{} | Esc to go back", name)))
            .wrap(Wrap { trim: false });
        f.render_widget(para, area);
    }
}
