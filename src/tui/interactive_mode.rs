use crate::error::Result;
use crate::mcp;
use crate::telemetry;
use crate::tui::{
    self, create_event_handler, init_terminal, restore_terminal, TuiEvent,
};
use crate::tui::components::{ChatView, StatusBar, ToolPanel, UiMessage as Message};
use crate::tui::state::AppState;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::path::PathBuf;
use tokio::sync::mpsc;
use crossterm::event::{EnableBracketedPaste, DisableBracketedPaste, KeyEvent, KeyCode, KeyModifiers};
use crossterm::execute;
use tui_textarea::{Input, Key};

/// Options for interactive mode
#[derive(Debug, Clone)]
pub struct InteractiveOptions {
    pub initial_prompt: Option<String>,
    pub debug: bool,
    pub verbose: bool,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub model: Option<String>,
    pub add_dirs: Vec<PathBuf>,
    pub continue_conversation: bool,
    pub resume_session_id: Option<String>,
    pub mcp_config: Option<String>,
    pub dangerously_skip_permissions: bool,
}

/// Run the interactive TUI
pub async fn run(options: InteractiveOptions) -> Result<()> {
    // Initialize terminal
    let mut terminal = init_terminal()?;
    
    // Enable bracketed paste mode
    execute!(
        terminal.backend_mut(),
        EnableBracketedPaste
    )?;
    
    // Create event channel
    let (tx, mut rx) = create_event_handler();
    
    // Start event loop in background
    let event_tx = tx.clone();
    tokio::spawn(async move {
        tui::run_event_loop(event_tx).await;
    });
    
    // Initialize app state
    let mut app_state = AppState::new(options.clone());
    
    // Set the event sender for background tasks
    app_state.event_tx = Some(tx.clone());
    
    // Start the persistent agent loop for the entire session
    app_state.start_agent_loop();
    
    // Load MCP servers if configured
    if let Some(mcp_config) = &options.mcp_config {
        load_mcp_servers(&mut app_state, mcp_config).await?;
    }
    
    // Handle continue/resume
    if options.continue_conversation {
        app_state.continue_last_conversation().await?;
    } else if let Some(session_id) = &options.resume_session_id {
        app_state.resume_conversation(session_id).await?;
    }
    
    // Track telemetry
    telemetry::track("interactive_session_start", None::<serde_json::Value>).await;
    
    // Main loop
    let result = run_app(&mut terminal, &mut app_state, &mut rx).await;
    
    // Disable bracketed paste mode
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste
    )?;
    
    // Restore terminal
    restore_terminal(&mut terminal)?;
    
    // Track telemetry
    telemetry::track("interactive_session_end", None::<serde_json::Value>).await;
    
    result
}

/// Main application loop
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
    app_state: &mut AppState,
    rx: &mut mpsc::UnboundedReceiver<TuiEvent>,
) -> Result<()> {
    let mut needs_redraw = true;
    
    loop {
        // Only draw when needed
        if needs_redraw {
            terminal.draw(|f| draw_ui(f, app_state))?;
            needs_redraw = false;
        }
        
        // Handle events
        if let Some(event) = rx.recv().await {
            match event {
                TuiEvent::Exit => break,
                TuiEvent::Key(key) => {
                    if let Err(e) = handle_key_event(app_state, key).await {
                        // Log error to stderr so we can see it even if TUI crashes
                        eprintln!("Error handling key event: {}", e);
                        app_state.add_error(&format!("Error: {}", e));
                    }
                    needs_redraw = true;
                }
                TuiEvent::Mouse(mouse) => {
                    use crossterm::event::{MouseEventKind, MouseButton};
                    match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            app_state.scroll_up(3); // Scroll 3 lines at a time
                            needs_redraw = true;
                        }
                        MouseEventKind::ScrollDown => {
                            app_state.scroll_down(3);
                            needs_redraw = true;
                        }
                        _ => {}
                    }
                }
                TuiEvent::Paste(text) => {
                    if app_state.input_mode {
                        // Handle paste like JavaScript implementation
                        const MAX_TEXT_LENGTH: usize = 10_000;  // num90 from JS
                        const TRUNCATE_KEEP: usize = 500;       // num91/2 from JS
                        
                        // Count lines in the ORIGINAL text, not processed text
                        let original_line_count = text.lines().count();
                        
                        let processed_text = if text.len() > MAX_TEXT_LENGTH {
                            // Truncate large text keeping first 500 and last 500 chars
                            let start = &text[..TRUNCATE_KEEP];
                            let end = &text[text.len() - TRUNCATE_KEEP..];
                            
                            // Get the middle section for line counting
                            let middle = &text[TRUNCATE_KEEP..text.len() - TRUNCATE_KEEP];
                            let middle_lines = middle.lines().count();
                            
                            // Get next paste ID for truncation placeholder
                            let paste_id = app_state.next_paste_id;
                            app_state.next_paste_id += 1;
                            
                            // Store the truncated middle content
                            app_state.pasted_contents.insert(paste_id, middle.to_string());
                            
                            // Create truncated text with placeholder
                            let truncation_placeholder = format!("[...Truncated text #{} +{} lines...]", paste_id, middle_lines);
                            format!("{}{}{}", start, truncation_placeholder, end)
                        } else {
                            text.clone()
                        };
                        
                        // Check if paste is large enough to use placeholder
                        if original_line_count > 3 || text.len() > 800 {  // Threshold from JS (num93)
                            // Get next paste ID
                            let paste_id = app_state.next_paste_id;
                            app_state.next_paste_id += 1;
                            
                            // Store full content in pasted_contents - store the ORIGINAL text, not truncated
                            app_state.pasted_contents.insert(paste_id, text.clone());
                            
                            // Show placeholder in input box - show the TOTAL line count
                            let placeholder = format!("[Pasted text #{} +{} lines]", paste_id, original_line_count);
                            app_state.input_textarea.insert_str(&placeholder);
                            
                            // Don't show preview immediately - wait for submission
                        } else {
                            // Small paste - insert directly
                            app_state.input_textarea.insert_str(&processed_text);
                        }
                    }
                    needs_redraw = true;
                }
                TuiEvent::Resize(width, height) => {
                    app_state.handle_resize(width, height);
                    needs_redraw = true;
                }
                TuiEvent::Message(msg) => {
                    app_state.add_message(&msg);
                    needs_redraw = true;
                }
                TuiEvent::CommandOutput(output) => {
                    app_state.add_command_output(&output);
                    needs_redraw = true;
                }
                TuiEvent::Error(err) => {
                    app_state.add_error(&err);
                    needs_redraw = true;
                }
                TuiEvent::Tick => {
                    // Only redraw on tick if processing or animations needed
                    if app_state.is_processing {
                        needs_redraw = true;
                    }
                    app_state.tick().await?;
                }
                TuiEvent::Redraw => {
                    // Force a redraw for streaming updates
                    needs_redraw = true;
                }
                TuiEvent::PermissionRequired { tool_name, command, tool_use_id, input, responder } => {
                    // Add to the queue of pending permissions
                    app_state.pending_permissions.push_back(crate::tui::state::PendingPermission {
                        tool_name: tool_name.clone(),
                        command: command.clone(),
                        tool_use_id,
                        input,
                        responder,
                    });
                    
                    // Only show dialog if this is the first permission in the queue (no dialog already visible)
                    if app_state.pending_permissions.len() == 1 && !app_state.permission_dialog.visible {
                        app_state.permission_dialog.show(crate::permissions::PermissionRequest {
                            id: uuid::Uuid::new_v4().to_string(),
                            tool_name,
                            action: "execute".to_string(),
                            details: command,
                            timestamp: std::time::Instant::now(),
                        });
                    }
                    
                    needs_redraw = true;
                }
                TuiEvent::ProcessingComplete => {
                    // Unlock the UI when processing completes
                    app_state.is_processing = false;
                    app_state.input_mode = true;
                    needs_redraw = true;
                }
                TuiEvent::CancelOperation => {
                    // Send cancellation to agent loop
                    if let Some(tx) = &app_state.cancel_tx {
                        let _ = tx.send(());
                    }
                    // Ensure UI is unlocked
                    app_state.is_processing = false;
                    app_state.input_mode = true;
                    needs_redraw = true;
                }
                TuiEvent::UpdateTaskStatus(status) => {
                    app_state.set_task_status(status);
                    needs_redraw = true;
                }
                TuiEvent::TodosUpdated(todos) => {
                    app_state.update_todos(todos);
                    needs_redraw = true;
                }
                TuiEvent::SetIterationLimit(hit_limit, messages) => {
                    app_state.hit_iteration_limit = hit_limit;
                    app_state.continuation_messages = messages;
                    needs_redraw = true;
                }
                TuiEvent::SetStreamCanceller(canceller) => {
                    app_state.stream_cancel_tx = canceller;
                }
                TuiEvent::ToolExecutionComplete { tool_use_id, result } => {
                    // Handle tool execution completion
                    app_state.is_processing = false;
                    
                    match result {
                        Ok(tool_result) => {
                            // Display the actual tool output to the user
                            if let crate::ai::ContentPart::ToolResult { content, is_error, .. } = &tool_result {
                                if let Some(true) = is_error {
                                    app_state.add_error(content);
                                } else {
                                    // Add the command output as a message
                                    app_state.messages.push(crate::tui::components::UiMessage {
                                        role: "tool".to_string(),
                                        content: content.clone(),
                                        timestamp: crate::utils::timestamp_ms(),
                                    });
                                    app_state.invalidate_cache();
                                    app_state.scroll_to_bottom();
                                }
                            }
                            app_state.pending_tool_result = Some(tool_result);
                            app_state.continue_after_permission = true;
                        }
                        Err(error) => {
                            app_state.add_error(&format!("Tool execution failed: {}", error));
                            app_state.pending_tool_result = Some(crate::ai::ContentPart::ToolResult {
                                tool_use_id,
                                content: error,
                                is_error: Some(true),
                            });
                            app_state.continue_after_permission = true;
                        }
                    }
                    needs_redraw = true;
                }
            }
        }
        
        // Check if we should exit
        if app_state.should_exit() {
            break;
        }
    }
    
    Ok(())
}

/// Draw the UI
fn draw_ui(f: &mut Frame, app_state: &mut AppState) {
    let size = f.area();
    
    // Update input state detection for paste handling
    app_state.detect_paste_and_update_input_state();
    
    // Get dynamic input height based on expansion state
    let input_height = app_state.get_input_display_height();
    
    // Create main layout with spacing
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),               // Chat area
            Constraint::Length(1),            // Padding between chat and input
            Constraint::Length(input_height), // Dynamic input area
            Constraint::Length(1),            // Status bar
        ])
        .split(size);
    
    // Draw chat view with scrolling support
    // Get cached lines and rebuild cache if needed
    let cached_lines = app_state.get_cached_lines().clone();
    
    let chat_view = ChatView::new(&app_state.messages)
        .with_scroll(app_state.scroll_offset)
        .with_session_picker(
            app_state.show_session_picker,
            app_state.session_picker_selected
        )
        .with_expanded(app_state.expanded_view)
        .with_cached_lines(&cached_lines)
        .with_task_status(
            app_state.current_task_status.as_deref(),
            app_state.get_spinner_char(),
            app_state.is_processing
        )
        .with_next_todo(app_state.next_todo.as_deref());
    f.render_widget(chat_view, chunks[0]);
    
    // chunks[1] is now the padding space - leave it empty
    
    // Draw textarea with border - create title based on input state
    let line_count = app_state.calculate_input_line_count();
    let title = if app_state.input_expanded {
        if line_count > 1 {
            format!(" Input ({} lines, Enter to send) ", line_count)
        } else {
            " Input (Shift+Enter for newline, Enter to send) ".to_string()
        }
    } else {
        // Collapsed state - show line count indicator
        let collapsed_lines = line_count.saturating_sub(3);
        if collapsed_lines > 0 {
            format!(" Input (collapsed, +{} lines, Ctrl+E to expand) ", collapsed_lines)
        } else {
            " Input (Ctrl+E to expand, Enter to send) ".to_string()
        }
    };
    
    let input_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(if app_state.input_mode {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    let inner = input_block.inner(chunks[2]);
    f.render_widget(input_block, chunks[2]);
    
    // Render input content based on expansion state
    if app_state.input_expanded {
        // Normal expanded view - let textarea handle everything
        f.render_widget(&app_state.input_textarea, inner);
    } else {
        // Collapsed view - show only first 3 lines with manual rendering
        let lines: Vec<String> = app_state.input_textarea.lines().into_iter().map(|s| s.to_string()).collect();
        let mut display_lines = Vec::new();
        let max_width = inner.width as usize;
        
        // Show first 3 lines with wrapping for long lines
        let mut visual_line_count = 0;
        for line in lines.iter() {
            if visual_line_count >= 3 {
                break;
            }
            
            // Wrap long lines to fit within the inner width
            if line.len() > max_width {
                let chunks: Vec<String> = line.chars()
                    .collect::<Vec<_>>()
                    .chunks(max_width.saturating_sub(1)) // Leave space for cursor
                    .map(|chunk| chunk.iter().collect::<String>())
                    .collect();
                    
                for chunk in chunks {
                    if visual_line_count >= 3 {
                        break;
                    }
                    display_lines.push(Line::from(chunk));
                    visual_line_count += 1;
                }
            } else {
                display_lines.push(Line::from(line.clone()));
                visual_line_count += 1;
            }
        }
        
        // Add indicator if there are more lines
        if lines.len() > 3 {
            let extra_lines = lines.len() - 3;
            let indicator = format!("... +{} more lines (Ctrl+E to expand)", extra_lines);
            display_lines.push(Line::from(Span::styled(
                indicator,
                Style::default().fg(Color::DarkGray)
            )));
        }
        
        // Fill remaining space with empty lines if needed
        let available_height = inner.height as usize;
        while display_lines.len() < available_height {
            display_lines.push(Line::from(""));
        }
        
        let collapsed_paragraph = Paragraph::new(display_lines);
        f.render_widget(collapsed_paragraph, inner);
    }
    
    // Draw status bar
    let status_bar = StatusBar::new(app_state);
    f.render_widget(status_bar, chunks[3]);
    
    // Draw tool panel if active
    if app_state.show_tool_panel {
        let area = centered_rect(80, 60, size);
        f.render_widget(Clear, area);
        let tool_panel = ToolPanel::new(&app_state.active_tools);
        f.render_widget(tool_panel, area);
    }
    
    // Draw help overlay if active
    if app_state.show_help {
        let area = centered_rect(60, 80, size);
        f.render_widget(Clear, area);
        draw_help(f, area);
    }
    
    // Draw debug panel if active
    if app_state.debug_mode {
        let area = Rect {
            x: size.width - 40,
            y: 0,
            width: 40,
            height: size.height - 1,
        };
        f.render_widget(Clear, area);
        draw_debug_panel(f, area, app_state);
    }
    
    // Draw session picker overlay if active
    if app_state.show_session_picker {
        draw_session_picker(f, size, app_state);
    }

    // Draw status view overlay if active (matches JavaScript tabbed UI)
    if app_state.show_status_view {
        draw_status_view(f, size, app_state);
    }

    // Draw permission dialog if active
    app_state.permission_dialog.render(f, size);
    
    // Draw autocomplete dropdown if active
    if app_state.is_autocomplete_visible && !app_state.autocomplete_matches.is_empty() {
        // Position dropdown just above the input area
        let dropdown_height = (app_state.autocomplete_matches.len() * 3 + 2).min(32); // 3 lines per item + border
        let dropdown_width = 60; // Fixed width
        
        let dropdown_area = Rect {
            x: chunks[1].x,
            y: chunks[1].y.saturating_sub(dropdown_height as u16),
            width: dropdown_width.min(chunks[1].width),
            height: dropdown_height as u16,
        };
        
        f.render_widget(Clear, dropdown_area);
        let dropdown = crate::tui::components::AutocompleteDropdown::new(
            &app_state.autocomplete_matches,
            app_state.selected_suggestion
        );
        f.render_widget(dropdown, dropdown_area);
    }
}

/// Convert crossterm KeyEvent to tui_textarea Input
fn convert_key_to_input(key: KeyEvent) -> Input {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    
    let key_code = match key.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Tab => Key::Tab,
        KeyCode::Delete => Key::Delete,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Esc => Key::Esc,
        KeyCode::F(n) => Key::F(n),
        _ => Key::Null,
    };
    
    Input {
        key: key_code,
        ctrl,
        alt,
        shift,
    }
}

/// Handle key events
async fn handle_key_event(app_state: &mut AppState, key: KeyEvent) -> Result<()> {
    // Handle permission dialog first if it's active
    if app_state.permission_dialog.visible {
        if let Some(decision) = app_state.permission_dialog.handle_key(key) {
            use crate::permissions::PermissionBehavior;
            
            // Hide the dialog
            app_state.permission_dialog.hide();
            
            // Handle the streaming permission flow - take from front of queue
            if let Some(pending) = app_state.pending_permissions.pop_front() {
                // Convert PermissionBehavior to PermissionDecision
                let permission_decision = match decision {
                    PermissionBehavior::Allow => crate::tui::PermissionDecision::Allow,
                    PermissionBehavior::AlwaysAllow => crate::tui::PermissionDecision::AlwaysAllow,
                    PermissionBehavior::Deny => crate::tui::PermissionDecision::Deny,
                    PermissionBehavior::Never => crate::tui::PermissionDecision::Never,
                    PermissionBehavior::Wait => crate::tui::PermissionDecision::Wait,
                    _ => crate::tui::PermissionDecision::Deny,
                };
                
                // Send decision back through the oneshot channel to the streaming flow
                // The streaming flow will handle updating the global permission context
                let _ = pending.responder.send(permission_decision);
            }
            
            // Check if there are more permissions pending and show the next dialog
            if let Some(next_pending) = app_state.pending_permissions.front() {
                app_state.permission_dialog.show(crate::permissions::PermissionRequest {
                    id: uuid::Uuid::new_v4().to_string(),
                    tool_name: next_pending.tool_name.clone(),
                    action: "execute command".to_string(),
                    details: next_pending.command.clone(),
                    timestamp: std::time::Instant::now(),
                });
            }
            // OLD PERMISSION FLOW REMOVED: All permission handling now happens in streaming flow
        }
        return Ok(());
    }

    // Handle status view keys (matches JavaScript - Tab to cycle, Esc to close)
    if app_state.show_status_view {
        match key.code {
            KeyCode::Tab => {
                // Cycle through tabs: Status (0) -> Config (1) -> Usage (2) -> Status (0)
                app_state.status_view_tab = (app_state.status_view_tab + 1) % 3;
                app_state.status_config_selected = 0;  // Reset selection when changing tabs
                return Ok(());
            }
            KeyCode::BackTab => {
                // Reverse cycle through tabs
                app_state.status_view_tab = if app_state.status_view_tab == 0 { 2 } else { app_state.status_view_tab - 1 };
                app_state.status_config_selected = 0;
                return Ok(());
            }
            KeyCode::Up => {
                // Only navigate in Config tab
                if app_state.status_view_tab == 1 && app_state.status_config_selected > 0 {
                    app_state.status_config_selected -= 1;
                }
                return Ok(());
            }
            KeyCode::Down => {
                // Only navigate in Config tab (17 settings total from JavaScript)
                if app_state.status_view_tab == 1 && app_state.status_config_selected < 16 {
                    app_state.status_config_selected += 1;
                }
                return Ok(());
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                // Toggle setting in Config tab
                if app_state.status_view_tab == 1 {
                    // Config settings toggling would be implemented here
                    // For now, just acknowledge the key press
                }
                return Ok(());
            }
            KeyCode::Esc => {
                app_state.show_status_view = false;
                return Ok(());
            }
            _ => return Ok(()),
        }
    }

    if app_state.show_session_picker {
        match key.code {
            KeyCode::Up => {
                if app_state.session_picker_selected > 0 {
                    app_state.session_picker_selected -= 1;
                }
                return Ok(());
            }
            KeyCode::Down => {
                if app_state.session_picker_selected < app_state.session_picker_items.len().saturating_sub(1) {
                    app_state.session_picker_selected += 1;
                }
                return Ok(());
            }
            KeyCode::Enter => {
                let session_id = app_state.session_picker_items[app_state.session_picker_selected].id.clone();
                app_state.show_session_picker = false;
                app_state.resume_conversation(&session_id).await?;
                return Ok(());
            }
            KeyCode::Esc => {
                app_state.show_session_picker = false;
                app_state.clear_messages();
                return Ok(());
            }
            _ => return Ok(()),
        }
    }
    
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app_state.quit();
            return Ok(());
        }
        KeyCode::Char('?') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app_state.toggle_help();
            return Ok(());
        }
        KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app_state.toggle_debug();
            return Ok(());
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app_state.clear_messages();
            return Ok(());
        }
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Toggle expanded view mode (shows full output vs collapsed)
            app_state.expanded_view = !app_state.expanded_view;
            return Ok(());
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if app_state.is_processing {
                app_state.cancel_operation().await?;
                // Add cancellation feedback
                app_state.messages.push(Message {
                    role: "assistant".to_string(),
                    content: "Operation cancelled by user.".to_string(),
                    timestamp: crate::utils::timestamp_ms(),
                });
                app_state.scroll_to_bottom();
            } else if app_state.show_help {
                app_state.toggle_help();
            } else if app_state.show_tool_panel {
                app_state.toggle_tool_panel();
            }
            return Ok(());
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Toggle input area expansion (Ctrl+E)
            app_state.toggle_input_expansion();
            return Ok(());
        }
        // Arrow keys are for input history when in input mode, not scrolling
        KeyCode::Esc => {
            // First check if we're processing and should cancel
            if app_state.is_processing {
                app_state.cancel_operation().await?;
                // Add cancellation feedback
                app_state.messages.push(Message {
                    role: "assistant".to_string(),
                    content: "Operation cancelled by user.".to_string(),
                    timestamp: crate::utils::timestamp_ms(),
                });
                app_state.scroll_to_bottom();
                return Ok(());
            }
            // Then handle dialogs
            if app_state.show_help {
                app_state.toggle_help();
            } else if app_state.show_tool_panel {
                app_state.toggle_tool_panel();
            }
            return Ok(());
        }
        _ => {}
    }
    
    // Handle autocomplete dropdown first if it's visible
    if app_state.is_autocomplete_visible && !app_state.autocomplete_matches.is_empty() {
        match key.code {
            KeyCode::Up => {
                app_state.autocomplete_select_previous();
                return Ok(());
            }
            KeyCode::Down => {
                app_state.autocomplete_select_next();
                return Ok(());
            }
            KeyCode::Enter => {
                app_state.autocomplete_select_current();
                return Ok(());
            }
            KeyCode::Esc => {
                // If processing, cancel operation takes priority
                if app_state.is_processing {
                    app_state.cancel_operation().await?;
                    app_state.messages.push(Message {
                        role: "assistant".to_string(),
                        content: "Operation cancelled by user.".to_string(),
                        timestamp: crate::utils::timestamp_ms(),
                    });
                    app_state.scroll_to_bottom();
                } else {
                    app_state.hide_autocomplete();
                }
                return Ok(());
            }
            _ => {
                // Let other keys pass through to normal input handling
                // This will update the input and trigger new search
            }
        }
    }

    // Handle input mode
    if app_state.input_mode {
        // Special handling for Enter - Shift+Enter for newline, Enter to submit
        if key.code == KeyCode::Enter {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                // Shift+Enter - insert newline
                app_state.input_textarea.insert_newline();
            } else {
                // Enter without Shift - submit message
                app_state.submit_input().await?;
            }
            return Ok(());
        }
        
        // Special handling for Tab - completion
        if key.code == KeyCode::Tab {
            app_state.handle_tab_completion();
            return Ok(());
        }
        
        // Ctrl+N for newline (since Shift+Enter may not work on all terminals)
        if key.code == KeyCode::Char('n') && key.modifiers.contains(KeyModifiers::CONTROL) {
            app_state.input_textarea.insert_newline();
            return Ok(());
        }
        
        // Override Ctrl+U to match bash behavior (delete to beginning of line)
        // tui-textarea maps Ctrl+U to undo by default, but bash uses it for delete-to-beginning
        if key.code == KeyCode::Char('u') && key.modifiers.contains(KeyModifiers::CONTROL) {
            app_state.input_textarea.delete_line_by_head();
            return Ok(());
        }
        
        // Special handling for history navigation
        // Allow up/down arrows for history when:
        // 1. Single line input, OR
        // 2. Multi-line but cursor is on first line (Up) or last line (Down)
        let lines = app_state.input_textarea.lines();
        let cursor_row = app_state.input_textarea.cursor().0;
        match key.code {
            KeyCode::Up => {
                // Allow history navigation if single line OR cursor is on first line
                if lines.len() <= 1 || cursor_row == 0 {
                    app_state.history_up();
                    return Ok(());
                }
            }
            KeyCode::Down => {
                // Allow history navigation if single line OR cursor is on last line  
                if lines.len() <= 1 || cursor_row == lines.len() - 1 {
                    app_state.history_down();
                    return Ok(());
                }
            }
            _ => {}
        }
        
        // Convert to tui_textarea Input and let it handle everything else
        let input = convert_key_to_input(key);
        app_state.input_textarea.input(input);
        
        // Update input state detection after any text changes
        app_state.detect_paste_and_update_input_state();
        
        // Trigger autocomplete search when input changes (matches JavaScript behavior)
        let current_input = app_state.input_textarea.lines()[0].clone();
        app_state.search_commands(&current_input);
    }
    
    Ok(())
}

/// Draw help overlay
fn draw_help(f: &mut Frame, area: Rect) {
    let help_text = vec![
        "llminate Interactive Mode - Help",
        "",
        "General Commands:",
        "  Ctrl+Q, Ctrl+D    Quit",
        "  Ctrl+C            Cancel current operation",
        "  Ctrl+L            Clear screen",
        "  Ctrl+?            Toggle this help",
        "  Ctrl+G            Toggle debug panel",
        "  Tab               Auto-complete",
        "  Up/Down           Navigate history",
        "",
        "Input Commands:",
        "  Enter             Send message",
        "  Shift+Enter       Insert newline",
        "  Ctrl+V            Paste (multiline supported)",
        "  Ctrl+E            Toggle input area expand/collapse",
        "",
        "Special Commands:",
        "  /help             Show available commands",
        "  /clear            Clear conversation",
        "  /save             Save conversation",
        "  /load             Load conversation",
        "  /model <name>     Change model",
        "  /tools            Show available tools",
        "  /mcp              List MCP servers",
        "  /exit, /quit      Exit application",
        "",
        "Press ESC to close this help",
    ];
    
    let help_widget = Paragraph::new(help_text.join("\n"))
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().fg(Color::White));
    
    f.render_widget(help_widget, area);
}

/// Draw session picker overlay
fn draw_session_picker(f: &mut Frame, area: Rect, app_state: &AppState) {
    let picker_area = centered_rect(90, 80, area);
    f.render_widget(Clear, picker_area);
    
    let block = Block::default()
        .title(" Select a conversation to resume ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));
    
    let inner = block.inner(picker_area);
    f.render_widget(block, picker_area);
    
    let mut lines = vec![
        ratatui::text::Line::from(" /resume"),
        ratatui::text::Line::from("     Modified     Created        Msgs Git Branch                     Summary"),
        ratatui::text::Line::from(""),
    ];
    
    for (i, session) in app_state.session_picker_items.iter().enumerate() {
        let modified = app_state.format_relative_time(session.modified_timestamp);
        let created = app_state.format_relative_time(session.created_timestamp);
        
        let summary = "Loading...";
        let msgs = 0;
        let branch = app_state.get_git_branch();
        
        let prefix = if i == app_state.session_picker_selected {
            "❯"
        } else {
            " "
        };
        
        let line_text = format!("{} {:>2}. {:12} {:12} {:>7} {:20} {}",
            prefix,
            i + 1,
            modified,
            created,
            msgs,
            branch,
            summary
        );
        
        let style = if i == app_state.session_picker_selected {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
            Style::default()
        };
        
        lines.push(ratatui::text::Line::from(vec![ratatui::text::Span::styled(line_text, style)]));
    }
    
    lines.push(ratatui::text::Line::from(""));
    lines.push(ratatui::text::Line::from("Use ↑/↓ to select, Enter to resume, Esc to cancel"));
    
    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

/// Draw status view overlay (matches JavaScript tabbed UI)
fn draw_status_view(f: &mut Frame, area: Rect, app_state: &AppState) {
    let status_area = centered_rect(85, 85, area);
    f.render_widget(Clear, status_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));

    let inner = block.inner(status_area);
    f.render_widget(block, status_area);

    // Tab header: Settings: [Status] Config Usage (tab to cycle)
    let tab_names = ["Status", "Config", "Usage"];
    let mut tab_spans: Vec<ratatui::text::Span> = vec![
        ratatui::text::Span::styled("Settings: ", Style::default().fg(Color::White)),
    ];

    for (i, name) in tab_names.iter().enumerate() {
        if i == app_state.status_view_tab {
            // Selected tab - highlighted
            tab_spans.push(ratatui::text::Span::styled(
                format!(" {} ", name),
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(ratatui::style::Modifier::BOLD)
            ));
        } else {
            tab_spans.push(ratatui::text::Span::styled(
                format!(" {} ", name),
                Style::default().fg(Color::Gray)
            ));
        }
    }
    tab_spans.push(ratatui::text::Span::styled(
        " (tab to cycle)",
        Style::default().fg(Color::DarkGray)
    ));

    let mut lines = vec![
        ratatui::text::Line::from(tab_spans),
        ratatui::text::Line::from(""),
    ];

    // Collect all data before building lines to avoid lifetime issues
    let version = env!("CARGO_PKG_VERSION").to_string();
    let session_id = if app_state.session_id.is_empty() {
        "new-session".to_string()
    } else {
        app_state.session_id.clone()
    };
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let (login_method, organization, email) = get_account_info();
    let model_display = format_model_display(&app_state.current_model);
    let memory_info = get_memory_info();
    let setting_sources = get_setting_sources();

    match app_state.status_view_tab {
        0 => {
            // Status tab content (matches JavaScript screenshot)
            let bold = Style::default().fg(Color::White).add_modifier(ratatui::style::Modifier::BOLD);
            let normal = Style::default().fg(Color::White);

            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("Version: ".to_string(), bold),
                ratatui::text::Span::styled(version.clone(), normal),
            ]));
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("Session ID: ".to_string(), bold),
                ratatui::text::Span::styled(session_id.clone(), normal),
            ]));
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("cwd: ".to_string(), bold),
                ratatui::text::Span::styled(cwd.clone(), normal),
            ]));
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("Login method: ".to_string(), bold),
                ratatui::text::Span::styled(login_method.clone(), normal),
            ]));
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("Organization: ".to_string(), bold),
                ratatui::text::Span::styled(organization.clone(), normal),
            ]));
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("Email: ".to_string(), bold),
                ratatui::text::Span::styled(email.clone(), normal),
            ]));
            lines.push(ratatui::text::Line::from(""));
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("Model: ".to_string(), bold),
                ratatui::text::Span::styled(model_display.clone(), normal),
            ]));
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("Memory:".to_string(), bold),
                ratatui::text::Span::styled(memory_info.clone(), normal),
            ]));
            lines.push(ratatui::text::Line::from(vec![
                ratatui::text::Span::styled("Setting sources: ".to_string(), bold),
                ratatui::text::Span::styled(setting_sources.clone(), normal),
            ]));
        }
        1 => {
            // Config tab content (matches JavaScript screenshot)
            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                "Configure Claude Code preferences",
                Style::default().fg(Color::Gray)
            )));
            lines.push(ratatui::text::Line::from(""));

            let config_items = get_config_items();
            for (i, (name, value)) in config_items.iter().enumerate() {
                let prefix = if i == app_state.status_config_selected { "❯ " } else { "  " };
                let style = if i == app_state.status_config_selected {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                };
                let value_style = if value == "true" {
                    Style::default().fg(Color::Cyan)
                } else if value == "false" {
                    Style::default().fg(Color::Gray)
                } else {
                    Style::default().fg(Color::Cyan)
                };

                // Create a formatted line with name left-aligned and value right-aligned
                let name_display = format!("{}{}", prefix, name);
                lines.push(ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled(format!("{:<50}", name_display), style),
                    ratatui::text::Span::styled(value.to_string(), value_style),
                ]));
            }
        }
        2 => {
            // Usage tab content - uses REAL data from app_state, NEVER hardcoded
            // Session usage: calculated from actual token count vs model limit
            let token_count = app_state.estimate_token_count();
            let model_limit = app_state.get_model_token_limit();
            let session_pct = ((token_count as f64 / model_limit as f64) * 100.0).min(100.0) as u8;

            // Extra usage setting from actual settings files
            let extra_usage_enabled = {
                let settings = crate::config::load_settings(crate::config::SettingsSource::User).ok();
                settings.and_then(|s| s.extra.get("extraUsage").and_then(|v| v.as_bool())).unwrap_or(false)
            };

            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                "Current session",
                Style::default().fg(Color::White).add_modifier(ratatui::style::Modifier::BOLD)
            )));
            lines.push(ratatui::text::Line::from(render_usage_bar(session_pct)));
            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                format!("{} / {} tokens used", token_count, model_limit),
                Style::default().fg(Color::Gray)
            )));
            lines.push(ratatui::text::Line::from(""));

            // Weekly usage data requires API integration - show actual status
            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                "Current week (all models)",
                Style::default().fg(Color::White).add_modifier(ratatui::style::Modifier::BOLD)
            )));
            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                "Weekly usage data requires API integration",
                Style::default().fg(Color::DarkGray)
            )));
            lines.push(ratatui::text::Line::from(""));

            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                "Current week (Sonnet only)",
                Style::default().fg(Color::White).add_modifier(ratatui::style::Modifier::BOLD)
            )));
            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                "Weekly usage data requires API integration",
                Style::default().fg(Color::DarkGray)
            )));
            lines.push(ratatui::text::Line::from(""));

            lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                "Extra usage",
                Style::default().fg(Color::White).add_modifier(ratatui::style::Modifier::BOLD)
            )));
            if extra_usage_enabled {
                lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                    "Extra usage enabled",
                    Style::default().fg(Color::Green)
                )));
            } else {
                lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                    "Extra usage not enabled • /extra-usage to enable",
                    Style::default().fg(Color::Gray)
                )));
            }
        }
        _ => {}
    }

    lines.push(ratatui::text::Line::from(""));
    let footer = if app_state.status_view_tab == 1 {
        "Enter/Space to change · Esc to cancel"
    } else {
        "Esc to cancel"
    };
    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        footer,
        Style::default().fg(Color::DarkGray)
    )));

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

/// Get account info for status view
fn get_account_info() -> (String, String, String) {
    // Try to get auth info from OAuth token
    let oauth_path = dirs::home_dir()
        .map(|h| h.join(".claude").join("oauth_token.json"))
        .unwrap_or_default();

    if oauth_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&oauth_path) {
            if let Ok(token_data) = serde_json::from_str::<serde_json::Value>(&content) {
                let account_type = token_data.get("accountType")
                    .and_then(|t| t.as_str())
                    .unwrap_or("Claude Account");
                let email = token_data.get("email")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown");
                let org = token_data.get("organization")
                    .or_else(|| token_data.get("organizationName"))
                    .and_then(|o| o.as_str())
                    .map(|o| format!("{}'s Organization", o))
                    .unwrap_or_else(|| format!("{}'s Organization", email));

                return (account_type.to_string(), org, email.to_string());
            }
        }
    }

    // Fall back to API key
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        return ("API Key".to_string(), "(using direct API key)".to_string(), "(not applicable)".to_string());
    }

    ("Not logged in".to_string(), "(none)".to_string(), "(none)".to_string())
}

/// Format model display string
fn format_model_display(model: &str) -> String {
    if model.contains("opus") {
        "Default Opus 4.5 · Most capable for complex work".to_string()
    } else if model.contains("sonnet") {
        "Sonnet 4 · Fast and efficient".to_string()
    } else if model.contains("haiku") {
        "Haiku 3.5 · Quick responses".to_string()
    } else {
        format!("{} · Custom model", model)
    }
}

/// Get memory info for status view
fn get_memory_info() -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let claude_md = cwd.join("CLAUDE.md");
    if claude_md.exists() {
        " project: CLAUDE.md".to_string()
    } else {
        "".to_string()
    }
}

/// Get setting sources for status view
fn get_setting_sources() -> String {
    let mut sources = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();

    if home.join(".claude").join("settings.json").exists() {
        sources.push("User settings");
    }
    if cwd.join(".claude").join("settings.json").exists() {
        sources.push("Project settings");
    }
    if cwd.join(".claude").join("settings.local.json").exists() {
        sources.push("Local settings");
    }

    if sources.is_empty() {
        "None".to_string()
    } else {
        sources.join(", ")
    }
}

/// Get config items for status view Config tab
/// Reads from actual settings files - NEVER hardcoded
fn get_config_items() -> Vec<(&'static str, String)> {
    // Load settings from all sources (user, project, local)
    let user_settings = crate::config::load_settings(crate::config::SettingsSource::User).ok();
    let project_settings = crate::config::load_settings(crate::config::SettingsSource::Project).ok();
    let local_settings = crate::config::load_settings(crate::config::SettingsSource::Local).ok();

    // Helper to get a value from settings hierarchy (local > project > user)
    let get_setting = |key: &str| -> String {
        // Check local first, then project, then user
        let all_settings = [&local_settings, &project_settings, &user_settings];
        for settings_opt in all_settings.iter() {
            if let Some(settings) = settings_opt {
                if let Some(value) = settings.extra.get(key) {
                    return match value {
                        serde_json::Value::Bool(b) => b.to_string(),
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Number(n) => n.to_string(),
                        _ => value.to_string(),
                    };
                }
            }
        }
        "Not set".to_string()
    };

    vec![
        ("Auto-compact", get_setting("autoCompact")),
        ("Show tips", get_setting("showTips")),
        ("Thinking mode", get_setting("thinkingMode")),
        ("Prompt suggestions", get_setting("promptSuggestions")),
        ("Rewind code (checkpoints)", get_setting("rewindCode")),
        ("Verbose output", get_setting("verboseOutput")),
        ("Terminal progress bar", get_setting("terminalProgressBar")),
        ("Default permission mode", get_setting("defaultPermissionMode")),
        ("Respect .gitignore in file picker", get_setting("respectGitignore")),
        ("Theme", get_setting("theme")),
        ("Notifications", get_setting("notifications")),
        ("Output style", get_setting("outputStyle")),
        ("Editor mode", get_setting("editorMode")),
        ("Model", get_setting("model")),
        ("Auto-connect to IDE (external terminal)", get_setting("autoConnectIDE")),
        ("Claude in Chrome enabled by default", get_setting("chromeExtension")),
        ("Use custom API key", get_setting("useCustomApiKey")),
    ]
}


/// Render usage bar for status view
fn render_usage_bar(percent: u8) -> Vec<ratatui::text::Span<'static>> {
    let bar_width = 40;
    let filled = (percent as usize * bar_width / 100).min(bar_width);
    let empty = bar_width - filled;

    let mut spans = vec![];
    spans.push(ratatui::text::Span::styled(
        "█".repeat(filled),
        Style::default().fg(Color::Blue)
    ));
    spans.push(ratatui::text::Span::styled(
        "░".repeat(empty),
        Style::default().fg(Color::DarkGray)
    ));
    spans.push(ratatui::text::Span::styled(
        format!(" {}% used", percent),
        Style::default().fg(Color::White)
    ));
    spans
}

/// Draw debug panel
fn draw_debug_panel(f: &mut Frame, area: Rect, app_state: &AppState) {
    let debug_info = vec![
        format!("Session ID: {}", app_state.session_id),
        format!("Model: {}", app_state.current_model),
        format!("Messages: {}", app_state.messages.len()),
        format!("Input Mode: {}", app_state.input_mode),
        format!("Processing: {}", app_state.is_processing),
        format!("MCP Servers: {}", app_state.mcp_servers.len()),
        format!("Active Tools: {}", app_state.active_tools.len()),
        "".to_string(),
        "Memory Usage:".to_string(),
        format!("  Heap: {}", crate::utils::format_bytes(get_memory_usage())),
        format!("  Messages: {}", crate::utils::format_bytes(app_state.get_message_memory())),
        "".to_string(),
        "Performance:".to_string(),
        format!("  FPS: {:.1}", app_state.get_fps()),
        format!("  Latency: {}ms", app_state.get_latency()),
    ];
    
    let debug_widget = Paragraph::new(debug_info.join("\n"))
        .block(
            Block::default()
                .title(" Debug ")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Yellow)),
        )
        .style(Style::default().fg(Color::Gray));
    
    f.render_widget(debug_widget, area);
}

/// Create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Load MCP servers from configuration
async fn load_mcp_servers(app_state: &mut AppState, config: &str) -> Result<()> {
    let servers = mcp::parse_config(config)?;
    
    for (name, server_config) in servers {
        match mcp::start_client(name.clone(), server_config).await {
            Ok(client) => {
                app_state.add_mcp_server(name, client);
            }
            Err(e) => {
                app_state.add_error(&format!("Failed to start MCP server {}: {}", name, e));
            }
        }
    }
    
    Ok(())
}

/// Get current memory usage
fn get_memory_usage() -> u64 {
    // This is a simplified implementation
    // In a real implementation, you'd use a crate like `sysinfo` or `procfs`
    0
}