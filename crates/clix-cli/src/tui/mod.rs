use std::io;
use std::time::Duration;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use anyhow::Result;

use crate::tui::app::{App, Screen, Overlay, Focus};

pub mod app;
pub mod screens;
pub mod theme;
pub mod widgets;
pub mod work;

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

    // Confirm-discard dialog floats above the overlay
    if app.confirming_discard {
        render_confirm_discard(f, full);
    }

    // Toast floats above all other layers
    if let Some(ref t) = app.toast_state {
        render_toast(f, &t.message, t.is_error, full);
    }
}

// ─── header ──────────────────────────────────────────────────────────────────

fn breadcrumb(app: &App) -> String {
    let screen_name = match app.screen {
        Screen::Dashboard => "Dashboard",
        Screen::Profiles => "Profiles",
        Screen::Capabilities => "Capabilities",
        Screen::Packs => "Packs",
        Screen::Receipts => "Receipts",
        Screen::Workflows => "Workflows",
        Screen::Broker => "Broker",
        Screen::Secrets => "Secrets",
    };
    let overlay_name = match &app.overlay {
        Overlay::ProfileCreate(_) => Some("New Profile"),
        Overlay::ProfileSecrets(_) => Some("Edit Secrets"),
        Overlay::CapabilityCreate(_) => Some("New Capability"),
        Overlay::PackCreate(_) => Some("New Pack"),
        Overlay::PackEdit { .. } => Some("Edit Pack"),
        Overlay::InstallPack(_) => Some("Install Pack"),
        Overlay::Help => Some("Help"),
        Overlay::InfisicalSetup(_) => Some("Configure Infisical"),
        Overlay::SecretsTreeBrowser(_) => Some("Browse Secrets"),
        Overlay::InfisicalAccounts(_) => Some("Infisical Accounts"),
        Overlay::None => None,
    };
    match overlay_name {
        Some(ov) => format!("{} › {}", screen_name, ov),
        None => screen_name.to_string(),
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let active = if app.active_profiles.is_empty() {
        "no profile".to_string()
    } else {
        app.active_profiles.join(", ")
    };

    // Git badge: binary — either "out of sync" (amber) or hidden (clean)
    let git_badge: Option<(String, ratatui::style::Style)> = app.git_badge.as_ref().and_then(|b| {
        if !b.configured { return None; }
        if app.git_syncing {
            return Some((" ⟳ syncing ".to_string(), theme::dim()));
        }
        if b.dirty > 0 || b.ahead > 0 || b.behind > 0 {
            Some((" ● out of sync ".to_string(), theme::warn()))
        } else {
            None // synced — no badge (no noise when everything is fine)
        }
    });

    let crumb = breadcrumb(app);
    let left = Span::styled(" clix ", theme::accent_bold());
    let sep = Span::styled(" › ", theme::muted());
    let crumb_span = Span::styled(crumb.clone(), if app.focus == Focus::Content { theme::dim() } else { theme::muted() });

    let right_profile = format!(" {} ", active);
    let badge_len = git_badge.as_ref().map(|(s, _)| s.len() as u16).unwrap_or(0);
    let used = 7 + 3 + crumb.len() as u16 + right_profile.len() as u16 + badge_len;
    let pad = " ".repeat(area.width.saturating_sub(used) as usize);

    let mut spans = vec![left, sep, crumb_span, Span::raw(pad)];
    if let Some((label, style)) = git_badge {
        spans.push(Span::styled(label, style));
    }
    spans.push(Span::styled(right_profile, theme::dim()));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
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

fn render_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let active_idx = app.screen.sidebar_index();
    let sidebar_focused = app.focus == Focus::Sidebar && !app.overlay.is_open();

    let items: Vec<ListItem> = SIDEBAR_ITEMS.iter().enumerate().map(|(i, (_key, label))| {
        let is_active = i == active_idx;

        if is_active && sidebar_focused {
            ListItem::new(Line::from(vec![
                Span::styled("▶ ", theme::accent()),
                Span::styled(*label, theme::accent_bold()),
            ]))
        } else if is_active {
            ListItem::new(Line::from(vec![
                Span::styled("▸ ", theme::muted()),
                Span::styled(*label, theme::dim()),
            ]))
        } else {
            ListItem::new(Line::from(vec![
                Span::styled("  ", theme::muted()),
                Span::styled(*label, theme::dim()),
            ]))
        }
    }).collect();

    let border_style = if sidebar_focused { theme::border_focused() } else { theme::border_dim() };
    let list = List::new(items)
        .block(Block::default()
            .borders(Borders::RIGHT)
            .border_style(border_style));
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
        Screen::Receipts => screens::receipts::render(f, app, area),
        Screen::Workflows => screens::workflows::render(f, app, area),
    }
}

// ─── legend ───────────────────────────────────────────────────────────────────

fn render_legend(f: &mut Frame, app: &App, area: Rect) {
    let has_overlay = app.overlay.is_open();
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
            ("0-7", "switch"), ("tab", "next"), ("n", "new"), ("r", "reload"),
            ("^G", "sync"), ("?", "help"), ("q", "quit"),
        ]),
        Screen::Secrets => legend_spans(&[
            ("m", "accounts"), ("e", "edit"), ("t", "test"), ("b", "browse"),
            ("^G", "sync"), ("?", "help"), ("q", "quit"),
        ]),
        Screen::Receipts => legend_spans(&[
            ("↑↓", "move"), ("A", "approve pending"), ("r", "reload"), ("q", "quit"),
        ]),
        Screen::Workflows => legend_spans(&[
            ("↑↓", "move"), ("r", "reload"), ("q", "quit"),
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

fn render_confirm_discard(f: &mut Frame, area: Rect) {
    use ratatui::widgets::Clear;
    let width = 44u16.min(area.width);
    let height = 5u16;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let dialog = Rect::new(x, y, width, height);
    f.render_widget(Clear, dialog);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Discard changes? ", theme::accent_bold()))
        .border_style(theme::danger());
    let inner = block.inner(dialog);
    f.render_widget(block, dialog);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled("  You have unsaved changes.", theme::dim())),
        Line::from(Line::from(vec![
            Span::raw("  "),
            Span::styled("y", theme::accent_bold()),
            Span::styled(" discard  ", theme::muted()),
            Span::styled("n", theme::accent_bold()),
            Span::styled("/", theme::inactive()),
            Span::styled("esc", theme::accent_bold()),
            Span::styled(" keep editing", theme::muted()),
        ])),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

// ─── overlay rendering ────────────────────────────────────────────────────────

fn render_overlay(f: &mut Frame, app: &App, area: Rect) {
    match &app.overlay {
        Overlay::None => {}
        Overlay::Help => render_help(f, area),
        Overlay::ProfileCreate(wiz) => wiz.render(f, area),
        Overlay::ProfileSecrets(state) => state.render(f, area),
        Overlay::CapabilityCreate(wiz) => wiz.render(f, area),
        Overlay::PackCreate(wiz) => wiz.render(f, area),
        Overlay::PackEdit { pack_name, checklist } => render_pack_edit(f, pack_name, checklist, area),
        Overlay::InstallPack(buf) => render_install_pack(f, buf, area),
        Overlay::InfisicalSetup(state) => state.render(f, area),
        Overlay::SecretsTreeBrowser(tree) => tree.render(f, area),
        Overlay::InfisicalAccounts(state) => state.render(f, area),
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
        help_line("ctrl+g", "sync with git remote"),
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
