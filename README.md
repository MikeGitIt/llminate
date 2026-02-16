# llminate

A production-grade agentic coding assistant built in Rust, providing Claude Code-like capabilities with enhanced performance and safety.

## Overview

llminate is a complete Rust implementation of an AI-powered coding assistant, originally ported from JavaScript. It provides an interactive terminal user interface (TUI) for AI-assisted software development with support for multiple AI providers.

## Key Features

### Core Capabilities

- **Interactive TUI**: Full-featured terminal interface built with Ratatui
  - Markdown rendering with syntax highlighting
  - Code block visualization
  - Multi-pane layouts
  - Vim-style keybindings support

- **AI Integration**: Multiple AI provider support
  - Anthropic Claude API integration
  - OAuth authentication flow
  - Session management
  - Streaming responses

- **Development Tools**
  - File system operations (read, write, edit)
  - Bash command execution
  - Git integration
  - Web search and fetch capabilities
  - HTTP request handling
  - Jupyter notebook support

### Advanced Features

- **MCP (Model Context Protocol)** support for extensibility
- **Plugin system** for custom functionality
- **Telemetry and error tracking** with Sentry integration
- **Auto-updater** for keeping the tool current
- **Permission system** for secure operations

## Architecture

Built with modern Rust practices:
- Async/await with Tokio runtime
- Type-safe error handling with anyhow and thiserror
- Modular architecture with clear separation of concerns
- Comprehensive test coverage

### Key Components

- `src/ai/` - AI client implementations and adapters
- `src/auth/` - Authentication and authorization modules
- `src/tui/` - Terminal user interface components
- `src/utils/` - Utility functions and helpers
- `src/cli.rs` - Command-line interface
- `src/config.rs` - Configuration management
- `src/mcp.rs` - Model Context Protocol implementation
- `src/plugin.rs` - Plugin system

## Installation

### Prerequisites

- Rust 1.70+ and Cargo
- Git (for repository operations)
- Terminal with UTF-8 support

### Build from Source

```bash
# Clone the repository
git clone [repository-url]
cd llminate

# Build the project
cargo build --release

# Run tests
cargo test

# Install locally
cargo install --path .
```

## Configuration

Create a `.env` file in the project root:

```env
# AI Provider Configuration
ANTHROPIC_API_KEY=your_api_key_here

# Optional: Additional providers
# Add other provider keys as needed
```

See `config_example.json` for detailed configuration options.

## Usage

```bash
# Start the interactive TUI
llminate

# Run in print mode (non-interactive)
llminate --print "Your prompt here"

# Show help
llminate --help
```

### TUI Commands

Once in the TUI:
- Type your queries naturally
- Use `/help` for available commands
- Press `Ctrl+q` to exit

## Development

### Project Structure

- Comprehensive documentation in `*.md` files
- Implementation specifications in `SPECS/` directory
- Test suites in `tests/` directory
- Build configuration in `Cargo.toml`

### Testing

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test --test integration_tests

# Run with verbose output
cargo test -- --nocapture
```

## Rust Enhancements

This Rust implementation provides several enhancements over the original JavaScript version:

- **Type Safety**: Compile-time guarantees prevent runtime errors
- **Performance**: Zero-cost abstractions and efficient memory management
- **Concurrency**: Safe parallel execution with Rust's ownership model
- **Error Handling**: Comprehensive error context and recovery
- **Extended Format Support**: Additional file types and protocols

## License

[License information to be added]

## Contributing

[Contribution guidelines to be added]
