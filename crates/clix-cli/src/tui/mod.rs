use std::io;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use anyhow::Result;
use crate::tui::app::{App, ModalKind, Screen};
pub mod app;
pub mod screens;

pub fn run() -> Result<()> {
    // Load app data BEFORE touching terminal
    let mut app = App::new()?;

    // terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Restore terminal on panic
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        orig_hook(info);
    }));

    let result = run_loop(&mut terminal, &mut app);

    // cleanup (always runs, ignore errors to ensure cleanup proceeds)
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| render(f, app))?;
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                app.handle_key(key);
            }
        }
        if app.should_quit { break; }
    }
    Ok(())
}

fn render(f: &mut Frame, app: &App) {
    // Layout: 1-line tab bar, content area, 1-line status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    // Tab bar
    let titles = vec!["[1] Profiles", "[2] Capabilities", "[3] Packs"];
    let selected = match app.screen {
        Screen::Profiles => 0,
        Screen::Capabilities => 1,
        Screen::Packs => 2,
    };
    let tabs = Tabs::new(titles)
        .select(selected)
        .highlight_style(Style::default().bold().fg(Color::Rgb(231, 91, 42)));
    f.render_widget(tabs, chunks[0]);

    // Screen content
    match app.screen {
        Screen::Profiles => screens::profiles::render(f, app, chunks[1]),
        Screen::Capabilities => screens::capabilities::render(f, app, chunks[1]),
        Screen::Packs => screens::packs::render(f, app, chunks[1]),
    }

    // Status bar
    let status = app.last_error.as_deref()
        .unwrap_or("q:quit  r:reload  ←/→:tabs  ↑/↓:select  Enter:action  n:new  i:install(packs)  Esc:back");
    let style = if app.last_error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let status_bar = Paragraph::new(status).style(style);
    f.render_widget(status_bar, chunks[2]);

    // Render modal overlay if open
    if app.modal.is_open() {
        render_modal(f, app);
    }
}

fn render_modal(f: &mut Frame, app: &App) {
    let area = f.area();
    let (title, field_labels): (&str, &[&str]) = match app.modal.kind.as_ref().unwrap() {
        ModalKind::CreateProfile => ("Create Profile", &["Name", "Description (optional)"]),
        ModalKind::CreateCapability => ("Create Capability", &["Name (e.g. git.status)", "Description (optional)", "Command (e.g. git)"]),
        ModalKind::CreatePack => ("Create Pack", &["Name", "Description (optional)"]),
        ModalKind::InstallPack => ("Install Pack", &["Path to directory or .clixpack.zip"]),
    };
    let mut height = (field_labels.len() as u16) * 3 + 4;
    let mut width = area.width * 6 / 10;
    height = height.min(area.height);
    width = width.min(area.width);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = ratatui::layout::Rect::new(x, y, width, height);

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", title))
        .border_style(Style::default().fg(Color::Rgb(231, 91, 42)));
    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let field_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(field_labels.iter().map(|_| Constraint::Length(3)).collect::<Vec<_>>())
        .split(inner);

    for (i, (label, chunk)) in field_labels.iter().zip(field_chunks.iter()).enumerate() {
        let value = if i == app.modal.field_idx {
            format!("{}_", app.modal.input_buf)
        } else {
            app.modal.fields.get(i).cloned().unwrap_or_default()
        };
        let style = if i == app.modal.field_idx {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let field = Paragraph::new(value)
            .block(Block::default().borders(Borders::ALL).title(*label).border_style(style));
        f.render_widget(field, *chunk);
    }
}
