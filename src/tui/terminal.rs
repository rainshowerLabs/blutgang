use crate::config::types::Settings;

use crossterm::execute;
use ratatui::{
    backend::Backend,
    backend::CrosstermBackend,
    layout::{
        Constraint,
        Direction,
        Layout,
    },
    prelude::*,
    widgets::*,
    Frame,
    Terminal,
};
use std::{
    error::Error,
    io::Stdout,
};
use tokio::time::sleep;

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>, Box<dyn Error>> {
    let mut stdout = std::io::stdout();
    //enable_raw_mode()?;
    execute!(stdout)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

pub async fn run_tui(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    config: Settings,
) -> Result<(), Box<dyn Error>> {
    // Redraw the full terminal window since we are not doing the new screen thing
    let _ = terminal.clear();

    // Draw the tui in a loop
    loop {
        let _ = terminal.draw(|f| ui(f, &config))?;
        // Make sure the cursor is shown because we dont want to do raw mode
        terminal.show_cursor()?;

        // Wait 50ms so we dont constantly block everything
        sleep(std::time::Duration::from_millis(50)).await;
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, config: &Settings) {
    // Get address we're bound to from the config
    let address = config.address.to_string();
    // Parse the DB settings from the config
    let db_settings = &config.sled_config;

    let blutgang_text = vec![
        Line::from(format!("Bound to: {}", address)),
        Line::from(format!("DB info: {:?}", db_settings)),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Percentage(10),
                Constraint::Percentage(60),
                Constraint::Percentage(30),
            ]
            .as_ref(),
        )
        .split(f.size());

    let block = Paragraph::new(blutgang_text)
        .block(Block::default().title("Paragraph").borders(Borders::ALL))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    f.render_widget(block, chunks[0]);
    let stats = Block::default().title("Stats").borders(Borders::ALL);
    f.render_widget(stats, chunks[1]);
    let rpcs = Block::default().title("RPCs").borders(Borders::ALL);
    f.render_widget(rpcs, chunks[2]);
}
