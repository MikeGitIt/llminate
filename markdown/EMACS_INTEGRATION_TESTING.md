# llminate Emacs Integration — Testing Guide

This document covers how to test the llminate + ruemacs + completion server Emacs integration. Steps are ordered from simplest to most complex — each step validates a layer before building on it.

---

## Step 1: Build the Rust binaries

```bash
# Build llminate
cd ~/Code/rust_projects/llminate
cargo build --release

# Build completion server
cd ~/ruemacs/src/ruemacs_completion_server
cargo build --release
```

Both should compile with zero errors. If either fails, fix before proceeding.

---

## Step 2: Test llminate --keep-alive from terminal

This tests the Rust side in isolation, no Emacs involved.

```bash
cd ~/Code/rust_projects/llminate

# Send a simple prompt and check the JSON-lines protocol
echo '{"type":"Message","role":"user","content":"Say hello in one word"}' | \
  cargo run --release -- -p --output-format stream-json --input-format stream-json --keep-alive
```

### What to look for

- A `{"type":"Start","session_id":"...","model":"..."}` line
- Multiple `{"type":"Message","role":"assistant","content":"..."}` lines (streaming tokens)
- A `{"type":"End","reason":"completed"}` line
- A `{"type":"Ready"}` line — this proves keep-alive works
- The process should **NOT** exit — it waits for more input (`Ctrl+C` to kill)

If you don't see `Ready`, the keep-alive loop isn't working.

---

## Step 3: Test completion server health endpoint

```bash
# Start the server in one terminal
cd ~/ruemacs/src/ruemacs_completion_server
cargo run --release

# In another terminal, check health
curl http://localhost:3000/health
```

**Expected:** `{"status":"ok"}`

---

## Step 4: Load elisp files in Emacs batch mode (syntax check)

```bash
emacs --batch \
  -L ~/.emacs.d/lisp/ \
  -L ~/.emacs.d/lisp/ruemacs/ \
  -L ~/.emacs.d/lisp/completion/ \
  --eval "(progn
    (require 'llminate-bridge)
    (require 'llminate-emacs-commands)
    (require 'llminate-chat)
    (require 'llminate-approval)
    (require 'llminate-completion)
    (require 'llminate-layout)
    (require 'llminate-session)
    (require 'llminate-mode)
    (message \"All packages loaded successfully\"))" \
  2>&1
```

### What to look for

"All packages loaded successfully" with no errors. Warnings about missing functions (like `magit` or `transient`) are OK in batch mode — they'll be available in your real Emacs session.

---

## Step 5: Interactive Emacs testing — load the mode

Open Emacs normally and evaluate:

```elisp
;; In *scratch* or M-:
(add-to-list 'load-path "~/.emacs.d/lisp/")
(require 'llminate-mode)
(llminate-mode 1)
```

### Check

- Modeline should show ` llm[off]` or ` llm[idle]`

### Permanent setup

Once verified, add to `~/.emacs`:

```elisp
(add-to-list 'load-path "~/.emacs.d/lisp/")
(require 'llminate-mode)
(llminate-mode 1)
```

---

## Step 6: Test the chat UI (no llminate subprocess yet)

```
C-c q q    (or M-x llminate-chat-toggle)
```

### Check

- `*llminate Chat*` buffer appears as a side window on the right
- `*llminate Prompt*` buffer appears at the bottom
- You can type in the prompt buffer
- `C-c q q` again hides both windows

---

## Step 7: Test the bridge (connects to llminate)

Make sure the llminate binary is in your PATH or set the path first:

```elisp
(setq llminate-bridge-executable "~/Code/rust_projects/llminate/target/release/llminate")
```

Then:

```
C-c q q       ;; Open chat panel
;; Type "hello" in the prompt buffer
C-c C-c       ;; Send the prompt
```

### What to look for

- Modeline changes to ` llm[streaming]`
- Text appears token-by-token in the chat log buffer
- When done, modeline returns to ` llm[idle]`
- A separator line appears after the response

### If it fails

- Check `*Messages*` buffer for errors
- Check if a process named `llminate` is running: `M-x list-processes`
- Try running Step 2 again in a terminal to isolate whether it's a Rust or Elisp issue

---

## Step 8: Test the EmacsCommand reverse channel

In the chat prompt, type something that should trigger Emacs interaction:

```
Open the file ~/.emacs in my editor
```

### What to look for

- llminate should use the `EmacsCommand` tool
- You'll see `[Emacs] find-file ~/.emacs` in the chat log
- The file should actually open in an Emacs buffer
- Modeline briefly shows ` llm[emacs:find-file]`

### Try another

```
Show me the git status of this project
```

llminate should use `EmacsCommand` with `magit-status` instead of shelling out to `git status`.

### Safety test

Ask llminate to run a command NOT in the whitelist. It should receive a denial result. You can view the whitelist with:

```
C-c q w    (or M-x llminate-emacs-commands-list)
```

---

## Step 9: Test tool approval

Send a prompt that triggers a file write:

```
Create a file at /tmp/llminate-test.txt with the content "hello from llminate"
```

### What to look for

- A preview buffer pops up at the bottom showing the file path and content
- Keybindings displayed: `y`=approve, `n`=deny, `a`=always allow, `e`=edit, `d`=diff
- Press `y` — the file should be created
- Verify: `cat /tmp/llminate-test.txt` should show `hello from llminate`

### Test denial

Send another prompt that triggers a tool. Press `n` to deny. llminate should receive the denial and acknowledge it in its response.

### Test always-allow

Press `a` on a tool approval. Subsequent uses of that tool should be auto-approved for the rest of the session.

---

## Step 10: Test the IDE layout

```
C-c q l    (or M-x llminate-layout-toggle)
```

### Check

- Full IDE layout appears:
  - Treemacs on the left
  - Main editor in the center
  - Chat log on the right
  - Activity log at the bottom-left
  - Prompt input at the bottom-right
- `C-c q l` again restores your previous window layout
- Activity log shows tool executions, approvals, and EmacsCommand calls with timestamps

---

## Step 11: Test code completion

Requires the completion server to be running (Step 3).

1. Start the server from Emacs: `M-x llminate-completion-start-server`
2. Open a Rust file (or any `prog-mode` file)
3. Start typing a function body and pause for 0.5 seconds
4. Corfu popup should appear with AI completions annotated `[AI]`
5. `TAB` to accept, verify the completion is inserted

### Manual trigger

If auto-completion doesn't fire:

```
C-c q .    (or M-x llminate-completion-at-point)
```

### Stop the server

```
M-x llminate-completion-stop-server
```

---

## Step 12: Test session persistence

1. Send a few messages in the chat
2. Note the session ID: `M-: llminate-bridge--session-id`
3. Save the session: `M-x llminate-session-save`
4. Kill and restart Emacs
5. List saved sessions: `M-x llminate-session-list`
6. Your session should appear — press `RET` on it to resume
7. The chat should continue from where you left off

### Alternative: resume with keybinding

```
C-c q r    (or M-x llminate-session-resume)
```

---

## Step 13: Test editor context (Phase 5)

1. Open a Rust file with some eglot/flymake errors
2. Send diagnostics to llminate:

```
C-c q d    (or M-x llminate-send-diagnostics)
```

3. Verify llminate's response references the actual errors and file context

### Test region explanation

1. Select a region of code
2. `C-c q e` (explain region)
3. Verify llminate receives the selected code with language context

### Test region fix

1. Select a region with a bug
2. `C-c q f` (fix region)
3. Verify llminate suggests a fix for the specific code

---

## Keybinding Reference

All keybindings use the `C-c q` prefix:

| Key | Command | Description |
|-----|---------|-------------|
| `C-c q q` | `llminate-chat-toggle` | Show/hide chat panel |
| `C-c q s` | `llminate-chat-send` | Send prompt from chat |
| `C-c q l` | `llminate-layout-toggle` | Toggle full IDE layout |
| `C-c q r` | `llminate-session-resume` | Resume a saved session |
| `C-c q c` | `llminate-command-palette` | Command palette |
| `C-c q e` | `llminate-explain-region` | Explain selected code |
| `C-c q f` | `llminate-fix-region` | Fix/refactor selected code |
| `C-c q d` | `llminate-send-diagnostics` | Send diagnostics to llminate |
| `C-c q .` | `llminate-completion-at-point` | Trigger AI completion |
| `C-c q w` | `llminate-emacs-commands-list` | View/edit Emacs command whitelist |

### Chat prompt keybindings

| Key | Action |
|-----|--------|
| `C-c C-c` | Send the prompt |
| `C-c C-k` | Cancel / clear prompt |
| `M-p` | Previous history entry |
| `M-n` | Next history entry |
| `RET` | Insert newline (multi-line input) |

### Approval preview keybindings

| Key | Action |
|-----|--------|
| `y` | Approve |
| `n` | Deny |
| `a` | Always allow this tool |
| `e` | Edit input before approving |
| `d` | Show ediff (for file edits) |
| `q` | Quit (deny) |

---

## Troubleshooting

| Problem | What to check |
|---------|---------------|
| Bridge won't start | `M-x list-processes`, check `*Messages*` buffer, verify `llminate-bridge-executable` path |
| No streaming output | Run Step 2 in terminal first to isolate Rust vs Elisp issue |
| EmacsCommand denied | `C-c q w` to check whitelist — is the command registered? |
| Completion server won't start | Check port 3000 isn't in use: `lsof -i :3000` |
| Approval popup doesn't appear | Verify `(require 'llminate-approval)` loaded: `M-: (featurep 'llminate-approval)` |
| Modeline not updating | Check hooks: `M-: llminate-bridge-state-change-hook` |
| Session not saving | Check file permissions on `~/.emacs.d/llminate-sessions.json` |
| Corfu not showing AI completions | Ensure completion server is running (`curl localhost:3000/health`) and CAPF is hooked (`M-: completion-at-point-functions`) |

### Debug: View raw bridge output

```elisp
;; Show the bridge process buffer with raw JSON-lines
(switch-to-buffer (process-buffer llminate-bridge--process))
```

### Debug: Check bridge state

```elisp
M-: llminate-bridge--state        ;; Current state (idle, streaming, etc.)
M-: llminate-bridge--session-id   ;; Session ID
M-: llminate-bridge--model        ;; Model in use
```

### Reset everything

```elisp
(llminate-mode -1)                ;; Disable the mode
(llminate-bridge-stop)            ;; Kill the subprocess
(llminate-completion-stop-server) ;; Kill completion server
(llminate-mode 1)                 ;; Re-enable fresh
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                           Emacs                                     │
│                                                                     │
│  llminate-mode.el ──── llminate-bridge.el ◄──► llminate (Rust)     │
│       │                     │    ▲                  │               │
│       │                     │    │ EmacsEval/Result  │               │
│       │                     ▼    │                  ▼               │
│  llminate-chat.el    llminate-emacs-commands.el   Claude API       │
│  llminate-layout.el  llminate-approval.el                          │
│  llminate-session.el                                                │
│                                                                     │
│  llminate-completion.el ◄──► ruemacs_completion_server (Rust)      │
│       │                          │                                  │
│       ▼                          ▼                                  │
│    corfu/CAPF              OpenAI / Anthropic API                   │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Files Reference

### Rust files modified/created

| File | Description |
|------|-------------|
| `~/Code/rust_projects/llminate/src/cli.rs` | Added `--keep-alive` flag |
| `~/Code/rust_projects/llminate/src/tui/print_mode.rs` | Extended StreamEvent, keep-alive loop, bidirectional protocol |
| `~/Code/rust_projects/llminate/src/ai/emacs_tool.rs` | EmacsCommandTool — AI calls Emacs functions |
| `~/Code/rust_projects/llminate/src/ai/tools.rs` | Registered EmacsCommand tool |
| `~/Code/rust_projects/llminate/src/ai/mod.rs` | Module declaration |
| `~/ruemacs/src/ruemacs_completion_server/src/ruemacs_server.rs` | Fixed Anthropic API, enabled cache, added /health |

### Elisp files created

| File | Size | Description |
|------|------|-------------|
| `~/.emacs.d/lisp/llminate-bridge.el` | 22KB | Bidirectional subprocess bridge + editor context |
| `~/.emacs.d/lisp/llminate-emacs-commands.el` | 12KB | 47-command whitelist with security levels |
| `~/.emacs.d/lisp/llminate-chat.el` | 16KB | Streaming chat UI with 8 custom faces |
| `~/.emacs.d/lisp/llminate-approval.el` | 22KB | Tool approval UX with transient + ediff |
| `~/.emacs.d/lisp/llminate-completion.el` | 15KB | CAPF/corfu integration with completion server |
| `~/.emacs.d/lisp/llminate-layout.el` | 8KB | IDE window layout with activity log |
| `~/.emacs.d/lisp/llminate-session.el` | 10KB | Session persistence and resume |
| `~/.emacs.d/lisp/llminate-mode.el` | 11KB | Global minor mode, keybindings, modeline |
