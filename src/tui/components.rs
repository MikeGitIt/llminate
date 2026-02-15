use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap},
};
use std::collections::HashMap;
use crate::tui::markdown::parse_markdown;

/// Chat view component
pub struct ChatView<'a> {
    messages: &'a [UiMessage],
    scroll_offset: usize,
    show_session_picker: bool,
    session_picker_selected: usize,
    expanded_view: bool,
    cached_lines: Option<&'a Vec<ratatui::text::Line<'static>>>,
    task_status: Option<&'a str>,
    spinner_char: &'a str,
    is_processing: bool,
    next_todo: Option<&'a str>,
}

impl<'a> ChatView<'a> {
    pub fn new(messages: &'a [UiMessage]) -> Self {
        Self {
            messages,
            scroll_offset: 0,
            show_session_picker: false,
            session_picker_selected: 0,
            expanded_view: false,
            cached_lines: None,
            task_status: None,
            spinner_char: "-",
            is_processing: false,
            next_todo: None,
        }
    }
    
    pub fn with_scroll(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }
    
    pub fn with_session_picker(mut self, show: bool, selected: usize) -> Self {
        self.show_session_picker = show;
        self.session_picker_selected = selected;
        self
    }
    
    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded_view = expanded;
        self
    }
    
    pub fn with_cached_lines(mut self, cached: &'a Vec<ratatui::text::Line<'static>>) -> Self {
        self.cached_lines = Some(cached);
        self
    }
    
    pub fn with_task_status(mut self, status: Option<&'a str>, spinner: &'a str, processing: bool) -> Self {
        self.task_status = status;
        self.spinner_char = spinner;
        self.is_processing = processing;
        self
    }
    
    pub fn with_next_todo(mut self, next_todo: Option<&'a str>) -> Self {
        self.next_todo = next_todo;
        self
    }
}

impl<'a> Widget for ChatView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = area;
        
        // Use cached lines if available, but always add task status if processing
        let mut all_lines = if let Some(cached) = self.cached_lines {
            cached.clone()
        } else {
            // Fallback to rebuilding if no cache
            // Virtual scrolling: estimate which messages might be visible
            let viewport_height = inner.height as usize;
            let mut all_lines: Vec<Line> = Vec::new();
            let mut current_line = 0;
            let start_line = self.scroll_offset;
            let end_line = start_line + viewport_height + 10; // Add buffer for safety
            
            for msg in self.messages.iter() {
                // Early exit if we've rendered enough lines past the viewport
                if current_line > end_line + 100 {
                    break;
                }
                
                match msg.role.as_str() {
                "user" => {
                    // Check if this is a command
                    if msg.content.starts_with('/') {
                        // Display command in cyan with continuation indicator
                        all_lines.push(Line::from(vec![
                            Span::styled(msg.content.clone(), Style::default().fg(Color::Cyan))
                        ]));
                        all_lines.push(Line::from(vec![
                            Span::raw("  ⎿  "),
                        ]));
                    } else {
                        // Regular user message - use White for visibility on dark terminals
                        for line in msg.content.lines() {
                            all_lines.push(Line::from(vec![
                                Span::styled(line.to_string(), Style::default().fg(Color::White))
                            ]));
                        }
                    }
                }
                "command_output" => {
                    // Command output - no dots, just indented
                    let lines: Vec<&str> = msg.content.lines().collect();
                    if lines.len() > 10 && !self.expanded_view {
                        // Show collapsed version with first few lines as preview
                        for line in lines.iter().take(3) {
                            // Check if this is a diff line and apply appropriate color
                            let style = if line.starts_with('+') && !line.starts_with("+++") {
                                Style::default().fg(Color::Green)
                            } else if line.starts_with('-') && !line.starts_with("---") {
                                Style::default().fg(Color::Red)
                            } else if line.starts_with("@@") {
                                Style::default().fg(Color::Cyan)
                            } else if line.starts_with("Updated ") && line.contains(" with ") && line.contains(" addition") {
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                            } else {
                                // Use White for visibility on dark terminals
                                Style::default().fg(Color::White)
                            };
                            
                            all_lines.push(Line::from(vec![
                                Span::raw("     "),
                                Span::styled(line.to_string(), style),
                            ]));
                        }
                        // Show collapse indicator
                        all_lines.push(Line::from(vec![
                            Span::raw("  ⎿  "),
                            Span::styled(format!("... {} more lines", lines.len() - 3), Style::default().fg(Color::DarkGray)),
                            Span::raw(" "),
                            Span::styled("(ctrl+r to expand)", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
                        ]));
                    } else {
                        // Show full output with diff coloring
                        for line in lines {
                            // Check if this is a diff line and apply appropriate color
                            let style = if line.starts_with('+') && !line.starts_with("+++") {
                                Style::default().fg(Color::Green)
                            } else if line.starts_with('-') && !line.starts_with("---") {
                                Style::default().fg(Color::Red)
                            } else if line.starts_with("@@") {
                                Style::default().fg(Color::Cyan)
                            } else if line.starts_with("Updated ") && line.contains(" with ") && line.contains(" addition") {
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default()
                            };
                            
                            all_lines.push(Line::from(vec![
                                Span::raw("     "), 
                                Span::styled(line.to_string(), style),
                            ]));
                        }
                    }
                }
                "assistant" => {
                    let dot = if cfg!(target_os = "macos") { "⏺" } else { "●" };
                    
                    // Check if this is a tool execution message
                    let is_tool_msg = msg.content.starts_with("[Executing tool:") || 
                                     msg.content.starts_with("**Result:**");
                    let dot_color = if is_tool_msg { Color::Green } else { Color::White };
                    
                    // For tool results, check if we need to collapse long output
                    if msg.content.starts_with("**Result:**") {
                        let lines: Vec<&str> = msg.content.lines().collect();
                        if lines.len() > 10 && !self.expanded_view {
                            // Show collapsed version with first few lines as preview
                            all_lines.push(Line::from(vec![
                                Span::styled(dot, Style::default().fg(Color::Green)),
                                Span::raw(" "),
                                Span::raw("**Result:**"),
                            ]));
                            
                            // Show first 3 lines as preview
                            for line in lines.iter().skip(1).take(3) {
                                all_lines.push(Line::from(vec![
                                    Span::raw("     "),
                                    // Use White for visibility on dark terminals
                                    Span::styled(line.to_string(), Style::default().fg(Color::White)),
                                ]));
                            }
                            
                            // Show collapse indicator
                            all_lines.push(Line::from(vec![
                                Span::raw("  ⎿  "),
                                Span::styled(format!("... {} more lines", lines.len() - 4), Style::default().fg(Color::DarkGray)),
                                Span::raw(" "),
                                Span::styled("(ctrl+r to expand)", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
                            ]));
                        } else {
                            // Show full result with dot
                            let text = parse_markdown(&msg.content);
                            let mut first_line = true;
                            for mut line in text.lines {
                                if first_line {
                                    let mut new_spans = vec![
                                        Span::styled(dot, Style::default().fg(dot_color)),
                                        Span::raw(" "),
                                    ];
                                    new_spans.extend(line.spans);
                                    all_lines.push(Line::from(new_spans));
                                    first_line = false;
                                } else {
                                    all_lines.push(line);
                                }
                            }
                        }
                    } else {
                        // Regular assistant message or tool execution message
                        let text = parse_markdown(&msg.content);
                        let mut first_line = true;
                        for mut line in text.lines {
                            if first_line {
                                let mut new_spans = vec![
                                    Span::styled(dot, Style::default().fg(dot_color)),
                                    Span::raw(" "),
                                ];
                                new_spans.extend(line.spans);
                                all_lines.push(Line::from(new_spans));
                                first_line = false;
                            } else {
                                all_lines.push(line);
                            }
                        }
                    }
                }
                "system" => {
                    let dot = if cfg!(target_os = "macos") { "⏺" } else { "●" };
                    let mut first_line = true;
                    for line in msg.content.lines() {
                        if first_line {
                            all_lines.push(Line::from(vec![
                                Span::styled(dot, Style::default().fg(Color::Yellow)),
                                Span::raw(" "),
                                Span::styled(line.to_string(), Style::default().fg(Color::Yellow))
                            ]));
                            first_line = false;
                        } else {
                            all_lines.push(Line::from(vec![
                                Span::raw("   "), // Indent continuation lines
                                Span::styled(line.to_string(), Style::default().fg(Color::Yellow))
                            ]));
                        }
                    }
                }
                "error" => {
                    let dot = if cfg!(target_os = "macos") { "⏺" } else { "●" };
                    let mut first_line = true;
                    for line in msg.content.lines() {
                        if first_line {
                            all_lines.push(Line::from(vec![
                                Span::styled(dot, Style::default().fg(Color::Red)),
                                Span::raw(" "),
                                Span::styled(line.to_string(), Style::default().fg(Color::Red))
                            ]));
                            first_line = false;
                        } else {
                            all_lines.push(Line::from(vec![
                                Span::raw("   "),
                                Span::styled(line.to_string(), Style::default().fg(Color::Red))
                            ]));
                        }
                    }
                }
                "paste_preview" => {
                    // Use White for visibility on dark terminals
                    for line in msg.content.lines() {
                        all_lines.push(Line::from(vec![
                            Span::styled(line.to_string(), Style::default().fg(Color::White))
                        ]));
                    }
                }
                _ => {
                    for line in msg.content.lines() {
                        all_lines.push(Line::from(vec![
                            Span::styled(line.to_string(), Style::default())
                        ]));
                    }
                }
            }
            
                // Track how many lines this message created
                current_line = all_lines.len();
            }
            
            
            all_lines
        };
        
        // Always add task status display if processing, even with cached lines
        if self.is_processing && self.task_status.is_some() {
            let status_text = self.task_status.unwrap_or("");
            all_lines.push(Line::from(vec![
                Span::styled(self.spinner_char, Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(status_text, Style::default().fg(Color::White)),
                Span::raw(" "),
                Span::styled("(esc to interrupt • ctrl+r to expand)", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
            ]));
        }
        
        // Always add next TODO display if there is one
        if let Some(next_todo) = self.next_todo {
            // Truncate long todo descriptions
            let todo_text = if next_todo.len() > 80 {
                format!("{}...", &next_todo[..77])
            } else {
                next_todo.to_string()
            };
            
            all_lines.push(Line::from(vec![
                Span::styled("⎿ Next: ", Style::default().fg(Color::Cyan)),
                Span::styled(todo_text, Style::default().fg(Color::White)),
            ]));
        }
        
        let text = Text::from(all_lines);
        
        let paragraph = Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .scroll((self.scroll_offset as u16, 0));
        
        paragraph.render(inner, buf);
    }
}


/// Status bar component
pub struct StatusBar<'a> {
    state: &'a crate::tui::state::AppState,
}

impl<'a> StatusBar<'a> {
    pub fn new(state: &'a crate::tui::state::AppState) -> Self {
        Self { state }
    }
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(20),  // Mode
                Constraint::Min(20),     // Model
                Constraint::Length(30),  // Session
                Constraint::Length(20),  // Status
            ])
            .split(area);
        
        // Mode indicator
        let mode = if self.state.input_mode {
            Span::styled("INPUT", Style::default().fg(Color::Green))
        } else if self.state.is_processing {
            Span::styled("PROCESSING", Style::default().fg(Color::Yellow))
        } else {
            Span::styled("READY", Style::default().fg(Color::Cyan))
        };
        
        Paragraph::new(mode).render(chunks[0], buf);
        
        // Model
        let model = format!("Model: {}", self.state.current_model);
        Paragraph::new(model)
            .style(Style::default().fg(Color::Gray))
            .render(chunks[1], buf);
        
        // Session ID
        let session = format!("Session: {}", &self.state.session_id[..8]);
        Paragraph::new(session)
            .style(Style::default().fg(Color::Gray))
            .render(chunks[2], buf);
        
        // Help hint
        let help = "Ctrl+? for help";
        Paragraph::new(help)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Right)
            .render(chunks[3], buf);
    }
}

/// Tool panel component
pub struct ToolPanel<'a> {
    tools: &'a HashMap<String, ToolInfo>,
    selected: Option<usize>,
}

impl<'a> ToolPanel<'a> {
    pub fn new(tools: &'a HashMap<String, ToolInfo>) -> Self {
        Self {
            tools,
            selected: None,
        }
    }
    
    pub fn with_selected(mut self, index: usize) -> Self {
        self.selected = Some(index);
        self
    }
}

impl<'a> Widget for ToolPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Available Tools ")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan));
        
        let inner = block.inner(area);
        block.render(area, buf);
        
        let items: Vec<ListItem> = self.tools
            .iter()
            .map(|(name, info)| {
                let content = vec![
                    Line::from(vec![
                        Span::styled(name, Style::default().fg(Color::Yellow)),
                        Span::raw(" - "),
                        Span::raw(&info.description),
                    ]),
                ];
                ListItem::new(content)
            })
            .collect();
        
        let list = List::new(items)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        
        Widget::render(list, inner, buf);
    }
}

/// UI Message structure (different from app::Message)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiMessage {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

/// Tool information
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

/// Progress indicator component
pub struct ProgressIndicator {
    message: String,
    progress: f64,
    indeterminate: bool,
}

impl ProgressIndicator {
    pub fn new(message: String) -> Self {
        Self {
            message,
            progress: 0.0,
            indeterminate: true,
        }
    }
    
    pub fn with_progress(mut self, progress: f64) -> Self {
        self.progress = progress.clamp(0.0, 1.0);
        self.indeterminate = false;
        self
    }
}

impl Widget for ProgressIndicator {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Blue));
        
        let inner = block.inner(area);
        block.render(area, buf);
        
        // Render message
        let message_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };
        
        Paragraph::new(self.message)
            .style(Style::default().fg(Color::White))
            .render(message_area, buf);
        
        // Render progress bar
        if inner.height >= 3 {
            let bar_area = Rect {
                x: inner.x,
                y: inner.y + 2,
                width: inner.width,
                height: 1,
            };
            
            if self.indeterminate {
                // Animated spinner for indeterminate progress
                let spinner = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
                let frame = (crate::utils::timestamp_ms() / 100) as usize % spinner.len();
                let spinner_char = &spinner[frame..frame + 1];
                
                Paragraph::new(format!("{} Working...", spinner_char))
                    .style(Style::default().fg(Color::Yellow))
                    .render(bar_area, buf);
            } else {
                // Determinate progress bar
                let filled = (self.progress * bar_area.width as f64) as u16;
                let empty = bar_area.width.saturating_sub(filled);
                
                let bar = format!(
                    "{}{}",
                    "█".repeat(filled as usize),
                    "░".repeat(empty as usize)
                );
                
                Paragraph::new(bar)
                    .style(Style::default().fg(Color::Green))
                    .render(bar_area, buf);
            }
        }
    }
}

/// Confirmation dialog component
pub struct ConfirmDialog {
    title: String,
    message: String,
    yes_text: String,
    no_text: String,
    selected: bool,
}

impl ConfirmDialog {
    pub fn new(title: String, message: String) -> Self {
        Self {
            title,
            message,
            yes_text: "Yes".to_string(),
            no_text: "No".to_string(),
            selected: false,
        }
    }
    
    pub fn with_options(mut self, yes: String, no: String) -> Self {
        self.yes_text = yes;
        self.no_text = no;
        self
    }
    
    pub fn with_selection(mut self, yes_selected: bool) -> Self {
        self.selected = yes_selected;
        self
    }
}

impl Widget for ConfirmDialog {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Yellow));
        
        let inner = block.inner(area);
        block.render(area, buf);
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),      // Message
                Constraint::Length(1),   // Spacer
                Constraint::Length(1),   // Buttons
            ])
            .split(inner);
        
        // Message
        Paragraph::new(self.message)
            .wrap(Wrap { trim: true })
            .render(chunks[0], buf);
        
        // Buttons
        let button_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(chunks[2]);
        
        let yes_style = if self.selected {
            Style::default().fg(Color::Black).bg(Color::Green)
        } else {
            Style::default().fg(Color::Green)
        };
        
        let no_style = if !self.selected {
            Style::default().fg(Color::Black).bg(Color::Red)
        } else {
            Style::default().fg(Color::Red)
        };
        
        Paragraph::new(format!("[ {} ]", self.yes_text))
            .style(yes_style)
            .alignment(Alignment::Center)
            .render(button_chunks[0], buf);
        
        Paragraph::new(format!("[ {} ]", self.no_text))
            .style(no_style)
            .alignment(Alignment::Center)
            .render(button_chunks[1], buf);
    }
}

/// Autocomplete dropdown component matching JavaScript implementation
pub struct AutocompleteDropdown<'a> {
    matches: &'a [crate::tui::state::AutocompleteMatch],
    selected_index: usize,
}

impl<'a> AutocompleteDropdown<'a> {
    pub fn new(matches: &'a [crate::tui::state::AutocompleteMatch], selected_index: usize) -> Self {
        Self {
            matches,
            selected_index,
        }
    }
}

impl Widget for AutocompleteDropdown<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.matches.is_empty() {
            return;
        }

        // Limit to max 10 items to match JavaScript
        let max_items = 10;
        let visible_matches = &self.matches[..self.matches.len().min(max_items)];
        
        let mut list_items = Vec::new();
        
        for (index, autocomplete_match) in visible_matches.iter().enumerate() {
            let cmd = &autocomplete_match.command;
            
            // Create main command line: /commandname (alias1, alias2)
            let mut spans = vec![
                Span::styled(
                    format!("/{}", cmd.name),
                    if index == self.selected_index {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::Cyan)
                    }
                ),
            ];
            
            // Add aliases if they exist - format: (alias1, alias2)
            if !cmd.aliases.is_empty() {
                let aliases_text = format!(" ({})", cmd.aliases.join(", "));
                spans.push(Span::styled(
                    aliases_text,
                    if index == self.selected_index {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }
                ));
            }
            
            let mut lines = vec![Line::from(spans)];
            
            // Add description line
            let description_style = if index == self.selected_index {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            
            lines.push(Line::from(vec![
                Span::styled(format!("  {}", cmd.description), description_style)
            ]));
            
            // Add argument hint line if it exists
            if let Some(hint) = &cmd.argument_hint {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {}", hint), description_style)
                ]));
            }
            
            list_items.push(ListItem::new(lines));
        }
        
        // Create the dropdown with border
        let list = List::new(list_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Commands ")
                    .style(Style::default().fg(Color::White))
            );
        
        list.render(area, buf);
    }
}