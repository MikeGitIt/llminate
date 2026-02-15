# JavaScript to Rust Architecture Mapping

## Identified Components and Rust Equivalents

### 1. TUI Framework (React/Ink → Ratatui)
- **JS**: React components with Ink for terminal UI
- **Rust**: `ratatui` - Modern terminal UI framework
- **Dependencies**: 
  ```toml
  ratatui = "0.26"
  crossterm = "0.27"
  ```

### 2. HTTP Server (Express → Axum)
- **JS**: Express middleware and routing
- **Rust**: `axum` - Ergonomic web framework
- **Dependencies**:
  ```toml
  axum = "0.7"
  tower = "0.4"
  tower-http = "0.5"
  ```

### 3. HTTP Parsing (llhttp → httparse/hyper)
- **JS**: llhttp for low-level HTTP parsing
- **Rust**: `httparse` or `hyper`'s built-in parsing
- **Dependencies**:
  ```toml
  httparse = "1.8"
  hyper = "1.0"
  ```

### 4. Protocol Buffers (protobuf → prost)
- **JS**: Google protobuf
- **Rust**: `prost` - Protocol buffer implementation
- **Dependencies**:
  ```toml
  prost = "0.12"
  prost-types = "0.12"
  ```

### 5. Layout Engine (Yoga → taffy)
- **JS**: Yoga layout engine
- **Rust**: `taffy` - Flexbox layout engine
- **Dependencies**:
  ```toml
  taffy = "0.3"
  ```

### 6. Async Runtime (Node.js → Tokio)
- **JS**: Node.js event loop
- **Rust**: `tokio` - Async runtime
- **Dependencies**:
  ```toml
  tokio = { version = "1.35", features = ["full"] }
  ```

### 7. State Management
- **JS**: React state/context
- **Rust**: Custom state management with Arc<Mutex<T>> or channels

### 8. CLI Parsing
- **JS**: Commander.js patterns
- **Rust**: `clap` - Command line argument parser
- **Dependencies**:
  ```toml
  clap = { version = "4.4", features = ["derive"] }
  ```

## Module Structure

```
src/
├── main.rs           # Entry point and CLI setup
├── lib.rs            # Library root
├── tui/
│   ├── mod.rs        # TUI module root
│   ├── app.rs        # Main application state
│   ├── components/   # UI components
│   │   ├── mod.rs
│   │   ├── editor.rs
│   │   ├── chat.rs
│   │   └── status.rs
│   ├── layout.rs     # Layout management with taffy
│   └── event.rs      # Event handling
├── api/
│   ├── mod.rs        # API module root
│   ├── server.rs     # Axum server setup
│   ├── routes/       # API routes
│   │   ├── mod.rs
│   │   ├── chat.rs
│   │   └── models.rs
│   └── middleware.rs # Custom middleware
├── llm/
│   ├── mod.rs        # LLM interaction module
│   ├── client.rs     # LLM client implementation
│   └── messages.rs   # Message types (protobuf)
├── protocol/
│   ├── mod.rs        # Protocol definitions
│   └── messages.proto # Protobuf definitions
└── utils/
    ├── mod.rs
    └── error.rs      # Error handling
```

## Key Implementation Notes

1. **Error Handling**: Use `anyhow` for application errors and `thiserror` for library errors
2. **Async Patterns**: All I/O operations should be async using Tokio
3. **State Sharing**: Use Arc<Mutex<T>> for shared state between TUI and API
4. **Message Passing**: Use tokio channels for component communication
5. **Layout**: Implement flexbox-style layouts using taffy for UI components