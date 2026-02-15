use crate::error::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;

/// Event handler for TUI
pub struct EventHandler {
    tx: mpsc::UnboundedSender<AppEvent>,
    rx: mpsc::UnboundedReceiver<AppEvent>,
    stop_signal: Arc<AtomicBool>,
}

/// Application events
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Key press event
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Tick for updates
    Tick,
    /// External message
    Message(MessageEvent),
    /// Tool execution event
    ToolEvent(ToolEvent),
    /// Network event
    NetworkEvent(NetworkEvent),
    /// Error event
    Error(String),
    /// Shutdown signal
    Shutdown,
}

/// Message events
#[derive(Debug, Clone)]
pub enum MessageEvent {
    /// User input received
    UserInput(String),
    /// Assistant response
    AssistantResponse(String),
    /// System message
    SystemMessage(String),
    /// Tool output
    ToolOutput(String, serde_json::Value),
    /// Streaming chunk
    StreamingChunk(String),
    /// End of stream
    StreamEnd,
}

/// Tool events
#[derive(Debug, Clone)]
pub enum ToolEvent {
    /// Tool execution started
    Started {
        tool_name: String,
        input: serde_json::Value,
    },
    /// Tool execution progress
    Progress {
        tool_name: String,
        progress: f64,
        message: String,
    },
    /// Tool execution completed
    Completed {
        tool_name: String,
        output: serde_json::Value,
        duration_ms: u64,
    },
    /// Tool execution failed
    Failed {
        tool_name: String,
        error: String,
    },
    /// Tool requires permission
    PermissionRequired {
        tool_name: String,
        action: String,
        details: String,
    },
}

/// Network events
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// Connected to service
    Connected(String),
    /// Disconnected from service
    Disconnected(String),
    /// Connection error
    ConnectionError(String, String),
    /// Rate limit hit
    RateLimit {
        service: String,
        retry_after: u64,
    },
}

impl EventHandler {
    /// Create new event handler
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let stop_signal = Arc::new(AtomicBool::new(false));
        
        Self {
            tx,
            rx,
            stop_signal,
        }
    }
    
    /// Get event sender
    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self.tx.clone()
    }
    
    /// Get event receiver
    pub fn receiver(&mut self) -> &mut mpsc::UnboundedReceiver<AppEvent> {
        &mut self.rx
    }
    
    /// Start event loop
    pub fn start(&self) {
        let tx = self.tx.clone();
        let stop_signal = self.stop_signal.clone();
        
        // Spawn keyboard/mouse event handler
        tokio::spawn(async move {
            let tx = tx.clone();
            let stop_signal = stop_signal.clone();
            
            loop {
                if stop_signal.load(Ordering::Relaxed) {
                    break;
                }
                
                // Poll for events with timeout
                if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                    if let Ok(event) = event::read() {
                        match event {
                            Event::Key(key) => {
                                let _ = tx.send(AppEvent::Key(key));
                            }
                            Event::Mouse(mouse) => {
                                let _ = tx.send(AppEvent::Mouse(mouse));
                            }
                            Event::Resize(width, height) => {
                                let _ = tx.send(AppEvent::Resize(width, height));
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
        
        // Spawn tick handler
        let tx = self.tx.clone();
        let stop_signal = self.stop_signal.clone();
        
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(100));
            
            loop {
                if stop_signal.load(Ordering::Relaxed) {
                    break;
                }
                
                ticker.tick().await;
                let _ = tx.send(AppEvent::Tick);
            }
        });
    }
    
    /// Stop event handler
    pub fn stop(&self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        let _ = self.tx.send(AppEvent::Shutdown);
    }
    
    /// Handle key event and return action
    pub fn handle_key_event(key: KeyEvent) -> KeyAction {
        match (key.code, key.modifiers) {
            // Control combinations
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => KeyAction::Cancel,
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => KeyAction::Quit,
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => KeyAction::Quit,
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => KeyAction::ClearScreen,
            (KeyCode::Char('a'), KeyModifiers::CONTROL) => KeyAction::SelectAll,
            (KeyCode::Char('e'), KeyModifiers::CONTROL) => KeyAction::End,
            (KeyCode::Char('k'), KeyModifiers::CONTROL) => KeyAction::DeleteToEnd,
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => KeyAction::DeleteToStart,
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => KeyAction::DeleteWord,
            (KeyCode::Char('t'), KeyModifiers::CONTROL) => KeyAction::SwapChars,
            (KeyCode::Char('f'), KeyModifiers::CONTROL) => KeyAction::Find,
            (KeyCode::Char('g'), KeyModifiers::CONTROL) => KeyAction::ToggleDebug,
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => KeyAction::Refresh,
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => KeyAction::Save,
            (KeyCode::Char('z'), KeyModifiers::CONTROL) => KeyAction::Undo,
            (KeyCode::Char('y'), KeyModifiers::CONTROL) => KeyAction::Redo,
            (KeyCode::Char('n'), KeyModifiers::CONTROL) => KeyAction::NextItem,
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => KeyAction::PrevItem,
            (KeyCode::Char('?'), KeyModifiers::CONTROL) => KeyAction::Help,
            
            // Alt combinations
            (KeyCode::Char('b'), KeyModifiers::ALT) => KeyAction::WordLeft,
            (KeyCode::Char('f'), KeyModifiers::ALT) => KeyAction::WordRight,
            (KeyCode::Char('d'), KeyModifiers::ALT) => KeyAction::DeleteWord,
            
            // Navigation keys
            (KeyCode::Up, _) => KeyAction::Up,
            (KeyCode::Down, _) => KeyAction::Down,
            (KeyCode::Left, _) => KeyAction::Left,
            (KeyCode::Right, _) => KeyAction::Right,
            (KeyCode::Home, _) => KeyAction::Home,
            (KeyCode::End, _) => KeyAction::End,
            (KeyCode::PageUp, _) => KeyAction::PageUp,
            (KeyCode::PageDown, _) => KeyAction::PageDown,
            
            // Special keys
            (KeyCode::Enter, _) => KeyAction::Submit,
            (KeyCode::Tab, KeyModifiers::NONE) => KeyAction::Tab,
            (KeyCode::BackTab, _) => KeyAction::BackTab,
            (KeyCode::Tab, KeyModifiers::SHIFT) => KeyAction::BackTab,
            (KeyCode::Backspace, _) => KeyAction::Backspace,
            (KeyCode::Delete, _) => KeyAction::Delete,
            (KeyCode::Esc, _) => KeyAction::Escape,
            
            // Function keys
            (KeyCode::F(1), _) => KeyAction::F1,
            (KeyCode::F(2), _) => KeyAction::F2,
            (KeyCode::F(3), _) => KeyAction::F3,
            (KeyCode::F(4), _) => KeyAction::F4,
            (KeyCode::F(5), _) => KeyAction::F5,
            (KeyCode::F(6), _) => KeyAction::F6,
            (KeyCode::F(7), _) => KeyAction::F7,
            (KeyCode::F(8), _) => KeyAction::F8,
            (KeyCode::F(9), _) => KeyAction::F9,
            (KeyCode::F(10), _) => KeyAction::F10,
            (KeyCode::F(11), _) => KeyAction::F11,
            (KeyCode::F(12), _) => KeyAction::F12,
            
            // Regular characters
            (KeyCode::Char(c), KeyModifiers::NONE) => KeyAction::Char(c),
            (KeyCode::Char(c), KeyModifiers::SHIFT) => KeyAction::Char(c),
            
            // Default
            _ => KeyAction::None,
        }
    }
}

/// Key actions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyAction {
    // Navigation
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    WordLeft,
    WordRight,
    
    // Editing
    Char(char),
    Backspace,
    Delete,
    DeleteWord,
    DeleteToEnd,
    DeleteToStart,
    SwapChars,
    
    // Control
    Submit,
    Cancel,
    Quit,
    Tab,
    BackTab,
    Escape,
    
    // Selection
    SelectAll,
    Copy,
    Paste,
    Cut,
    
    // History
    Undo,
    Redo,
    
    // UI
    Help,
    ToggleDebug,
    ClearScreen,
    Refresh,
    Find,
    Save,
    
    // List navigation
    NextItem,
    PrevItem,
    
    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    
    // No action
    None,
}

/// Create event broadcaster for distributing events to multiple handlers
pub struct EventBroadcaster {
    subscribers: Vec<mpsc::UnboundedSender<AppEvent>>,
}

impl EventBroadcaster {
    /// Create new broadcaster
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
    }
    
    /// Subscribe to events
    pub fn subscribe(&mut self) -> mpsc::UnboundedReceiver<AppEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscribers.push(tx);
        rx
    }
    
    /// Broadcast event to all subscribers
    pub fn broadcast(&mut self, event: AppEvent) {
        self.subscribers.retain(|tx| {
            tx.send(event.clone()).is_ok()
        });
    }
    
    /// Number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

/// Event filter for selective event handling
pub struct EventFilter {
    key_filter: Option<Box<dyn Fn(&KeyEvent) -> bool + Send>>,
    message_filter: Option<Box<dyn Fn(&MessageEvent) -> bool + Send>>,
    tool_filter: Option<Box<dyn Fn(&ToolEvent) -> bool + Send>>,
}

impl EventFilter {
    /// Create new event filter
    pub fn new() -> Self {
        Self {
            key_filter: None,
            message_filter: None,
            tool_filter: None,
        }
    }
    
    /// Set key event filter
    pub fn with_key_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&KeyEvent) -> bool + Send + 'static,
    {
        self.key_filter = Some(Box::new(filter));
        self
    }
    
    /// Set message event filter
    pub fn with_message_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&MessageEvent) -> bool + Send + 'static,
    {
        self.message_filter = Some(Box::new(filter));
        self
    }
    
    /// Set tool event filter
    pub fn with_tool_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&ToolEvent) -> bool + Send + 'static,
    {
        self.tool_filter = Some(Box::new(filter));
        self
    }
    
    /// Check if event passes filters
    pub fn should_handle(&self, event: &AppEvent) -> bool {
        match event {
            AppEvent::Key(key) => {
                self.key_filter.as_ref().map_or(true, |f| f(key))
            }
            AppEvent::Message(msg) => {
                self.message_filter.as_ref().map_or(true, |f| f(msg))
            }
            AppEvent::ToolEvent(tool) => {
                self.tool_filter.as_ref().map_or(true, |f| f(tool))
            }
            _ => true,
        }
    }
}