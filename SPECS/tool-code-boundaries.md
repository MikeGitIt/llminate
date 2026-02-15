# JavaScript Tool Architecture Analysis

## Entry Point and Main Flow

### Entry Point (Line 430992)
- **Main Entry**: `checker266()` at line 430992
- **Entry Function**: Defined at line 429950
- **CLI Setup**: `stringDecoder610()` at line 429997 sets up Commander.js CLI
- **Two Execution Modes**:
  1. **Print Mode** (non-interactive): `stringDecoder608()` at line 429219
  2. **Interactive Mode** (TUI): React components starting at line 430301

### Tool Registration
- **Tool Array**: `input20152` at line 430211
- **Tool Loading**: Via `value2433()` (built-in) and `value2204()` (MCP) at lines 430215-430223
- **Permission Filtering**: By `allowedTools`/`disallowedTools` arrays

## Critical Bash Tool Implementation Details

### Command Execution (Lines 51399-51542)
```javascript
// Line 51520: Actual spawn call
value4163.spawn(input20289, config8166, input20345)

// Platform-specific handling (lines 51400-51404)
if (process.platform !== "win32")
  return { cmd: TOA(input20325), args: config8199 }
```

### Working Directory Persistence

**Key Discovery**: JavaScript does NOT persist full shell sessions!

#### State Management (Lines 334532-334540)
- `obj.originalCwd`: Initial working directory (never changes)
- `obj.cwd`: Current working directory (updated after each command)
- NO environment variable persistence
- NO shell state persistence

#### How cd Commands Work (Lines 357436-357488)
1. **Temp File Creation**: `/tmp/claude-XXXX-cwd` (line 357436)
2. **Command Modification**: Appends `&& pwd -P >| /tmp/claude-XXXX-cwd` to EVERY command (line 357460)
3. **Directory Capture**: Reads temp file after execution
4. **State Update**: Updates `obj.cwd` with new directory
5. **Next Command**: Starts in updated directory but with FRESH shell

**CRITICAL**: Each command runs in a NEW shell process - only working directory persists!

### Permission System (Lines 366325-366331, 352016-352031)
```javascript
// Permission state structure
{
  mode: "default",
  additionalWorkingDirectories: new Set(),
  alwaysAllowRules: {},
  alwaysDenyRules: {},
  isBypassPermissionsModeAvailable: false
}
```

- **Directory Checking**: `tF()` function at line 352019
- **Path Containment**: `di()` function at lines 352024-352031
- **Default Allow**: Current working directory always allowed

### Background Shells (Lines 386652-386854)
- **Class46**: Background shell implementation (lines 386652-386729)
- **Ju Class**: Singleton manager (lines 386730-386854)
- **Output Accumulation**: Real stdout/stderr from spawned processes
- **No Fake Results**: All output comes from actual process execution

## Critical Bug Found: Sandbox Mode

### The Problem
**Rust Implementation Bug**: Sandbox profile ALLOWS all writes!

```rust
// WRONG - Current Rust implementation
(allow file-write*)      // Allows ALL writes
(allow file-write-create) // Allows creating files
(allow file-write-unlink) // Allows deleting files
```

### JavaScript Behavior
- **NO explicit sandbox mode found** in JavaScript code
- Permission system uses directory allowlists, not operation blocking
- All commands spawn real processes with real results

### Why Fake Results Appear
1. Sandbox allows writes in isolated filesystem
2. Commands succeed in sandbox (exit code 0)
3. `ls` shows files created in sandbox
4. Files don't exist in real filesystem
5. Tool reports "success" based on sandbox exit code

## Tool Implementation Locations

### Core Tools (Lines 185,000-197,000)
1. **Task Tool** (obj174): ~194,500-195,000
2. **Bash Tool** (obj100): ~187,000-188,000
3. **Glob Tool** (obj109): ~190,000-191,000
4. **Grep Tool** (zy): ~191,000-192,000
5. **LS Tool** (AE): ~189,000-190,000
6. **ExitPlanMode Tool** (Zu): ~193,000-194,000
7. **Read Tool** (LB): ~186,000-187,000
8. **Edit Tool** (bI): ~188,000-189,000
9. **MultiEdit Tool** (obj104): ~189,000-190,000
10. **Write Tool** (aJ): ~187,000-188,000
11. **NotebookRead Tool** (obj88): ~185,000-186,000
12. **NotebookEdit Tool** (uO): ~192,000-193,000
13. **TodoWrite Tool** (jG): ~194,000-195,000
14. **WebFetch Tool** (obj111): ~195,000-196,000
15. **WebSearch Tool** (obj175): ~196,000-197,000

### Tool Collection Function
- **WT Function**: Lines ~195,000-195,500
- Collects and filters all tools based on permissions

## Key Differences: JavaScript vs Rust

### JavaScript Implementation
1. **No Session Persistence**: New shell for each command
2. **Directory-Only State**: Only `cwd` persists via temp files
3. **No Environment Persistence**: Fresh environment each time
4. **Real Process Execution**: All results from actual spawn()
5. **No Sandbox Mode**: Uses directory permissions only

### Rust Implementation Issues
1. **Incorrect Session Persistence**: Trying to maintain full shell state
2. **Broken Sandbox**: Allows writes when it shouldn't
3. **Complex State Management**: Over-engineered compared to JS
4. **False Success Reports**: Sandbox succeeds but changes don't persist

## Required Fixes

1. **Fix Sandbox Profile**: Block ALL write operations in sandbox mode
2. **Simplify Shell State**: Only persist working directory, not full session
3. **Match JS Behavior**: New shell process for each command
4. **Clear Error Reporting**: Sandbox write attempts should fail with errors

## Code Organization

The JavaScript file structure:
1. **Lines 1-185,000**: Bundled dependencies and utilities
2. **Lines 185,000-197,000**: Tool implementations
3. **Lines 195,000-195,500**: Tool collection logic
4. **Lines 197,000-430,997**: Runtime and UI code
5. **Lines 429,000+**: Entry point and CLI setup

Tools are interspersed with utilities, not in a single block. Each tool is self-contained with schema definition immediately preceding implementation.