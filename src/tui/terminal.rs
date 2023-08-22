use crossterm::{
    execute,
    terminal::{
        disable_raw_mode,
        enable_raw_mode,
    },
};
use ratatui::{
    backend::Backend,
    backend::CrosstermBackend,
    layout::{
        Constraint,
        Direction,
        Layout,
    },
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

pub fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut())?;
    Ok(terminal.show_cursor()?)
}

pub async fn run_tui(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<(), Box<dyn Error>> {
    // Redraw the full terminal window since we are not doing the new screen thing
    let _ = terminal.clear();

    // Draw the tui in a loop
    loop {
    	let _ = terminal.draw(|f| ui(f))?;
    	// Make sure the cursor is shown because we dont want to do raw mode
    	terminal.show_cursor()?;

    	// Wait 350ms so we dont constantly block everything
    	sleep(std::time::Duration::from_millis(350)).await;
    }
    
    Ok(())
}

fn ui<B: Backend>(f: &mut Frame<B>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Percentage(10), Constraint::Percentage(90)].as_ref())
        .split(f.size());
    let block = Block::default().title("Blutgang").borders(Borders::ALL);
    f.render_widget(block, chunks[0]);
    let block = Block::default().title("Stats").borders(Borders::ALL);
    f.render_widget(block, chunks[1]);
}
