# Text Rendering and Markdown Specifications

## Overview
This document tracks the JavaScript implementation details for text rendering, markdown parsing, and syntax highlighting in the original application. These specifications guide the 1:1 Rust port.

## Current Status
- **Investigation Phase**: Analyzing JavaScript implementation
- **Target**: Match JavaScript text rendering behavior exactly

## JavaScript Implementation Analysis

### 1. Markdown Support
- **File Type Recognition**: The JS recognizes "text/markdown" mime type with extensions ["markdown", "md"]
- **Alternative Recognition**: Also recognizes "text/x-markdown" with extension ["mkd"]

### 2. Syntax Highlighting
- **Library Used**: highlight.js (hljs)
- **Key Features**:
  - Auto-detection of languages: `highlightAuto()`
  - Manual language specification: `highlight(language, code)`
  - CSS class prefix: "hljs-"
  - Language detection regex: `/\blang(?:uage)?-([\w-]+)\b/i`
  - No-highlight detection: `/^(no-?highlight)$/i`
- **Vue Integration**: Provides a Vue component called "highlightjs"
- **Configuration Options**:
  - `tabReplace`: Replace tabs in code
  - `useBR`: Use BR tags (deprecated)
  - `languages`: Specify available languages
  - `ignoreIllegals`: Ignore illegal syntax

### 3. Text Display Features (from user screenshots)
- **Working Features**:
  - Newline rendering (verified by user)
  - Bold text formatting (verified by user)
  - Tool execution display with formatted output
  
- **Not Working Features**:
  - Syntax highlighting for code blocks
  - Proper line wrapping
  - Color schemes matching JavaScript
  - Code block rendering

## Search Strategy
Need to search for:
1. How messages are formatted before display
2. Code block handling patterns
3. Color definitions and themes
4. Text wrapping logic
5. Markdown parsing libraries used

## Rust Implementation Plan

### Required Crates
Based on research, the following crates are recommended:
1. **tui-markdown**: Direct Ratatui integration with markdown support
   - Built-in pulldown-cmark parser
   - Syntax highlighting via syntect
   - Feature flag: `highlight-code` (enabled by default)
   - **WARNING**: Described as "experimental Proof of Concept"
   - **LIMITATION**: Not all markdown features are supported
   - **ISSUE**: Currently showing raw markdown syntax instead of rendering

2. **Alternative Solutions**:
   - **syntect-tui**: Lightweight translation layer between syntect and ratatui
   - **pulldown-cmark**: Direct markdown parsing (implement custom converter)
   - **comrak**: CommonMark + GFM parser with syntect plugin
   - **Dioxus TUI (Rink)**: HTML/CSS-based TUI (experimental)

3. **Supporting Crates**:
   - **syntect**: Syntax highlighting engine
   - **ansi-to-tui**: ANSI color conversion for terminal

### Implementation Steps
1. ~~Add tui-markdown dependency to Cargo.toml~~ (DONE but not working)
2. ~~Replace custom markdown parser with tui-markdown~~ (DONE but showing raw syntax)
3. **NEW PLAN**:
   - Remove tui-markdown (not working properly)
   - Add pulldown-cmark and syntect-tui dependencies
   - Implement custom markdown to ratatui Text converter
   - Use syntect-tui for syntax highlighting
4. Configure syntax highlighting themes
5. Implement proper text wrapping
6. Match JavaScript color scheme

### Current Issue
- tui-markdown is not rendering markdown, showing raw syntax instead
- Need to implement custom solution using pulldown-cmark and syntect directly

## Next Investigation Areas
- [ ] Search for message formatting functions
- [ ] Find color theme definitions
- [ ] Locate text wrapping logic
- [ ] Identify markdown library usage
- [ ] Find code block rendering logic