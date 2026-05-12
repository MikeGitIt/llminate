# Claude Code Written in Rust - Enhancements & Capabilities

This document tracks enhancements and capabilities that exist specifically because this implementation is written in Rust, rather than JavaScript.

---

## Additions (Tools not in JavaScript Claude Code)

### HttpRequest Tool
**Status:** Addition - Not present in JavaScript Claude Code

**What it does:**
- Makes raw HTTP requests (GET, POST, PUT, DELETE, PATCH)
- Supports custom headers
- Supports request body
- Returns structured response (status code, headers, body)

**Benefits over Bash + curl:**
1. **Structured input/output** - JSON schema input, parsed response output. No shell escaping issues with JSON bodies.
2. **Cross-platform consistency** - Uses reqwest library which behaves identically across macOS, Linux, Windows. curl behavior varies between systems.
3. **Proper error handling** - Rust's Result type provides clear error handling vs parsing curl exit codes and stderr.
4. **No shell injection risk** - Input is validated and sandboxed. Bash + curl with user-provided URLs/data has security implications.
5. **Tool integration** - Works with permission system, logging, and cancellation tokens like all other tools.

**Location:** `src/ai/tools.rs` lines 2670-2769

---

## Enhancements (Improved capabilities on existing tools)

### Read Tool - Extended Image Format Support
**Status:** Enhancement to existing JavaScript tool

**JavaScript supports:** png, jpg, jpeg, gif, webp

**Rust adds:** bmp, ico, tiff, tif, heic, heif, avif

**Reason:** Rust image libraries (image crate) support more formats out of the box.

**Location:** `src/ai/tools.rs` (Read tool implementation)

---

### LS (ListFiles) Tool
**Status:** Addition - No direct equivalent in JavaScript Claude Code

**What it does:**
- Lists directory contents with optional ignore patterns
- Provides structured file listing

**Location:** `src/ai/tools.rs` line 542

---

### Search (SearchFiles) Tool
**Status:** Addition - No direct equivalent in JavaScript Claude Code

**What it does:**
- Searches for text patterns within files
- Supports file pattern filtering

**Location:** `src/ai/tools.rs` line 543

---

### NotebookRead Tool - Improved Output Formatting
**Status:** Enhancement - Improved readability over JavaScript

**JavaScript format (single line):**
```
<cell id="cell-0"><cell_type>markdown</cell_type>content</cell id="cell-0">
```

**Rust format (with newlines):**
```
<cell id="cell-0">
<cell_type>markdown</cell_type>
content
[Execution count: N]
</cell id="cell-0">
```

**Benefits:**
- More readable output with proper newlines
- Includes execution count information for code cells
- Easier to parse visually

**Location:** `src/ai/notebook_tools.rs` function `format_cell_js_style`

---

## Future Enhancement Opportunities

These are areas where Rust could provide additional benefits not yet implemented:

1. **Parallel file operations** - Rust's fearless concurrency could enable faster multi-file edits
2. **Memory-mapped file reading** - For very large files, could use mmap for efficiency
3. **Native regex compilation** - Rust's regex crate compiles patterns for faster repeated searches
4. **Binary file handling** - Rust's strong type system could enable safer binary file manipulation

---

## Notes

- All enhancements MUST maintain API compatibility with JavaScript tools where they exist
- Additions should be clearly documented so users know what's available beyond standard Claude Code
- This document should be updated whenever new Rust-specific capabilities are added
