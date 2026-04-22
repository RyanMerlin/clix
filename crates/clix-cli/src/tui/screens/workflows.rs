use ratatui::{prelude::*, widgets::*};
use crate::tui::app::{App, Focus};
use crate::tui::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let workflows = app.workflow_registry.all();

    if workflows.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(" Workflows ", theme::accent_bold()))
            .border_style(theme::border_for(app.focus == Focus::Content));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled("  No workflows found.", theme::muted())),
            Line::from(""),
            Line::from(Span::styled(
                "  Place workflow YAML files in your packs alongside pack.yaml.",
                theme::inactive(),
            )),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    // Split into left list + right detail panel
    let chunks = Layout::horizontal([Constraint::Length(32), Constraint::Fill(1)])
        .split(area);

    // ── left: workflow list ──────────────────────────────────────────────────
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Workflows ", theme::accent_bold()))
        .border_style(theme::border_for(app.focus == Focus::Content));

    let items: Vec<ListItem> = workflows.iter().enumerate().map(|(i, wf)| {
        let style = if i == app.workflows_cursor { theme::selected() } else { theme::normal() };
        let label = format!(" {}", wf.name);
        ListItem::new(label).style(style)
    }).collect();

    let list = List::new(items).block(list_block);
    f.render_widget(list, chunks[0]);

    // ── right: detail panel ──────────────────────────────────────────────────
    let selected = workflows.get(app.workflows_cursor);
    let detail_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Steps ", theme::accent_bold()))
        .border_style(theme::border_normal());

    let Some(wf) = selected else {
        f.render_widget(detail_block, chunks[1]);
        return;
    };

    let inner = detail_block.inner(chunks[1]);
    f.render_widget(detail_block, chunks[1]);

    let mut lines: Vec<Line> = vec![Line::from("")];

    // Header
    lines.push(Line::from(vec![
        Span::styled("  ", theme::muted()),
        Span::styled(wf.name.as_str(), theme::accent_bold()),
        Span::styled(format!("  v{}", wf.version), theme::muted()),
    ]));

    if let Some(ref desc) = wf.description {
        lines.push(Line::from(Span::styled(
            format!("  {desc}"),
            theme::inactive(),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  {} step{}", wf.steps.len(), if wf.steps.len() == 1 { "" } else { "s" }),
        theme::muted(),
    )));
    lines.push(Line::from(""));

    for (i, step) in wf.steps.iter().enumerate() {
        lines.push(Line::from(vec![
            Span::styled(format!("  {:2}. ", i + 1), theme::muted()),
            Span::styled(step.capability.as_str(), theme::normal()),
        ]));
        if !step.input.is_null() && step.input != serde_json::Value::Object(Default::default()) {
            if let Ok(pretty) = serde_json::to_string(&step.input) {
                lines.push(Line::from(Span::styled(
                    format!("      {pretty}"),
                    theme::inactive(),
                )));
            }
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}
