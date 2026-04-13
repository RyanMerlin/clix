use std::io;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use anyhow::Result;
use crate::tui::app::{App, Screen};
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
        .highlight_style(Style::default().bold().fg(Color::Yellow));
    f.render_widget(tabs, chunks[0]);

    // Screen content
    match app.screen {
        Screen::Profiles => screens::profiles::render(f, app, chunks[1]),
        Screen::Capabilities => screens::capabilities::render(f, app, chunks[1]),
        Screen::Packs => screens::packs::render(f, app, chunks[1]),
    }

    // Status bar
    let status = app.last_error.as_deref()
        .unwrap_or("q:quit  r:reload  1/2/3/Tab:screens  j/k:navigate  Enter:select  Esc:back");
    let style = if app.last_error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let status_bar = Paragraph::new(status).style(style);
    f.render_widget(status_bar, chunks[2]);
}
