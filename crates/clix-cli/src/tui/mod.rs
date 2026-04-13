use std::io;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use anyhow::Result;
use crate::tui::app::{App, Screen};

pub mod app;
pub mod screens;

pub fn run() -> Result<()> {
    // terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new()?;
    let result = run_loop(&mut terminal, &mut app);

    // cleanup (always runs)
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| render(f, app))?;
        if let Event::Key(key) = event::read()? {
            app.handle_key(key);
        }
        if app.should_quit { break; }
    }
    Ok(())
}

fn render(f: &mut Frame, app: &App) {
    // Layout: 1-line tab bar at top, rest to content area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
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
        .highlight_style(Style::default().bold());
    f.render_widget(tabs, chunks[0]);

    // Content placeholder — replaced in Task 3
    let placeholder = Paragraph::new("Press q to quit | r to reload | 1/2/3 to switch screens")
        .alignment(Alignment::Center);
    f.render_widget(placeholder, chunks[1]);
}
