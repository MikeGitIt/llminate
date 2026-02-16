use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, CodeBlockKind};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxSet, SyntaxReference};
use once_cell::sync::Lazy;

// Cache expensive syntax highlighting resources
static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(|| SyntaxSet::load_defaults_newlines());
static THEME_SET: Lazy<ThemeSet> = Lazy::new(|| ThemeSet::load_defaults());

/// Convert markdown to ratatui Text with proper styling
pub fn parse_markdown(content: &str) -> Text<'static> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    
    let parser = Parser::new_ext(content, options);
    let mut lines: Vec<Line> = Vec::new();
    let mut current_line: Vec<Span> = Vec::new();
    
    // Use cached syntax highlighting resources
    let syntax_set = &*SYNTAX_SET;
    let theme = &THEME_SET.themes["base16-ocean.dark"];
    
    // State tracking
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_content = String::new();
    let mut in_bold = false;
    let mut in_italic = false;
    let mut in_code = false;
    let mut list_depth: usize = 0;
    let mut in_list_item = false;
    
    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Emphasis => in_italic = true,
                Tag::Strong => in_bold = true,
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    code_lang = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        _ => String::new(),
                    };
                    code_content.clear();
                    
                    // Start new line for code block
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                Tag::List(_) => {
                    list_depth += 1;
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                Tag::Item => {
                    let indent = "  ".repeat(list_depth.saturating_sub(1));
                    current_line.push(Span::raw(indent));
                    current_line.push(Span::styled("• ", Style::default().fg(Color::Yellow)));
                    in_list_item = true;
                }
                Tag::Paragraph => {
                    // Don't push the line if we're starting a paragraph inside a list item
                    // This keeps the bullet point on the same line as the text
                    if !in_list_item && !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                Tag::Heading { level, .. } => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    
                    let style = match level {
                        pulldown_cmark::HeadingLevel::H1 => {
                            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                        }
                        pulldown_cmark::HeadingLevel::H2 => {
                            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
                        }
                        pulldown_cmark::HeadingLevel::H3 => {
                            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                        }
                        _ => Style::default().fg(Color::Yellow),
                    };
                    
                    current_line.push(Span::styled("", style));
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Emphasis => in_italic = false,
                TagEnd::Strong => in_bold = false,
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    
                    // Render code block with syntax highlighting
                    if !code_content.is_empty() {
                        lines.push(Line::from(vec![Span::styled(
                            "─".repeat(40),
                            Style::default().fg(Color::Gray),
                        )]));

                        if !code_lang.is_empty() {
                            lines.push(Line::from(vec![Span::styled(
                                format!(" {} ", code_lang),
                                Style::default().fg(Color::Cyan).add_modifier(Modifier::ITALIC),
                            )]));
                        }

                        // Apply syntax highlighting if language is recognized
                        if let Some(syntax) = syntax_set.find_syntax_by_token(&code_lang) {
                            highlight_code(&code_content, syntax, theme, &mut lines);
                        } else {
                            // Fallback: render as plain text with code styling
                            for line in code_content.lines() {
                                lines.push(Line::from(vec![Span::styled(
                                    line.to_string(),
                                    Style::default().fg(Color::Gray).bg(Color::Rgb(40, 40, 40)),
                                )]));
                            }
                        }

                        lines.push(Line::from(vec![Span::styled(
                            "─".repeat(40),
                            Style::default().fg(Color::Gray),
                        )]));
                    }
                    
                    code_content.clear();
                    code_lang.clear();
                }
                TagEnd::List(_) => {
                    list_depth = list_depth.saturating_sub(1);
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                TagEnd::Item => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    in_list_item = false;
                }
                TagEnd::Paragraph => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    // Add spacing after paragraph only if not in a list
                    if !in_list_item && !lines.is_empty() {
                        lines.push(Line::from(vec![]));
                    }
                    // Reset list item flag after paragraph ends
                    if in_list_item {
                        in_list_item = false;
                    }
                }
                TagEnd::Heading(_) => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    lines.push(Line::from(vec![])); // Add spacing after heading
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    code_content.push_str(&text);
                } else {
                    // Use White as default foreground color for visibility
                    let mut style = Style::default().fg(Color::White);

                    if in_bold {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    if in_italic {
                        style = style.add_modifier(Modifier::ITALIC);
                    }
                    if in_code {
                        style = style.fg(Color::Yellow).bg(Color::Rgb(40, 40, 40));
                    }

                    current_line.push(Span::styled(text.to_string(), style));
                }
            }
            Event::Code(code) => {
                current_line.push(Span::styled(
                    format!("`{}`", code),
                    Style::default().fg(Color::Yellow).bg(Color::Rgb(40, 40, 40)),
                ));
            }
            Event::SoftBreak | Event::HardBreak => {
                if !current_line.is_empty() {
                    lines.push(Line::from(current_line.clone()));
                    current_line.clear();
                }
            }
            _ => {}
        }
    }
    
    // Push any remaining content
    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }
    
    // Remove trailing empty lines
    while lines.last().map(|l| l.spans.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    
    Text::from(lines)
}

/// Apply syntax highlighting to code
fn highlight_code(code: &str, syntax: &SyntaxReference, theme: &Theme, lines: &mut Vec<Line<'static>>) {
    let mut highlighter = HighlightLines::new(syntax, theme);
    
    for line in code.lines() {
        let highlighted = highlighter.highlight_line(line, &*SYNTAX_SET);
        
        if let Ok(ranges) = highlighted {
            let mut spans = Vec::new();
            
            for (style, text) in ranges {
                let fg = Color::Rgb(
                    style.foreground.r,
                    style.foreground.g,
                    style.foreground.b,
                );
                
                let mut ratatui_style = Style::default().fg(fg);
                
                // Convert syntect font style to ratatui modifiers
                if style.font_style.contains(syntect::highlighting::FontStyle::BOLD) {
                    ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                }
                if style.font_style.contains(syntect::highlighting::FontStyle::ITALIC) {
                    ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                }
                if style.font_style.contains(syntect::highlighting::FontStyle::UNDERLINE) {
                    ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
                }
                
                spans.push(Span::styled(text.to_string(), ratatui_style));
            }
            
            lines.push(Line::from(spans));
        } else {
            // Fallback if highlighting fails
            lines.push(Line::from(vec![Span::styled(
                line.to_string(),
                Style::default().fg(Color::Gray),
            )]));
        }
    }
}