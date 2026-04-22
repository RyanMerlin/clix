use ratatui::{prelude::*, widgets::*};
use crate::tui::app::{App, Focus};
use crate::tui::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Receipts ", theme::accent_bold()))
        .border_style(theme::border_for(app.focus == Focus::Content));

    if app.receipts_preview.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled("  No receipts yet.", theme::muted())),
            Line::from(""),
            Line::from(Span::styled("  Receipts are written each time a capability runs.", theme::inactive())),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let header_style = theme::accent_bold();
    let header = Row::new(vec![
        Cell::from("Time").style(header_style),
        Cell::from("Capability").style(header_style),
        Cell::from("Profile").style(header_style),
        Cell::from("").style(header_style),  // outcome icon
    ]);

    let rows: Vec<Row> = app.receipts_preview.iter().enumerate().map(|(i, r)| {
        let is_cursor = i == app.receipts_cursor;
        let outcome_style = match r.outcome.as_str() {
            "✓" => theme::ok(),
            "✗" => theme::danger(),
            "⊘" => theme::warn(),
            "…" => theme::info(),
            _ => theme::muted(),
        };
        let row_style = if is_cursor { theme::selected() } else { theme::normal() };
        Row::new(vec![
            Cell::from(r.time.as_str()).style(row_style),
            Cell::from(r.capability.as_str()).style(row_style),
            Cell::from(r.profile.as_str()).style(row_style),
            Cell::from(r.outcome.as_str()).style(outcome_style),
        ])
    }).collect();

    let widths = [
        Constraint::Length(15),
        Constraint::Fill(1),
        Constraint::Length(18),
        Constraint::Length(3),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .row_highlight_style(theme::selected());

    let mut state = TableState::default();
    state.select(Some(app.receipts_cursor));
    f.render_stateful_widget(table, area, &mut state);
}
