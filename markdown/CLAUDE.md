# CLAUDE.md - Project Context for Claude Code

## CRITICAL: Start of Every Session
**MANDATORY FIRST STEP**: At the beginning of EVERY new session or refresh, you MUST:
1. Read `claude-rules.md` file completely
2. Acknowledge and follow ALL rules throughout the session
3. Never deviate from these rules unless explicitly instructed by the user

## Project Overview
This is a complete Rust port of test-fixed.js (270,562 lines of minified JavaScript) - a production-grade agentic coding assistant similar to Claude Code. The goal is to remove ALL JavaScript dependencies and create a fully functional Rust implementation.

## CRITICAL: JavaScript Code is Obfuscated and Minified
**The test-fixed.js file is heavily obfuscated and minified:**
- Variable names are mangled (e.g., `input20325`, `config8199`, `value2417`)
- Function names are obfuscated (e.g., `stringDecoder532`, `CqB`)
- Tool names don't appear as literal strings (e.g., "BashOutput" becomes part of obfuscated code)
- Simple grep searches will often fail - need to search by:
  - Functionality patterns
  - Parameter descriptions
  - Error messages or user-facing strings
  - Schema definitions
- Use commands like `gawk`, `sed` for complex pattern matching
- Search for partial strings and context clues, not exact tool names

## Critical Development Rules

### 1. ABSOLUTE REQUIREMENTS
- **FULL IMPLEMENTATION AT ALL TIMES** - NO shortcuts, workarounds, or stub code
- **NO LAZY ANALYSIS** - Thoroughly analyze ALL code before making changes
- **NO DECEPTION** - Never claim something is complete when it's not fully implemented
- **VERIFY EVERYTHING** - Always check actual JavaScript behavior, never assume
- **TEST ACTUAL FUNCTIONALITY** - Compilation success ≠ working correctly
- **NEVER CLAIM CODE WORKS WITHOUT PROOF** - Never say code "works", "should work", "is fixed", or any variant without actual verification
- **FOLLOW EXACT INSTRUCTIONS** - You MUST follow the EXACT instructions given by the user, do not deviate or add your own interpretations
- **NEVER MAKE UP DATA** - You MUST NEVER fabricate data, statuses, line numbers, or implementation details. When asked for initial states, use the specified default values (e.g., "Not Started" for tracking documents)
- **WRITE REALISTIC TESTS** - Follow Rust best practices for tests. NO toy tests. Test the actual functionality/system under test
- **PROVIDE TUI TEST STEPS** - If something can only be tested in the TUI, provide detailed test steps for the user to verify
- **NEVER ASSUME CRATE APIs** - DO NOT EVER ASSUME THE API OF ANY CRATE. ALWAYS verify unknown details by checking the documentation first. NEVER make assumptions about method names, parameters, or behavior without verification

### 2. Code Quality Standards
- **NEVER use `.unwrap()`** - Always use proper error handling with `?` operator
- **NEVER use `panic!()`** - This is production code
- **NEVER use `todo!()` or `unimplemented!()`** - Implement everything fully
- **ALWAYS add context to errors** - Use `.context()` from anyhow
- **ALWAYS USE IDIOMATIC RUST** - Use snake_case for fields/variables, PascalCase for types, proper ownership, borrowing, and lifetimes
- **NEVER USE camelCase** - Use snake_case with `#[serde(rename = "...")]` for JSON field mapping

### 3. Implementation Process
1. **READ the JavaScript carefully** - The code is minified/obfuscated
2. **SEARCH by context** - Tool names are obfuscated, search by functionality
3. **VERIFY parameters** - Check actual schemas in JavaScript, not assumptions
4. **TEST comprehensively** - Unit tests AND integration with the full application
5. **DOCUMENT findings** - Note any deviations or enhancements

### 4. CRITICAL: Rust Enhancements Policy
**WHEREVER enhancements can be made due to using Rust, THEY SHOULD BE ADDED.**

This is a Rust implementation, not a 1:1 JavaScript clone. While core behavior and parameters must match JavaScript for compatibility, Rust's capabilities should be leveraged to provide enhancements:

- **Extended format support** - If Rust can handle more file types, image formats, etc., add them
- **Better error messages** - Rust's type system enables more precise error handling
- **Performance improvements** - Use Rust's zero-cost abstractions where beneficial
- **Additional safety checks** - Leverage Rust's safety guarantees
- **Enhanced functionality** - If a feature can be improved without breaking compatibility, do it

**Rules for enhancements:**
1. Core parameters and behavior MUST match JavaScript (for API compatibility)
2. Enhancements MUST NOT break existing functionality
3. All enhancements MUST be documented in this file under the relevant tool section
4. Mark enhancements clearly with "**Rust Enhancement**" in comments and documentation

## CRITICAL: TWO DIFFERENT AI CLIENT IMPLEMENTATIONS

**THIS IS KNOWN AND DOCUMENTED - DO NOT "REDISCOVER" THIS!**

There are TWO separate AI client implementations in this codebase:

### 1. `src/ai/client.rs` - `AIClient`
- **HAS proper OAuth handling** with `x-app: cli` header
- **HAS** `?beta=true` URL parameter for OAuth
- **HAS** `anthropic-beta: claude-code-20250219,oauth-2025-04-20` header
- **HAS** all X-Stainless headers matching JavaScript SDK
- **NOT currently used by the application**

### 2. `src/auth/client.rs` - `AnthropicClient`
- Uses SDK-like architecture with `build_headers()`, `post()`, etc.
- **MISSING** some OAuth-specific headers in `build_headers()`
- **IS wrapped by `AIClientAdapter`** in `src/ai/client_adapter.rs`
- **IS the client actually used** by `create_client()` in `src/ai/mod.rs`

### Current Flow:
```
create_client() -> AIClientAdapter -> AnthropicClient (auth/client.rs)
                   NOT using AIClient (ai/client.rs)!
```

### The Problem:
- `AIClient` has correct OAuth implementation but ISN'T USED
- `AnthropicClient` IS USED but has incomplete OAuth headers
- Print mode uses `AnthropicClient::chat()` -> SDK path
- TUI mode uses `AnthropicClient::chat_stream()` -> direct HTTP path

### Solution Options:
1. **Option A**: Make `create_client()` return `AIClient` instead of `AIClientAdapter`
2. **Option B**: Copy OAuth header logic from `AIClient` to `AnthropicClient::build_headers()`

**DO NOT waste time "discovering" this architecture again. It is KNOWN.**

## Project Structure

```
output/
├── src/
│   ├── ai/
│   │   ├── tools.rs          # Core tool implementations
│   │   ├── client.rs         # AI client interface
│   │   ├── conversation.rs   # Conversation management
│   │   └── streaming.rs      # Streaming responses
│   ├── tui/
│   │   ├── app.rs           # Main TUI application
│   │   ├── components.rs    # UI components
│   │   └── state.rs         # Application state
│   ├── cli.rs               # Command-line interface
│   ├── config.rs            # Configuration management
│   ├── error.rs             # Error types
│   └── main.rs              # Entry point
├── tests/
│   ├── test_bash_tool_advanced.rs
│   ├── test_file_edit_tool.rs
│   └── test_file_multi_edit_tool.rs
├── test-fixed.js            # Source JavaScript (270,562 lines)
├── claude-rules.md          # Development rules (MUST READ)
├── ARCHITECTURE.md          # Architecture decisions
├── CONTINUATION.md          # Session continuation notes
└── CLAUDE.md               # This file
```

## Tool Implementation Status

### ✅ Completed and Verified Tools

#### 1. Read (FileRead)
- **Parameters**: `file_path`, `offset`, `limit`
- **Key Fix**: Was using `start_line`/`end_line`, corrected to `offset`/`limit`
- **JavaScript Parity**: Full parity with JavaScript implementation
- **Rust Enhancements** (beyond JavaScript):
  - **Extended Image Support**: Supports `bmp`, `ico`, `tiff`, `tif`, `heic`, `heif`, `avif` in addition to JS's `png`, `jpg`, `jpeg`, `gif`, `webp`
  - **SVG as Text**: SVG files are correctly read as text (XML-based), not as binary images
- **Features matching JavaScript**:
  - Binary file extension rejection with explicit error message
  - Empty image file rejection
  - Empty file warning with `<system-reminder>` format
  - Offset out-of-range warning with line count info
  - Malware warning suffix appended to file content
  - File not found with current working directory info and similar file suggestions
- **Verification**: Tested with unit tests

#### 2. Write (FileWrite)
- **Parameters**: `file_path`, `content`
- **Key Fix**: Was using `path`, corrected to `file_path`
- **Verification**: Tested with unit tests

#### 3. Edit (FileEdit)
- **Parameters**: `file_path`, `old_string`, `new_string`, `replace_all`
- **Key Fix**: Parameter names were wrong (`path` → `file_path`, etc.)
- **Verification**: Comprehensive test suite created and passing

#### 4. MultiEdit (FileMultiEdit)
- **Parameters**: `file_path`, `edits` (array of edit objects)
- **Key Fix**: Corrected parameter names to match JavaScript
- **Verification**: Full test suite with sequential edit testing

#### 5. Bash
- **Parameters**: `command`, `timeout`, `description`, `run_in_background`, `dangerouslyDisableSandbox`
- **Critical Fix (2025-01-20)**:
  - Was using `sandbox` parameter with INVERTED logic (sandbox OFF by default - UNSAFE!)
  - Corrected to `dangerouslyDisableSandbox` matching JavaScript (sandbox ON by default - SAFE)
- **Dual-mode implementation**:
  - **Basic mode** (default): Only persists `pwd` - matches JavaScript exactly
  - **Advanced mode**: Full shell persistence (enhancement via `advanced_persistence` flag)
- **Rust Enhancements** (beyond JavaScript):
  - `shellExecutable`: Custom shell path for testing
  - `working_dir`: Explicit working directory
  - `env`: Environment variables object
  - `shell_id`: Persistent shell session ID
  - `stream`: Output streaming option
  - `advanced_persistence`: Full environment persistence mode
- **Key Discoveries**:
  - JavaScript only persists working directory, NOT environment variables
  - Must handle readonly variables (PPID, UID, EUID, IFS, etc.)
  - Uses `additionalWorkingDirectories` for permission management
- **JavaScript Parity**: Full parity with core parameters
- **Verification**: 11 comprehensive tests all passing

#### 6. NotebookRead
- **Parameters**: `notebook_path`
- **Key Fix**: Matches JavaScript's XML-like output format from `stringDecoder241`
- **Verification**: Tested with unit tests

#### 7. NotebookEdit
- **Parameters**: `notebook_path`, `cell_id`, `new_source`, `cell_type`, `edit_mode`
- **Key Fix**: Exact JavaScript parity including bug where numeric indices fail
- **Verification**: Full test suite with 11 tests all passing

#### 8. Grep
- **Parameters**: `pattern`, `path`, `include` (plus enhanced features)
- **Key Fix**: Added `include` parameter to match JavaScript exactly
- **JavaScript Parity**: Full parity with core functionality
- **Enhancements**: Additional `glob`, `type`, `output_mode`, context flags
- **Verification**: 5 comprehensive tests all passing

#### 9. Glob
- **Parameters**: `pattern`, `path`
- **Implementation**: File pattern matching with glob patterns
- **Verification**: Implemented, needs testing

#### 10. LS (ListFilesTool)
- **Parameters**: `path`, `ignore`
- **Implementation**: Directory listing with optional ignore patterns
- **Verification**: Implemented in tools.rs

#### 11. Search (SearchFilesTool)
- **Parameters**: `path`, `pattern`, `file_pattern`
- **Implementation**: Text search within files
- **Verification**: Implemented in tools.rs

#### 12. HttpRequest
- **Parameters**: `url`, `method`, `headers`, `body`
- **Implementation**: HTTP request functionality
- **Verification**: Implemented in tools.rs

#### 13. Task (AgentTool)
- **Implementation**: Sub-agent task execution
- **Location**: agent_tool.rs
- **Verification**: Implemented with comprehensive agent system

#### 14. TodoWrite & TodoRead
- **Implementation**: Task list management
- **Location**: todo_tool.rs
- **Verification**: Implemented with full task tracking

#### 15. WebFetch
- **Parameters**: `url`, `prompt`
- **Implementation**: Fetch and process web content
- **Location**: web_tools.rs
- **Verification**: Implemented with caching and security checks

#### 16. WebSearch
- **Parameters**: `query`, `allowed_domains`, `blocked_domains`
- **Implementation**: Web search functionality
- **Location**: web_tools.rs
- **Verification**: Implemented

#### 17. BashOutput (TaskOutput)
- **Parameters**: `task_id`, `block`, `timeout`, `filter` (Rust enhancement)
- **Critical Fix (2025-01-20)**:
  - Was using `bash_id` parameter - corrected to `task_id` to match JavaScript
  - Added missing `block` (default: true) and `timeout` (default: 30000, max: 600000) parameters
- **JavaScript Parity**: Full parity with core parameters
- **Rust Enhancement**: `filter` parameter for regex filtering of output

#### 18. KillBash
- **Parameters**: `shell_id`
- **JavaScript Parity**: Full parity
- **Verification**: Matches JavaScript exactly

### ✅ Critical Fixes Applied (2025-01-20)

#### add-dir Permission System - FULLY IMPLEMENTED
- **Issue**: `options.add_dirs` from CLI was NEVER processed in `AppState::new()`
- **Files Modified**:
  - `src/config.rs` - Added `PermissionsConfig`, `Settings`, `SettingsSource` structs
  - `src/tui/state.rs` - Fixed CLI processing and `/add-dir` slash command

- **Features Implemented (matching JavaScript)**:
  1. **CLI --add-dir processing**: Directories passed via CLI are now added to both `working_directories` and `PERMISSION_CONTEXT`
  2. **Settings persistence**: Supports JavaScript settings file structure:
     - `~/.claude/settings.json` (user settings)
     - `.claude/settings.json` (project settings, shared)
     - `.claude/settings.local.json` (local settings, gitignored)
  3. **Slash command with persistence flags**:
     - `/add-dir <path>` - session only (default, matches JavaScript)
     - `/add-dir <path> --persist` or `--local` - saves to `.claude/settings.local.json`
     - `/add-dir <path> --user` - saves to `~/.claude/settings.json`
  4. **Startup loading**: Directories from all settings files are loaded at startup

- **Rust Enhancements**:
  - Cleaner flag-based persistence syntax (`--persist`, `--local`, `--user`) vs JavaScript's dialog-based approach
  - More explicit control over where settings are saved

### ❌ Pending Tools (Found in JavaScript, Not Yet Implemented)

1. **MCP tools** - Model Context Protocol tools (16 commands)
   - mcp serve, add, remove, list, get, add-json, add-from-claude-desktop, reset-project-choices
   - mcp servers, tools, info, call, grep, resources, read

2. **Plugin tools** - Plugin management (10 commands)
   - plugin validate, marketplace add/list/remove/update, install, uninstall, enable, disable, update

3. **System tools** - setup-token, doctor, update, install

## CRITICAL DEBUGGING LIMITATION

**NEVER ADD `eprintln!()` OR `println!()` TO TUI CODE**
- The TUI application captures stdout/stderr, so print statements won't be visible
- We added the `tracing` crate to Cargo.toml but never implemented proper logging
- The JavaScript tool has a `--debug` flag that we only stubbed out
- Without proper debug logging, we cannot troubleshoot critical issues like:
  - File writes failing silently
  - Tool execution problems
  - Permission system issues
- **MUST IMPLEMENT**: Debug console/logging system before continuing bug fixes

## Key Technical Discoveries

### 1. JavaScript Bash Tool Behavior
- Only maintains working directory between commands
- Does NOT persist environment variables (contrary to initial assumptions)
- Has a permission system for directories via `additionalWorkingDirectories`
- Presents interactive prompts when accessing new directories

### 2. Tool Parameter Schemas
- Found by searching for schema definitions in minified code
- Names don't match the tool names due to obfuscation
- Must verify each parameter name against actual JavaScript

### 3. Shell Session Management
```rust
pub struct ShellSessionState {
    working_dir: PathBuf,
    original_working_dir: PathBuf,
    shell_executable: String,
    is_sandboxed: bool,
    env_vars: HashMap<String, String>,  // Only used in advanced mode
    advanced_persistence: bool,
    additional_working_directories: HashSet<PathBuf>,
}
```

### 4. Error Handling Patterns
- Filter readonly shell variables to prevent errors
- Use `2>/dev/null || true` for commands that might fail
- Handle IFS variable specially to prevent parsing issues

## Testing Guidelines

### Unit Tests
```bash
# Run all tests
cargo test

# Run specific tool tests with output
cargo test test_bash_tool -- --nocapture
cargo test test_file_edit_tool -- --nocapture
cargo test test_file_multi_edit_tool -- --nocapture
```

### Integration Testing
```bash
# Run the application
cargo run

# Test with specific commands
echo "test command" | cargo run
```

### Verification Process
1. Write comprehensive unit tests
2. Test edge cases and error conditions
3. Run in the actual application
4. Verify behavior matches JavaScript exactly (unless documented enhancement)

## Common Pitfalls to Avoid

1. **Assuming tool behavior** - Always verify against JavaScript
2. **Using wrong parameter names** - Check actual schemas
3. **Incomplete error handling** - Handle ALL error cases
4. **Not testing actual functionality** - Compilation ≠ working
5. **Adding unnecessary features** - Match JavaScript unless explicitly enhancing

## Development Workflow

1. **Start each session**:
   - Read `claude-rules.md`
   - Read this `CLAUDE.md`
   - Review `CONTINUATION.md` for current status

2. **For each tool**:
   - Search JavaScript for implementation
   - Verify parameter schemas
   - Implement with full functionality
   - Write comprehensive tests
   - Test in actual application

3. **Before claiming completion**:
   - All tests pass
   - No `unwrap()` or `todo!()`
   - Tested with real inputs
   - Behavior matches JavaScript

## User Expectations

The user has **ZERO TOLERANCE** for:
- Deceptive claims about completion
- Lazy implementations
- Shortcuts or workarounds
- Untested code
- False positives

The user **EXPECTS**:
- Full implementations first time
- Thorough testing before claims
- Honest status updates
- Production-quality code
- Complete functionality

## Important Commands

```bash
# Lint and typecheck (if available)
cargo clippy
cargo check

# Build release version
cargo build --release

# Run with verbose output
RUST_LOG=debug cargo run

# Clean and rebuild
cargo clean && cargo build

# Granular search in minified JavaScript (example finding prompts)
for file in $(find ~/Code/rust_projects/paragen/output/ -type f -name "test-fixed.js");do gawk 'match($0,/[P-p]rompt:.*/,arr) {print substr(RSTART,RLENGTH) $0}' $file;done

# Search for specific patterns in obfuscated code
gawk 'match($0,/tool_use|ToolUse|tool_result/,arr) {print NR": "substr($0,RSTART-20,RLENGTH+40)}' test-fixed.js
```

## CRITICAL: TUI Testing Limitations

**I (Claude) CANNOT test the TUI interactively. ONLY THE USER can test TUI functionality.**

- The TUI requires interactive terminal input/output which I cannot perform
- I can only: build code, analyze code, make changes based on user feedback
- I should NEVER claim to "test" the TUI or say "let me test this"
- I should NEVER claim something is fixed without user confirmation
- I should NEVER run `cargo run` expecting to interact with it
- Proper workflow: Analyze → Change code → Build → Ask user to test → Wait for results

## CRITICAL: Known TUI Performance Issue - STOP REPEATING THIS ANALYSIS

**The performance issue when toggling expanded_view (Ctrl+R) has been analyzed. The problem is clear:**

1. **Root Cause**: When `expanded_view` toggles, ChatView rebuilds ALL lines for ALL messages including:
   - Calling `parse_markdown()` for every assistant message
   - Creating thousands of Line/Span objects
   - This happens synchronously, blocking the UI

2. **Why it freezes**: With many messages (especially after /resume), rebuilding all lines takes seconds

3. **Ratatui's `.scroll()` is NOT the problem** - it works fine with large content

4. **The real fix needed**: Cache rendered lines in AppState and only rebuild when:
   - New messages are added
   - expanded_view changes (but reuse unchanged message renderings)

5. **ChatView is stateless** - recreated every frame, so it cannot hold the cache

6. **Stop trying these approaches that don't work**:
   - Virtual scrolling in ChatView (can't know line heights before rendering)
   - Estimating line counts (that's a hack, not a fix)
   - Trying to cache in ChatView (it's recreated every frame)

**The solution requires caching rendered lines in AppState, but that's a significant architectural change.**

## Next Steps

1. Verify Bash tool works in actual application (not just tests)
2. Implement interactive directory permission prompts
3. Continue with Grep and Glob tools
4. Implement remaining tools one by one
5. Full integration testing

Remember: Every line of code should be production-ready. No exceptions.