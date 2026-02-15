pub mod interactive_mode;
pub mod print_mode;
pub mod components;
pub mod state;
pub mod events;
pub mod app;
pub mod markdown;

use crate::error::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, stderr};
use tokio::sync::mpsc;

/// Initialize the terminal for TUI
pub fn init_terminal() -> Result<Terminal<CrosstermBackend<io::Stderr>>> {
    enable_raw_mode()?;
    let mut stderr = stderr();
    execute!(stderr, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stderr);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore the terminal to normal mode
pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stderr>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}


/// Event types for TUI
#[derive(Debug)]
pub enum TuiEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize(u16, u16),
    Tick,
    Message(String),
    CommandOutput(String),  // For tool output that should be collapsible
    Error(String),
    Exit,
    Redraw,
    ToolExecutionComplete {
        tool_use_id: String,
        result: std::result::Result<crate::ai::ContentPart, String>,
    },
    PermissionRequired {
        tool_name: String,
        command: String,
        tool_use_id: String,
        input: serde_json::Value,
        responder: tokio::sync::oneshot::Sender<PermissionDecision>,
    },
    ProcessingComplete,
    CancelOperation,
    UpdateTaskStatus(Option<String>),
    TodosUpdated(Vec<crate::ai::todo_tool::Todo>),
    SetIterationLimit(bool, Option<Vec<crate::ai::Message>>),
    SetStreamCanceller(Option<std::sync::Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<()>>>>>),
}

/// Permission decision from user
#[derive(Debug, Clone)]
pub enum PermissionDecision {
    Allow,
    Deny,
    AlwaysAllow,
    Never,
    Wait,  // User wants to provide feedback before continuing
}

/// Create event handler channel
pub fn create_event_handler() -> (mpsc::UnboundedSender<TuiEvent>, mpsc::UnboundedReceiver<TuiEvent>) {
    mpsc::unbounded_channel()
}

/// Run event loop in background
pub async fn run_event_loop(tx: mpsc::UnboundedSender<TuiEvent>) {
    let tick_rate = std::time::Duration::from_millis(250);
    let mut last_tick = std::time::Instant::now();
    
    loop {
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| std::time::Duration::from_secs(0));
            
        if event::poll(timeout).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(key)) => {
                    // Check for quit combinations
                    if (key.code == KeyCode::Char('q') || key.code == KeyCode::Char('d')) 
                        && key.modifiers.contains(KeyModifiers::CONTROL) {
                        let _ = tx.send(TuiEvent::Exit);
                        break;
                    }
                    let _ = tx.send(TuiEvent::Key(key));
                }
                Ok(Event::Mouse(mouse)) => {
                    let _ = tx.send(TuiEvent::Mouse(mouse));
                }
                Ok(Event::Paste(data)) => {
                    let _ = tx.send(TuiEvent::Paste(data));
                }
                Ok(Event::Resize(width, height)) => {
                    let _ = tx.send(TuiEvent::Resize(width, height));
                }
                _ => {}
            }
        }
        
        if last_tick.elapsed() >= tick_rate {
            let _ = tx.send(TuiEvent::Tick);
            last_tick = std::time::Instant::now();
        }
    }
}
