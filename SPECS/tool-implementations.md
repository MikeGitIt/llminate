# Tool Implementations in JavaScript

## Overview
The JavaScript codebase is heavily obfuscated/minified. Tools are collected in the `WT` function and registered with obfuscated variable names.

## Tool Collection Function
```javascript
var WT = (input20325, config8199) => {
  let next2170 = [obj174, obj100, obj109, zy, AE, Zu, LB, bI, obj104, aJ, ...(process.env.CLAUDE_CODE_ENABLE_UNIFIED_READ_TOOL ? [] : [obj88]), uO, obj111, ...(config8199 ? [lN, jG] : []), obj175, ...[], ...[]],
  // ... tool filtering logic
```

## Identified Tools

### 1. Task Tool (Agent)
- **Variable name**: `obj174`
- **Registered name**: `pX = "Task"`
- **Description**: "Launch a new task"
- **Schema**: 
  ```javascript
  value2415 = obj26.object({
    description: obj26.string().describe("A short (3-5 word) description of the task"),
    prompt: obj26.string().describe("The task for the agent to perform")
  });
  ```
- **Key behaviors**:
  - Can run multiple parallel agents based on `parallelTasksCount`
  - Filters out itself from available tools when creating sub-agents
  - Tracks token usage and tool use counts
  - Has synthesis step when multiple agents run
  - Marked as read-only and concurrency-safe

### 2. WebFetch Tool
- **Variable name**: `obj111`
- **Registered name**: `str142 = "WebFetch"`
- **Description**: Long description about fetching web content
- **Schema**:
  ```javascript
  obj26.strictObject({
    url: obj26.string().url().describe("The URL to fetch content from"),
    prompt: obj26.string().describe("The prompt to run on the fetched content")
  });
  ```

### 3. File Read Tool
- **Variable name**: `LB`
- **Registered name**: `PD` (need to find where PD is defined)
- **Description function**: Returns `wfA`
- **Prompt function**: Returns `EfA`
- **User facing name**: "Read"
- **Schema**:
  ```javascript
  value2210 = obj26.strictObject({
    file_path: obj26.string().describe("The absolute path to the file to read"),
    offset: obj26.number().optional().describe("The line number to start reading from. Only provide if the file is too large to read at once"),
    limit: obj26.number().optional().describe("The number of lines to read. Only provide if the file is too large to read at once.")
  })
  ```
- **Key features**:
  - Read-only, concurrency-safe
  - Handles text files, images, and notebooks
  - Has special rendering for different file types
  - Checks permissions before reading

### 4. Bash Tool
- **Variable name**: `obj100`
- **Registered name**: `QK = "Bash"`
- **Description function**: Returns input description or "Run shell command"
- **User facing name**: "Execute Command"
- **Implementation details** (Line 357466):
  - Uses Node.js `spawn` with shell executable
  - Passes flags: `["-c", "-l", commandChain]` (login shell)
  - Command preparation (Lines 357450-357459):
    - Quotes command and adds `< /dev/null`
    - Uses `eval` to execute: `eval ${quotedCommand} < /dev/null`
    - Tracks pwd with: `pwd -P >| ${tempFile}`
  - Environment variables set: `SHELL`, `GIT_EDITOR: "true"`, `CLAUDECODE: "1"`
  - Uses `detached: true` for spawn options
- **Key features**:
  - Execute shell commands with timeout
  - Supports persistent shell sessions
  - Has working directory tracking
  - Login shell mode for proper environment loading

### 5. Glob Tool
- **Variable name**: `obj109`
- **Registered name**: `str147 = "Glob"`
- **Description**: Fast file pattern matching tool
- **User facing name**: "Search"
- **Schema**:
  ```javascript
  value2320 = obj26.strictObject({
    pattern: obj26.string().describe("The glob pattern to match files against"),
    path: obj26.string().optional().describe('The directory to search in...')
  })
  ```

### 6. Grep Tool
- **Variable name**: `zy`
- **Registered name**: `str149 = "Grep"`
- **Description function**: Uses `stringDecoder163` function
- **Schema**:
  ```javascript
  obj26.strictObject({
    pattern: obj26.string().describe("The regular expression pattern to search for in file contents"),
    path: obj26.string().optional().describe("The directory to search in. Defaults to the current working directory."),
    include: obj26.string().optional().describe('File pattern to include in the search (e.g. "*.js", "*.{ts,tsx}")')
  })
  ```

### 7. LS Tool
- **Variable name**: `AE`
- **Registered name**: `str150 = "LS"`
- **Description**: Lists files and directories in a given path
- **User facing name**: "List"
- **Schema**:
  ```javascript
  value2213 = obj26.strictObject({
    path: obj26.string().describe("The absolute path to the directory to list (must be absolute, not relative)"),
    ignore: obj26.array(obj26.string()).optional().describe("List of glob patterns to ignore")
  })
  ```

### 8. ExitPlanMode Tool
- **Variable name**: `Zu`
- **Registered name**: `str178 = "exit_plan_mode"`
- **Description**: Prompts the user to exit plan mode and start coding
- **Schema**:
  ```javascript
  value2243 = obj26.strictObject({
    plan: obj26.string().describe("The plan you came up with, that you want to run by the user for approval. Supports markdown. The plan should be pretty concise.")
  })
  ```

### 9. Edit Tool
- **Variable name**: `bI`
- **Registered name**: `uU = "Edit"`
- **Description**: A tool for editing files
- **Schema**:
  ```javascript
  obj26.strictObject({
    file_path: obj26.string().describe("The absolute path to the file to modify"),
    old_string: obj26.string().describe("The text to replace"),
    new_string: obj26.string().describe("The text to replace it with (must be different from old_string)"),
    replace_all: obj26.boolean().default(false).optional().describe("Replace all occurences of old_string (default false)")
  })
  ```

### 10. MultiEdit Tool
- **Variable name**: `obj104`
- **Registered name**: `str187 = "MultiEdit"`
- **Description**: Tool for making multiple edits to a single file
- **User facing name**: Similar to Edit tool
- **Schema**:
  ```javascript
  value2271 = obj26.strictObject({
    file_path: obj26.string().describe("The absolute path to the file to modify"),
    edits: obj26.array(value2270).min(1, "At least one edit is required").describe("Array of edit operations to perform sequentially on the file")
  })
  ```

### 11. Write Tool
- **Variable name**: `aJ`
- **Registered name**: `str188 = "Write"`
- **Description**: Write a file to the local filesystem
- **User facing name**: "Write"
- **Schema**:
  ```javascript
  value2278 = obj26.strictObject({
    file_path: obj26.string().describe("The absolute path to the file to write (must be absolute, not relative)"),
    content: obj26.string().describe("The content to write to the file")
  })
  ```

### 12. NotebookRead Tool
- **Variable name**: `obj88`
- **Registered name**: `KS = "NotebookRead"`
- **Description**: Extract and read source code from all code cells in a Jupyter notebook
- **Conditionally included**: Based on `CLAUDE_CODE_ENABLE_UNIFIED_READ_TOOL`
- **Schema**:
  ```javascript
  value2207 = obj26.strictObject({
    notebook_path: obj26.string().describe("The absolute path to the Jupyter notebook file to read (must be absolute, not relative)"),
    cell_id: obj26.string().optional().describe("The ID of a specific cell to read. If not provided, all cells will be read.")
  })
  ```

### 13. NotebookEdit Tool
- **Variable name**: `uO`
- **Registered name**: `Vu = "NotebookEdit"`
- **Description**: Completely replaces the contents of a specific cell in a Jupyter notebook
- **Schema**:
  ```javascript
  obj26.strictObject({
    notebook_path: obj26.string().describe("The absolute path to the Jupyter notebook file to edit"),
    cell_id: obj26.string().optional().describe("The ID of the cell to edit..."),
    new_source: obj26.string().describe("The new source for the cell"),
    cell_type: obj26.enum(["code", "markdown"]).optional().describe("The type of the cell..."),
    edit_mode: obj26.enum(["replace", "insert", "delete"]).optional().describe("The type of edit to make...")
  })
  ```

### 14. TodoWrite Tool
- **Variable name**: `jG`
- **Registered name**: `"TodoWrite"`
- **User facing name**: "Update Todos"
- **Conditionally included**: Based on `config8199`
- **Schema**:
  ```javascript
  value2038 = obj26.strictObject({
    todos: value2034.describe("The updated todo list")
  })
  ```

### 15. WebSearch Tool
- **Variable name**: `obj175`
- **Registered name**: `str210 = "WebSearch"`
- **Description**: Allows Claude to search the web and use the results to inform responses
- **User facing name**: "Web Search"
- **Key features**:
  - Provides up-to-date information for current events
  - Domain filtering supported
  - Only available in the US

### 16. Other Tool Variables (not yet fully analyzed)
- `lN` - Conditionally included based on `config8199` (likely another development tool)

## Key Patterns Observed

### Tool Object Structure
Each tool object appears to have:
- `name`: The registered tool name
- `description()`: Async function returning description
- `inputSchema`: Schema definition using `obj26` (likely zod)
- `call()`: Async generator function for execution
- `isReadOnly()`: Returns boolean
- `isConcurrencySafe()`: Returns boolean
- `isEnabled()`: Returns boolean
- `userFacingName()`: Returns display name
- `checkPermissions()`: Async function for permission checking

### Schema Validation
The code uses `obj26` which appears to be a schema validation library (likely zod):
- `obj26.object()` - Object schema
- `obj26.string()` - String schema
- `obj26.strictObject()` - Strict object schema
- `.describe()` - Adds description to fields

### Execution Context
Tools receive these parameters in their `call()` method:
```javascript
async *call(input, {
  abortController,
  options: { debug, tools, verbose, isNonInteractiveSession },
  getToolPermissionContext,
  readFileState,
  setInProgressToolUseIDs
}, param3, param4)
```

## Notes on Code Obfuscation
- Variable names are heavily obfuscated (e.g., `input20325`, `config8199`)
- Tool names are stored in separate variables (e.g., `pX = "Task"`)
- The actual tool implementations are spread throughout the file
- Need to trace variable references to understand full implementation

## TypeScript vs JavaScript Discrepancies
The TypeScript definitions show `subagent_type` for AgentInput, but the actual JavaScript implementation only has `description` and `prompt`. This suggests the TypeScript definitions may be outdated or the field is handled differently in the implementation.

## Summary of All Tools

1. **Task** (obj174) - Launches autonomous sub-agents
2. **WebFetch** (obj111) - Fetches and processes web content
3. **Read** (LB) - Reads files with offset/limit support
4. **Bash** (obj100) - Executes shell commands
5. **Glob** (obj109) - File pattern matching
6. **Grep** (zy) - Text search in files
7. **LS** (AE) - Directory listing with ignore patterns
8. **exit_plan_mode** (Zu) - Exits planning mode
9. **Edit** (bI) - Single file editing
10. **MultiEdit** (obj104) - Multiple edits to one file
11. **Write** (aJ) - Write files
12. **NotebookRead** (obj88) - Read Jupyter notebooks
13. **NotebookEdit** (uO) - Edit Jupyter notebooks
14. **TodoWrite** (jG) - Manage todo lists
15. **WebSearch** (obj175) - Web search integration
16. **Unknown** (lN) - Conditionally included tool

## Tool Collection Order
The tools are collected in the `WT` function in this order:
```javascript
[obj174, obj100, obj109, zy, AE, Zu, LB, bI, obj104, aJ, ...(condition ? [] : [obj88]), uO, obj111, ...(config ? [lN, jG] : []), obj175]
```

This translates to:
1. Task
2. Bash
3. Glob
4. Grep
5. LS
6. ExitPlanMode
7. Read
8. Edit
9. MultiEdit
10. Write
11. NotebookRead (conditional)
12. NotebookEdit
13. WebFetch
14. Unknown + TodoWrite (conditional)
15. WebSearch