use std::io;
use std::time::Duration;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use anyhow::Result;

use crate::tui::app::{App, Screen, Overlay};

pub mod app;
pub mod screens;
pub mod theme;
pub mod widgets;

pub fn run() -> Result<()> {
    // Suppress loader warnings for the duration of the TUI session —
    // eprintln! in alternate-screen mode corrupts the display.
    clix_core::TUI_MODE.store(true, std::sync::atomic::Ordering::Relaxed);

    let mut app = App::new()?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        orig_hook(info);
    }));

    let result = run_loop(&mut terminal, &mut app);

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        app.tick();
        terminal.draw(|f| render(f, app))?;
        // Poll with 250ms timeout so toasts can auto-dismiss
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key);
                }
            }
        }
        if app.should_quit { break; }
    }
    Ok(())
}

fn render(f: &mut Frame, app: &App) {
    let full = f.area();

    // Three vertical bands: header (1), body (min), legend (1)
    let bands = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(full);

    render_header(f, app, bands[0]);

    // Body: sidebar (16) + content — 16 fits "Capabilities" with ▸ prefix
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(16), Constraint::Min(0)])
        .split(bands[1]);

    render_sidebar(f, app, body[0]);
    render_content(f, app, body[1]);

    render_legend(f, app, bands[2]);

    // Overlays (on top of everything)
    render_overlay(f, app, full);
}

// ─── header ──────────────────────────────────────────────────────────────────

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let active = if app.active_profiles.is_empty() {
        "no profile".to_string()
    } else {
        app.active_profiles.join(", ")
    };

    let left = Span::styled(" clix ", theme::accent_bold());
    let spacer = Span::styled("─".repeat(area.width.saturating_sub(12 + active.len() as u16 + 2) as usize), theme::muted());
    let right = Span::styled(format!(" {} ", active), theme::dim());

    let line = Line::from(vec![left, spacer, right]);
    f.render_widget(Paragraph::new(line), area);
}

// ─── sidebar ─────────────────────────────────────────────────────────────────

static SIDEBAR_ITEMS: &[(&str, &str)] = &[
    ("0", "Dashboard"),
    ("1", "Profiles"),
    ("2", "Capabilities"),
    ("3", "Packs"),
    ("4", "Receipts"),
    ("5", "Workflows"),
    ("6", "Broker"),
    ("7", "Secrets"),
];

static STUB_SCREENS: &[Screen] = &[Screen::Receipts, Screen::Workflows];

fn render_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let active_idx = app.screen.sidebar_index();

    let items: Vec<ListItem> = SIDEBAR_ITEMS.iter().enumerate().map(|(i, (key, label))| {
        let is_active = i == active_idx;
        let is_stub = STUB_SCREENS.contains(&Screen::from_index(i));

        if is_active {
            ListItem::new(Line::from(vec![
                Span::styled("▸ ", theme::accent()),
                Span::styled(*label, theme::accent_bold()),
            ]))
        } else if is_stub {
            ListItem::new(Line::from(vec![
                Span::styled("  ", theme::inactive()),
                Span::styled(*label, theme::inactive()),
            ]))
        } else {
            ListItem::new(Line::from(vec![
                Span::styled("  ", theme::muted()),
                Span::styled(*label, theme::dim()),
            ]))
        }
    }).collect();

    let list = List::new(items)
        .block(Block::default()
            .borders(Borders::RIGHT)
            .border_style(theme::border_dim()));
    f.render_widget(list, area);
}

// ─── content ─────────────────────────────────────────────────────────────────

fn render_content(f: &mut Frame, app: &App, area: Rect) {
    match app.screen {
        Screen::Dashboard => screens::dashboard::render(f, app, area),
        Screen::Profiles => screens::profiles::render(f, app, area),
        Screen::Capabilities => screens::capabilities::render(f, app, area),
        Screen::Packs => screens::packs::render(f, app, area),
        Screen::Secrets => screens::secrets::render(f, app, area),
        Screen::Broker => screens::broker::render(f, app, area),
        _ => render_stub(f, app, area),
    }
}

fn render_stub(f: &mut Frame, app: &App, area: Rect) {
    let name = match app.screen {
        Screen::Receipts => "Receipts",
        Screen::Workflows => "Workflows",
        Screen::Broker => "Broker",
        _ => "",
    };
    let experimental = std::env::var("CLIX_TUI_EXPERIMENTAL").is_ok();
    let lines = if experimental {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {} — experimental preview (CLIX_TUI_EXPERIMENTAL=1)", name),
                theme::muted(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  This screen is incomplete. Data shown may be incorrect.",
                theme::inactive(),
            )),
        ]
    } else {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {} — not available in this release", name),
                theme::muted(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Use the CLI: clix receipts list --json",
                theme::inactive(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Set CLIX_TUI_EXPERIMENTAL=1 to access the in-progress preview.",
                theme::dim(),
            )),
        ]
    };
    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).border_style(theme::border_dim()));
    f.render_widget(para, area);
}

// ─── legend ───────────────────────────────────────────────────────────────────

fn render_legend(f: &mut Frame, app: &App, area: Rect) {
    let has_overlay = matches!(app.overlay, Overlay::ProfileCreate(_) | Overlay::ProfileSecrets(_) | Overlay::CapabilityCreate(_) | Overlay::PackCreate(_) | Overlay::PackEdit { .. } | Overlay::InstallPack(_) | Overlay::Help | Overlay::InfisicalSetup(_));
    if has_overlay { return; }

    let hints: Vec<Span> = match app.screen {
        Screen::Profiles => legend_spans(&[
            ("↑↓", "move"), ("enter", "toggle"), ("s", "secrets"), ("n", "new"), ("tab", "next screen"), ("?", "help"), ("q", "quit"),
        ]),
        Screen::Capabilities => legend_spans(&[
            ("↑↓", "move"), ("enter", "drill in"), ("esc", "back"), ("n", "new"), ("tab", "next screen"), ("q", "quit"),
        ]),
        Screen::Packs => legend_spans(&[
            ("↑↓", "move"), ("n", "new pack"), ("e", "edit caps"), ("i", "install"), ("tab", "next screen"), ("q", "quit"),
        ]),
        Screen::Broker => legend_spans(&[
            ("r", "refresh"), ("s", "start"), ("x", "stop"), ("0-7", "switch"), ("q", "quit"),
        ]),
        Screen::Dashboard => legend_spans(&[
            ("0-7", "switch"), ("tab", "next"), ("c", "config"), ("n", "new"), ("r", "reload"), ("?", "help"), ("q", "quit"),
        ]),
        Screen::Secrets => legend_spans(&[
            ("e", "edit"), ("t", "test"), ("b", "browse"), ("r", "reset"), ("?", "help"), ("q", "quit"),
        ]),
        _ => legend_spans(&[
            ("0-7", "switch"), ("tab", "next"), ("n", "new"), ("r", "reload"), ("?", "help"), ("q", "quit"),
        ]),
    };

    f.render_widget(Paragraph::new(Line::from(hints)).style(theme::muted()), area);
}

fn legend_spans(pairs: &[(&str, &str)]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    spans.push(Span::raw(" "));
    for (i, (key, desc)) in pairs.iter().enumerate() {
        if i > 0 { spans.push(Span::styled("  ", theme::inactive())); }
        spans.push(Span::styled(key.to_string(), theme::accent()));
        spans.push(Span::styled(format!(":{}", desc), theme::muted()));
    }
    spans
}

// ─── overlay rendering ────────────────────────────────────────────────────────

fn render_overlay(f: &mut Frame, app: &App, area: Rect) {
    match &app.overlay {
        Overlay::None => {}
        Overlay::Toast { message, is_error, .. } => render_toast(f, message, *is_error, area),
        Overlay::Help => render_help(f, area),
        Overlay::ProfileCreate(wiz) => wiz.render(f, area),
        Overlay::ProfileSecrets(state) => state.render(f, area),
        Overlay::CapabilityCreate(wiz) => wiz.render(f, area),
        Overlay::PackCreate(wiz) => wiz.render(f, area),
        Overlay::PackEdit { pack_name, checklist } => render_pack_edit(f, pack_name, checklist, area),
        Overlay::InstallPack(buf) => render_install_pack(f, buf, area),
        Overlay::InfisicalSetup(state) => state.render(f, area),
    }
}

fn render_toast(f: &mut Frame, message: &str, is_error: bool, area: Rect) {
    let style = if is_error { theme::danger() } else { theme::ok() };
    let icon = if is_error { "✗ " } else { "✓ " };
    let text = format!("{}{}", icon, message);
    let width = (text.len() as u16 + 4).min(area.width);
    let height = 1u16;
    let x = area.x + area.width.saturating_sub(width);
    let y = area.y + area.height.saturating_sub(2);
    let toast_area = Rect::new(x, y, width, height);
    f.render_widget(
        Paragraph::new(Span::styled(format!(" {} ", text), style)),
        toast_area
    );
}

fn render_help(f: &mut Frame, area: Rect) {
    let width = 50u16.min(area.width);
    let height = 22u16.min(area.height);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let dialog = Rect::new(x, y, width, height);

    f.render_widget(Clear, dialog);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Keymap ", theme::accent_bold()))
        .border_style(theme::border_focused());
    let inner = block.inner(dialog);
    f.render_widget(block, dialog);

    let lines = vec![
        Line::from(""),
        help_line("0-6 / tab", "switch screen"),
        help_line("↑ / ↓", "move cursor"),
        help_line("enter", "confirm / drill in"),
        help_line("esc", "back"),
        help_line("n", "new (create wizard)"),
        help_line("i", "install pack"),
        help_line("r", "reload all"),
        Line::from(""),
        Line::from(Span::styled("  Profiles", theme::accent())),
        help_line("enter", "toggle active"),
        Line::from(""),
        Line::from(Span::styled("  Capabilities", theme::accent())),
        help_line("enter", "drill in / detail"),
        help_line("esc", "navigate back"),
        Line::from(""),
        Line::from(Span::styled("  Wizards", theme::accent())),
        help_line("tab / shift-tab", "next / prev field"),
        help_line("← →", "cycle options"),
        help_line("space", "toggle in checklist"),
        help_line("/ then text", "filter list"),
        Line::from(""),
        Line::from(Span::styled("  any key to close this", theme::muted())),
    ];
    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, inner);
}

fn help_line(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:16}", key), theme::accent()),
        Span::styled(desc.to_string(), theme::dim()),
    ])
}

fn render_install_pack(f: &mut Frame, buf: &str, area: Rect) {
    let width = 60u16.min(area.width);
    let height = 5u16;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let dialog = Rect::new(x, y, width, height);

    f.render_widget(Clear, dialog);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Install Pack ", theme::accent_bold()))
        .border_style(theme::border_focused());
    let inner = block.inner(dialog);
    f.render_widget(block, dialog);

    let display = format!("{}_", buf);
    let para = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(display, theme::normal())),
        Line::from(""),
        Line::from(Span::styled("enter: install   esc: cancel", theme::muted())),
    ]);
    f.render_widget(para, inner);
}

fn render_pack_edit(f: &mut Frame, pack_name: &str, checklist: &crate::tui::widgets::checklist::Checklist, area: Rect) {
    let width = area.width.saturating_sub(4).max(55);
    let height = area.height.saturating_sub(2).max(14);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let dialog = Rect::new(x, y, width, height);

    f.render_widget(Clear, dialog);

    let title = format!(" Edit Pack · {} ", pack_name);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, theme::accent_bold()))
        .border_style(theme::border_focused());
    let inner = block.inner(dialog);
    f.render_widget(block, dialog);

    let title = format!("Capabilities ({} selected)", checklist.selected_count());
    checklist.render_with_hint(f, inner, &title, true, "enter:save  esc:cancel");
}
