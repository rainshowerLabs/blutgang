use crate::config::types::Settings;
use crate::rpc::types::Rpc;

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
    sync::{
        Arc,
        RwLock,
    },
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
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    response_list: &Arc<RwLock<Vec<String>>>,
) -> Result<(), Box<dyn Error>> {
    // Redraw the full terminal window since we are not doing the new screen thing
    let _ = terminal.clear();

    // Draw the tui in a loop
    loop {
        let _ = terminal.draw(|f| ui(f, &config, &rpc_list, &response_list))?;
        // Make sure the cursor is shown because we dont want to do raw mode
        terminal.show_cursor()?;

        // Wait 50ms so we dont constantly block everything
        sleep(std::time::Duration::from_millis(50)).await;
    }
}

fn format_rpc_text(rpc_list: &Arc<RwLock<Vec<Rpc>>>) -> Vec<Line<'static>> {
    let mut rpc_text = Vec::new();
    let rpc_list = rpc_list.read().unwrap();
    for rpc in rpc_list.iter() {
        rpc_text.push(Line::from(format!("RPC URL: {}", rpc.url)));
        rpc_text.push(Line::from(format!(
            "Latency (avg): {:.2} ms | Consecutive: {} | Is erroring: {}",
            rpc.status.latency / 1_000_000.0,
            rpc.consecutive,
            rpc.status.is_erroring
        )));
    }
    rpc_text
}

fn format_response_list(response_list: &Arc<RwLock<Vec<String>>>) -> Vec<Line<'static>> {
    let mut response_text = Vec::new();
    let response_list = response_list.read().unwrap();
    for response in response_list.iter().rev() {
        response_text.push(Line::from(format!("{}", response)));
    }
    response_text
}

fn ui<B: Backend>(
    f: &mut Frame<B>,
    config: &Settings,
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    response_list: &Arc<RwLock<Vec<String>>>,
) {
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
        .block(Block::default().title("Blutgang").borders(Borders::ALL))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    f.render_widget(block, chunks[0]);
    let stats = Paragraph::new(format_response_list(response_list))
        .block(Block::default().title("Responses").borders(Borders::ALL))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    f.render_widget(stats, chunks[1]);
    let rpcs = Paragraph::new(format_rpc_text(rpc_list))
        .block(Block::default().title("RPCs").borders(Borders::ALL))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    f.render_widget(rpcs, chunks[2]);
}
