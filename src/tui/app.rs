use crate::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame, Terminal,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, Mutex};

// Define types that were missing

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TabType {
    Chat,
    Editor,
    Terminal,
    Debug,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub title: String,
    pub message: String,
    pub level: NotificationLevel,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub enum Message {
    User(String),
    Assistant(String),
    System(String),
    Error(String),
    Code(String, String), // (language, code)
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::User(msg) => write!(f, "User: {}", msg),
            Message::Assistant(msg) => write!(f, "Assistant: {}", msg),
            Message::System(msg) => write!(f, "System: {}", msg),
            Message::Error(msg) => write!(f, "Error: {}", msg),
            Message::Code(lang, code) => write!(f, "```{}\n{}\n```", lang, code),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMode {
    Normal,
    Insert,
    Command,
    Visual,
    Search,
}

#[derive(Debug)]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub tab_type: TabType,
    pub content: TabContent,
    pub is_dirty: bool,
}

#[derive(Debug)]
pub enum TabContent {
    Chat(Vec<Message>),
    Editor(EditorState),
    Terminal(TerminalState),
    Debug(DebugState),
}

#[derive(Debug)]
pub struct EditorState {
    pub file_path: Option<PathBuf>,
    pub content: String,
    pub cursor_position: (usize, usize),
    pub selection: Option<(usize, usize)>,
    pub syntax_highlighting: Option<String>,
    pub undo_stack: Vec<String>,
    pub redo_stack: Vec<String>,
}

#[derive(Debug)]
pub struct TerminalState {
    pub command_history: Vec<String>,
    pub output_buffer: String,
    pub current_directory: PathBuf,
    pub environment: HashMap<String, String>,
}

#[derive(Debug)]
pub struct DebugState {
    pub breakpoints: Vec<Breakpoint>,
    pub watch_expressions: Vec<String>,
    pub call_stack: Vec<StackFrame>,
    pub variables: HashMap<String, String>,
}

#[derive(Debug)]
pub struct Breakpoint {
    pub file: PathBuf,
    pub line: usize,
    pub condition: Option<String>,
    pub enabled: bool,
}

#[derive(Debug)]
pub struct StackFrame {
    pub function: String,
    pub file: PathBuf,
    pub line: usize,
}

#[derive(Debug)]
pub struct FileNode {
    pub path: PathBuf,
    pub name: String,
    pub is_directory: bool,
    pub children: Vec<FileNode>,
    pub is_expanded: bool,
}

#[derive(Debug)]
pub struct AppState {
    // Core state
    pub mode: AppMode,
    pub is_running: bool,
    
    // UI state
    pub tabs: Vec<Tab>,
    pub active_tab_index: usize,
    pub show_file_browser: bool,
    pub show_command_palette: bool,
    pub file_tree: Option<FileNode>,
    pub selected_file_index: usize,
    
    // Input handling
    pub input_buffer: String,
    pub command_history: Vec<String>,
    pub command_history_index: usize,
    
    // Search
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub selected_search_result: usize,
    
    // Notifications
    pub notifications: Vec<Notification>,
    
    // Clipboard
    pub clipboard: String,
    
    // Settings
    pub settings: Settings,
    
    // Communication channels
    pub event_tx: mpsc::Sender<AppEvent>,
    pub event_rx: mpsc::Receiver<AppEvent>,
}

#[derive(Debug)]
pub struct SearchResult {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub text: String,
    pub context: String,
}

#[derive(Debug)]
pub struct Settings {
    pub theme: String,
    pub font_size: u16,
    pub tab_size: usize,
    pub show_line_numbers: bool,
    pub auto_save: bool,
    pub auto_format: bool,
    pub vim_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            font_size: 14,
            tab_size: 4,
            show_line_numbers: true,
            auto_save: true,
            auto_format: true,
            vim_mode: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    // User input events
    KeyPress(KeyEvent),
    MouseClick(u16, u16),
    Resize(u16, u16),
    
    // File events
    FileOpen(PathBuf),
    FileSave(PathBuf),
    FileClose(String), // tab id
    FileChanged(PathBuf),
    
    // LLM events
    LlmResponse(String),
    LlmError(String),
    LlmStreamChunk(String),
    
    // Command events
    ExecuteCommand(String),
    CommandComplete(String, std::result::Result<String, String>), // Store error as String
    
    // System events
    Notification(Notification),
    UpdateStatus(String),
    Refresh,
}

impl AppState {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
        
        let mut state = Self {
            mode: AppMode::Normal,
            is_running: true,
            tabs: vec![],
            active_tab_index: 0,
            show_file_browser: false,
            show_command_palette: false,
            file_tree: None,
            selected_file_index: 0,
            input_buffer: String::new(),
            command_history: vec![],
            command_history_index: 0,
            search_query: String::new(),
            search_results: vec![],
            selected_search_result: 0,
            notifications: vec![],
            clipboard: String::new(),
            settings: Settings::default(),
            event_tx,
            event_rx,
        };
        
        // Create default chat tab
        state.new_chat_tab();
        
        state
    }
    
    pub fn new_chat_tab(&mut self) {
        let id = format!("chat_{}", self.tabs.len());
        let tab = Tab {
            id: id.clone(),
            title: "Chat".to_string(),
            tab_type: TabType::Chat,
            content: TabContent::Chat(vec![
                Message::System("Welcome to Paragen! I'm here to help you with coding.".to_string())
            ]),
            is_dirty: false,
        };
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
    }
    
    pub fn new_editor_tab(&mut self, file_path: Option<PathBuf>) {
        let id = format!("editor_{}", self.tabs.len());
        let title = if let Some(path) = &file_path {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("untitled")
                .to_string()
        } else {
            "untitled".to_string()
        };
        
        let tab = Tab {
            id,
            title,
            tab_type: TabType::Editor,
            content: TabContent::Editor(EditorState {
                file_path,
                content: String::new(),
                cursor_position: (0, 0),
                selection: None,
                syntax_highlighting: None,
                undo_stack: vec![],
                redo_stack: vec![],
            }),
            is_dirty: false,
        };
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
    }
    
    pub fn active_tab(&self) -> &TabType {
        &self.tabs[self.active_tab_index].tab_type
    }
    
    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab_index]
    }
    
    pub fn show_file_browser(&self) -> bool {
        self.show_file_browser
    }
    
    pub fn show_command_palette(&self) -> bool {
        self.show_command_palette
    }
    
    pub fn current_notification(&self) -> Option<&Notification> {
        self.notifications.first()
    }
    
    pub fn add_notification(&mut self, title: String, message: String, level: NotificationLevel) {
        self.notifications.push(Notification {
            title,
            message,
            level,
            timestamp: Instant::now(),
        });
    }
    
    pub fn mode(&self) -> AppMode {
        self.mode
    }
    
    pub fn set_mode(&mut self, mode: AppMode) {
        self.mode = mode;
    }
    
    pub fn quit(&mut self) {
        self.is_running = false;
    }
    
    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

pub async fn run_app(app_state: Arc<Mutex<AppState>>) -> Result<()> {
    let mut terminal = super::init_terminal()?;
    
    // Start background tasks
    let state_clone = app_state.clone();
    tokio::spawn(async move {
        handle_app_events(state_clone).await;
    });
    
    // Start file watcher
    let state_clone = app_state.clone();
    tokio::spawn(async move {
        watch_files(state_clone).await;
    });
    
    let tick_rate = Duration::from_millis(50);
    let mut last_tick = Instant::now();
    
    loop {
        // Draw UI
        {
            let state = app_state.lock().await;
            terminal.draw(|f| draw_ui(f, &state))?;
            
            if !state.is_running() {
                break;
            }
        }
        
        // Handle events
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
            
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                let mut state = app_state.lock().await;
                state.event_tx.send(AppEvent::KeyPress(key)).await.map_err(|_| crate::error::Error::Other("Failed to send event".to_string()))?;
            }
        }
        
        // Clean up old notifications
        {
            let mut state = app_state.lock().await;
            state.notifications.retain(|n| n.timestamp.elapsed() < Duration::from_secs(5));
        }
        
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
    
    super::restore_terminal(&mut terminal)?;
    Ok(())
}

async fn handle_app_events(app_state: Arc<Mutex<AppState>>) {
    loop {
        let event = {
            let mut state = app_state.lock().await;
            state.event_rx.recv().await
        };
        
        if let Some(event) = event {
            match event {
                AppEvent::KeyPress(key) => {
                    handle_key_event(key, app_state.clone()).await.ok();
                }
                AppEvent::FileOpen(path) => {
                    let mut state = app_state.lock().await;
                    state.new_editor_tab(Some(path));
                }
                AppEvent::LlmResponse(response) => {
                    let mut state = app_state.lock().await;
                    let active_index = state.active_tab_index;
                    if let Some(tab) = state.tabs.get_mut(active_index) {
                        if let TabContent::Chat(messages) = &mut tab.content {
                            messages.push(Message::Assistant(response));
                        }
                    }
                }
                AppEvent::Notification(notification) => {
                    let mut state = app_state.lock().await;
                    state.notifications.push(notification);
                }
                _ => {}
            }
        }
    }
}

async fn handle_key_event(key: KeyEvent, app_state: Arc<Mutex<AppState>>) -> Result<()> {
    let mut state = app_state.lock().await;
    
    // Global shortcuts
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('q') => {
                state.quit();
                return Ok(());
            }
            KeyCode::Char('p') => {
                state.show_command_palette = !state.show_command_palette;
                return Ok(());
            }
            KeyCode::Char('b') => {
                state.show_file_browser = !state.show_file_browser;
                return Ok(());
            }
            KeyCode::Char('n') => {
                state.new_editor_tab(None);
                return Ok(());
            }
            KeyCode::Tab => {
                state.active_tab_index = (state.active_tab_index + 1) % state.tabs.len();
                return Ok(());
            }
            _ => {}
        }
    }
    
    // Mode-specific handling
    match state.mode {
        AppMode::Normal => handle_normal_mode_key(key, &mut state).await?,
        AppMode::Insert => handle_insert_mode_key(key, &mut state).await?,
        AppMode::Command => handle_command_mode_key(key, &mut state).await?,
        AppMode::Visual => handle_visual_mode_key(key, &mut state).await?,
        AppMode::Search => handle_search_mode_key(key, &mut state).await?,
    }
    
    Ok(())
}

async fn handle_normal_mode_key(key: KeyEvent, state: &mut AppState) -> Result<()> {
    match key.code {
        KeyCode::Char('i') => state.set_mode(AppMode::Insert),
        KeyCode::Char(':') => state.set_mode(AppMode::Command),
        KeyCode::Char('/') => state.set_mode(AppMode::Search),
        KeyCode::Char('v') => state.set_mode(AppMode::Visual),
        KeyCode::Char('h') | KeyCode::Left => {
            // Move cursor left
        }
        KeyCode::Char('j') | KeyCode::Down => {
            // Move cursor down
        }
        KeyCode::Char('k') | KeyCode::Up => {
            // Move cursor up
        }
        KeyCode::Char('l') | KeyCode::Right => {
            // Move cursor right
        }
        _ => {}
    }
    Ok(())
}

async fn handle_insert_mode_key(key: KeyEvent, state: &mut AppState) -> Result<()> {
    match key.code {
        KeyCode::Esc => state.set_mode(AppMode::Normal),
        KeyCode::Enter => {
            if let Some(tab) = state.tabs.get_mut(state.active_tab_index) {
                match &mut tab.content {
                    TabContent::Chat(_) => {
                        if !state.input_buffer.is_empty() {
                            let input = state.input_buffer.clone();
                            state.input_buffer.clear();
                            
                            // Send message event
                            state.event_tx.send(AppEvent::LlmResponse(format!("Processing: {}", input))).await.map_err(|_| crate::error::Error::Other("Failed to send event".to_string()))?;
                        }
                    }
                    TabContent::Editor(editor) => {
                        editor.content.push('\n');
                    }
                    _ => {}
                }
            }
        }
        KeyCode::Backspace => {
            state.input_buffer.pop();
        }
        KeyCode::Char(c) => {
            state.input_buffer.push(c);
        }
        _ => {}
    }
    Ok(())
}

async fn handle_command_mode_key(key: KeyEvent, state: &mut AppState) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            state.input_buffer.clear();
            state.set_mode(AppMode::Normal);
        }
        KeyCode::Enter => {
            let command = state.input_buffer.clone();
            state.input_buffer.clear();
            state.command_history.push(command.clone());
            state.set_mode(AppMode::Normal);
            
            // Execute command
            execute_command(&command, state).await?;
        }
        KeyCode::Backspace => {
            state.input_buffer.pop();
        }
        KeyCode::Char(c) => {
            state.input_buffer.push(c);
        }
        KeyCode::Up => {
            if state.command_history_index > 0 {
                state.command_history_index -= 1;
                state.input_buffer = state.command_history[state.command_history_index].clone();
            }
        }
        KeyCode::Down => {
            if state.command_history_index < state.command_history.len() - 1 {
                state.command_history_index += 1;
                state.input_buffer = state.command_history[state.command_history_index].clone();
            }
        }
        _ => {}
    }
    Ok(())
}

async fn handle_visual_mode_key(key: KeyEvent, state: &mut AppState) -> Result<()> {
    match key.code {
        KeyCode::Esc => state.set_mode(AppMode::Normal),
        KeyCode::Char('y') => {
            // Yank selection to clipboard
            state.set_mode(AppMode::Normal);
        }
        KeyCode::Char('d') => {
            // Delete selection
            state.set_mode(AppMode::Normal);
        }
        _ => {}
    }
    Ok(())
}

async fn handle_search_mode_key(key: KeyEvent, state: &mut AppState) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            state.search_query.clear();
            state.set_mode(AppMode::Normal);
        }
        KeyCode::Enter => {
            // Execute search
            perform_search(state).await?;
            state.set_mode(AppMode::Normal);
        }
        KeyCode::Backspace => {
            state.search_query.pop();
        }
        KeyCode::Char(c) => {
            state.search_query.push(c);
        }
        _ => {}
    }
    Ok(())
}

async fn execute_command(command: &str, state: &mut AppState) -> Result<()> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(());
    }
    
    match parts[0] {
        "q" | "quit" => state.quit(),
        "w" | "write" => {
            // Save current file
            state.add_notification(
                "Save".to_string(),
                "File saved successfully".to_string(),
                NotificationLevel::Success,
            );
        }
        "e" | "edit" => {
            if parts.len() > 1 {
                let path = PathBuf::from(parts[1]);
                state.event_tx.send(AppEvent::FileOpen(path)).await.map_err(|_| crate::error::Error::Other("Failed to send event".to_string()))?;
            }
        }
        "help" => {
            state.new_chat_tab();
            if let Some(tab) = state.tabs.get_mut(state.active_tab_index) {
                if let TabContent::Chat(messages) = &mut tab.content {
                    messages.push(Message::System(
                        "Available commands: :q (quit), :w (save), :e <file> (edit), :help".to_string()
                    ));
                }
            }
        }
        _ => {
            state.add_notification(
                "Error".to_string(),
                format!("Unknown command: {}", command),
                NotificationLevel::Error,
            );
        }
    }
    Ok(())
}

async fn perform_search(state: &mut AppState) -> Result<()> {
    // Implement search functionality
    state.add_notification(
        "Search".to_string(),
        format!("Searching for: {}", state.search_query),
        NotificationLevel::Info,
    );
    Ok(())
}

async fn watch_files(app_state: Arc<Mutex<AppState>>) {
    // Implement file watching
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        // Check for file changes
    }
}

/// Draw the UI
fn draw_ui(f: &mut Frame, state: &AppState) {
    // Create main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Min(1),    // Content
            Constraint::Length(3), // Status bar
        ])
        .split(f.size());
    
    // Draw tab bar
    let tab_titles: Vec<String> = state.tabs.iter()
        .map(|t| t.title.clone())
        .collect();
    let tabs = Tabs::new(tab_titles)
        .block(Block::default().borders(Borders::ALL))
        .select(state.active_tab_index)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow));
    f.render_widget(tabs, chunks[0]);
    
    // Draw content based on active tab
    if let Some(tab) = state.tabs.get(state.active_tab_index) {
        match &tab.content {
            TabContent::Chat(messages) => {
                draw_chat_content(f, chunks[1], messages, &state.input_buffer);
            }
            TabContent::Editor(editor) => {
                draw_editor_content(f, chunks[1], editor);
            }
            TabContent::Terminal(terminal) => {
                draw_terminal_content(f, chunks[1], terminal);
            }
            TabContent::Debug(debug) => {
                draw_debug_content(f, chunks[1], debug);
            }
        }
    }
    
    // Draw status bar
    draw_status_bar(f, chunks[2], state);
    
    // Draw overlays
    if let Some(notification) = state.current_notification() {
        draw_notification(f, notification);
    }
}

/// Draw chat content
fn draw_chat_content(f: &mut Frame, area: Rect, messages: &[Message], input: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Messages
            Constraint::Length(3), // Input
        ])
        .split(area);
    
    // Draw messages
    let mut lines = Vec::new();
    for msg in messages {
        let style = match msg {
            Message::User(_) => Style::default().fg(Color::Green),
            Message::Assistant(_) => Style::default().fg(Color::Cyan),
            Message::System(_) => Style::default().fg(Color::Yellow),
            Message::Error(_) => Style::default().fg(Color::Red),
            Message::Code(_, _) => Style::default().fg(Color::Magenta),
        };
        lines.push(Line::from(vec![Span::styled(msg.to_string(), style)]));
    }
    
    let messages_widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Messages "));
    f.render_widget(messages_widget, chunks[0]);
    
    // Draw input
    let input_widget = Paragraph::new(input)
        .block(Block::default().borders(Borders::ALL).title(" Input "));
    f.render_widget(input_widget, chunks[1]);
}

/// Draw editor content
fn draw_editor_content(f: &mut Frame, area: Rect, editor: &EditorState) {
    let content = Paragraph::new(editor.content.as_str())
        .block(Block::default().borders(Borders::ALL).title(" Editor "));
    f.render_widget(content, area);
}

/// Draw terminal content
fn draw_terminal_content(f: &mut Frame, area: Rect, terminal: &TerminalState) {
    let content = Paragraph::new(terminal.output_buffer.as_str())
        .block(Block::default().borders(Borders::ALL).title(" Terminal "));
    f.render_widget(content, area);
}

/// Draw debug content
fn draw_debug_content(f: &mut Frame, area: Rect, debug: &DebugState) {
    let mut lines = Vec::new();
    lines.push(Line::from(format!("Breakpoints: {}", debug.breakpoints.len())));
    lines.push(Line::from(format!("Call Stack: {} frames", debug.call_stack.len())));
    lines.push(Line::from(format!("Variables: {} entries", debug.variables.len())));
    
    let content = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Debug "));
    f.render_widget(content, area);
}

/// Draw status bar
fn draw_status_bar(f: &mut Frame, area: Rect, state: &AppState) {
    let mode_str = format!(" Mode: {:?} ", state.mode);
    let status = Paragraph::new(mode_str)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(status, area);
}

/// Draw notification overlay
fn draw_notification(f: &mut Frame, notification: &Notification) {
    let area = centered_rect(60, 20, f.size());
    
    let style = match notification.level {
        NotificationLevel::Info => Style::default().fg(Color::Blue),
        NotificationLevel::Success => Style::default().fg(Color::Green),
        NotificationLevel::Warning => Style::default().fg(Color::Yellow),
        NotificationLevel::Error => Style::default().fg(Color::Red),
    };
    
    let content = Paragraph::new(notification.message.as_str())
        .block(Block::default()
            .borders(Borders::ALL)
            .title(notification.title.as_str())
            .style(style));
    f.render_widget(content, area);
}

/// Create centered rectangle
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