# Current State: llminate Emacs Integration

**Date**: 2026-04-02
**Status**: Active development / live testing

---

## Overview

llminate is integrated into Emacs as a long-lived subprocess speaking a JSON-lines protocol over stdin/stdout. The integration consists of 8 elisp packages, Rust-side keep-alive mode, and a bidirectional command channel (Emacs can call llminate, llminate can call Emacs functions).

---

## Architecture

```
Emacs (elisp)                          llminate (Rust)
─────────────                          ───────────────
llminate-mode.el                       src/cli.rs (--keep-alive flag)
  ├─ llminate-bridge.el ──stdin/stdout── src/tui/print_mode.rs
  │    ├─ process filter (JSON-lines)      ├─ run_keep_alive()
  │    ├─ llminate-emacs-commands.el       ├─ emit_event() → stdout
  │    └─ reverse channel (EmacsEval)      ├─ spawn_stdin_reader() ← stdin
  ├─ llminate-chat.el                     └─ process_keep_alive_turn()
  │    ├─ streaming overlay display
  │    └─ response handler               src/ai/emacs_tool.rs
  ├─ llminate-approval.el                  └─ EmacsCommandTool
  ├─ llminate-completion.el
  ├─ llminate-layout.el
  └─ llminate-session.el
```

### JSON-lines Protocol (stdout)

```
→ {"type":"Start","session_id":"...","model":"..."}
← {"type":"Message","role":"user","content":"..."}    (stdin from Emacs)
→ {"type":"Message","role":"user","content":"..."}     (echo)
→ {"type":"Message","role":"assistant","content":"..."}  (streaming tokens)
→ {"type":"ToolUse","name":"...","input":{...}}
→ {"type":"ToolResult","output":{...}}
→ {"type":"EmacsEval","command":"...","args":[...],"request_id":"..."}
← {"type":"EmacsEvalResult","request_id":"...","success":true,"result":"..."}
→ {"type":"End","reason":"completed"}
→ {"type":"Ready"}
```

---

## Files

### Rust (modified)

| File | Changes |
|------|---------|
| `src/cli.rs` | `--keep-alive` flag (hidden), auto-enable stderr logging in print+debug mode |
| `src/tui/print_mode.rs` | `run_keep_alive()`, `emit_event()` with BrokenPipe handling (Result<bool>), `spawn_stdin_reader()`, `process_keep_alive_turn()`, 5 new StreamEvent variants |
| `src/ai/emacs_tool.rs` | `EmacsCommandTool` with `PendingEmacsRequests` global, 60s timeout |
| `src/ai/tools.rs` | Registered `EmacsCommand` tool |
| `src/ai/mod.rs` | `pub mod emacs_tool` |
| `src/main.rs` | Fixed tracing: all console layers use `io::stderr`, print-mode fallback uses no-op subscriber |

### Elisp (created)

| File | Size | Description |
|------|------|-------------|
| `~/.emacs.d/lisp/llminate-bridge.el` | ~22KB | Subprocess management, JSON-lines filter, event dispatch, editor context |
| `~/.emacs.d/lisp/llminate-emacs-commands.el` | ~12KB | 47-command whitelist (allow/prompt/deny security levels) |
| `~/.emacs.d/lisp/llminate-chat.el` | ~16KB | Chat UI with streaming overlay, side-window display, history |
| `~/.emacs.d/lisp/llminate-approval.el` | ~22KB | Tool approval via transient.el, ediff for file edits |
| `~/.emacs.d/lisp/llminate-completion.el` | ~15KB | CAPF/corfu integration with ruemacs_completion_server |
| `~/.emacs.d/lisp/llminate-layout.el` | ~8KB | IDE layout (treemacs + chat + activity log) |
| `~/.emacs.d/lisp/llminate-session.el` | ~10KB | Session persistence to JSON, resume/list/delete |
| `~/.emacs.d/lisp/llminate-mode.el` | ~11KB | Global minor mode, C-c q keybindings, modeline indicator |

---

## Bugs Found and Fixed

### 1. Binary not in PATH
- **Symptom**: `Searching for program: No such file or directory, llminate`
- **Fix**: User sets `llminate-bridge-executable` to full path

### 2. Stale release binary
- **Symptom**: `unexpected argument '--keep-alive' found`
- **Fix**: `cargo build --release` after Rust changes

### 3. Permission mode case mismatch
- **Symptom**: `invalid value 'Ask' for '--permission-mode'`
- **Cause**: clap `ValueEnum` generates lowercase; elisp default was `"Ask"`
- **Fix**: Changed defcustom default to `"ask"` in llminate-bridge.el

### 4. Bridge crash loop on stop

- **Symptom**: Calling `llminate-bridge-stop` (or `M-x llminate-bridge-stop`) did not actually stop the bridge. Instead, the `*Messages*` buffer was immediately flooded with `[llminate] Process crashed` errors and the bridge kept restarting in an infinite loop. The only way to escape was to kill Emacs entirely.

- **Cause**: A subtle ordering bug in how Emacs process sentinels interact with `delete-process`.

  The bridge has an auto-restart feature (`llminate-bridge-restart-on-crash`): when the process sentinel detects a crash, it schedules a restart via `run-with-timer`. The sentinel guards against restarting an intentional stop by checking `(not (eq llminate-bridge--state 'stopped))`.

  The **original** `llminate-bridge-stop` function was written as:
  ```elisp
  (defun llminate-bridge-stop ()
    (when (process-live-p llminate-bridge--process)
      (delete-process llminate-bridge--process))  ;; ← kills the process
    (setq llminate-bridge--state 'stopped))       ;; ← sets state AFTER
  ```

  The problem: in Emacs, `delete-process` calls the process sentinel **synchronously** — meaning the sentinel runs *inside* the `delete-process` call, before it returns. At that point, `llminate-bridge--state` was still `'streaming` (or `'idle`, etc.) because the `(setq ... 'stopped)` line hadn't executed yet.

  So the sentinel saw: "process died, state is NOT `'stopped`, `restart-on-crash` is t → schedule restart." The restart launches a new process, which then also gets stopped, triggering another sentinel, scheduling another restart — infinite loop.

  Additionally, the sentinel itself sets `(setq llminate-bridge--state 'stopped)` (line 210), which happens during the `delete-process` call. But then the original stop function's `(setq ... 'stopped)` runs AFTER `delete-process` returns — by which time a timer has already been scheduled to restart.

- **Fix**: Two changes:
  1. **Set state to `'stopped` BEFORE calling `delete-process`** — so when the sentinel fires synchronously, it sees `'stopped` and skips the auto-restart:
     ```elisp
     (setq llminate-bridge--state 'stopped)       ;; ← FIRST
     (delete-process llminate-bridge--process)     ;; ← sentinel fires here, sees 'stopped
     ```
  2. **Added explicit guard in the sentinel**: `(not (eq llminate-bridge--state 'stopped))` — this was already present but was being defeated by the ordering bug. Now it works correctly because the state is set before the sentinel runs.

- **Key Insight**: In Emacs, `delete-process` is not asynchronous — the sentinel runs inline, within the same call stack. Any state that the sentinel reads must be set *before* `delete-process` is called, not after.

### 5. Tracing output pollutes stdout (JSON protocol)
- **Symptom**: ANSI-colored tracing lines mixed into JSON-lines, `JSON parse error`
- **Cause**: Fallback tracing subscriber in `init_tracing()` wrote to `io::stdout`
- **Fix**: (a) All console tracing layers now use `io::stderr` (b) Print-mode fallback installs no-op subscriber (c) `--debug` auto-enables stderr logging in print mode (d) Added `:stderr` buffer to `make-process`

### 6. Hook arity mismatch
- **Symptom**: `Wrong number of arguments: #[(_event) ...], 0`
- **Cause**: `llminate-mode--on-start` and `--on-ready` took 1 arg; hooks used `run-hooks` (0 args)
- **Fix**: Changed to 0-arg signatures

### 7. BrokenPipe crash on bridge stop
- **Symptom**: `IO error: Broken pipe (os error 32)` with full stack trace
- **Cause**: `emit_event` used `?` which propagated BrokenPipe as a fatal error
- **Fix**: `emit_event` returns `Result<bool>` — `Ok(false)` on BrokenPipe; all callers in `run_keep_alive` and `process_keep_alive_turn` check the bool and exit cleanly

### 8. User message echoed into assistant response
- **Symptom**: "Hello" prefixed to assistant's streaming text
- **Cause**: Protocol echoes user message as `{"type":"Message","role":"user",...}`; bridge forwarded ALL messages to the response callback
- **Fix**: `handle-message` now filters by role — only `"assistant"` messages are forwarded to the streaming callback

### 9. Explain-region prompt not sent
- **Symptom**: `C-c q e` shows "You:" and empty "Assistant:" but no response
- **Cause**: (a) Race condition — prompts queued during `'starting` phase never drained because `handle-start` didn't drain the queue (only `handle-ready` did). (b) `begin-assistant-turn` called eagerly before knowing if prompt would be sent or queued
- **Fix**: (a) `handle-start` now drains the prompt queue (b) Assistant turn created lazily on first response chunk via `llminate-chat--ensure-assistant-turn`

---

## Known Issues (Open)

### Streaming display performance — FIXED (6 optimizations applied)

**Symptom**: Text appeared in the chat buffer noticeably slower than the actual token arrival rate from the API.

**Root Cause**: Multiple layers of per-token overhead compounded — overlay moves forcing redisplay, elisp JSON parsing, per-token buffer writes, inefficient line splitting, and debug buffer writes on every chunk.

**Fixes Applied** (all 6 implemented):

1. **Dropped the streaming overlay** (llminate-chat.el):
   - **Before**: `move-overlay` called on every token, forcing Emacs to re-render the entire overlay region (which grows with each token — O(n) per token, O(n²) total).
   - **After**: Uses a right-gravity marker (`llminate-chat--stream-insert-marker`). Text is inserted at the marker with `face` text property. No overlay means no per-insertion redisplay of the growing region. Streaming face is removed in one pass at `end-assistant-turn`.

2. **Batched text insertion** (llminate-chat.el):
   - **Before**: Every streaming token triggered: `with-current-buffer` → `inhibit-read-only` → `goto-char` → `insert` → `move-overlay` → potentially hundreds of buffer operations per second.
   - **After**: Tokens accumulate in `llminate-chat--stream-pending` (a string variable). A 30ms `run-at-time` timer (`llminate-chat--flush-stream`) flushes accumulated text into the buffer in a single insert. Reduces buffer operations from per-token to ~33/sec.

3. **Native JSON parsing** (llminate-bridge.el):
   - **Before**: `json-read-from-string` — the pure-elisp JSON parser, called for every streaming token.
   - **After**: `json-parse-string` — the native C implementation (available since Emacs 27). 5-10x faster for small JSON objects. Same keyword-plist output format.

4. **Optimized line buffer** (llminate-bridge.el):
   - **Before**: `(split-string buf "\n")` creates a list of ALL substrings → `(butlast lines)` copies the list minus last → `(car (last lines))` traverses the entire list. Three full-list operations per filter call.
   - **After**: Single-pass `string-search` loop. Only one `substring` call for the remaining tail. No list allocation at all.

5. **Debug buffer writes made conditional** (llminate-bridge.el):
   - **Before**: Every process filter call did `with-current-buffer` + `goto-char` + `insert` into ` *llminate-process*`, even in production.
   - **After**: Gated behind `llminate-bridge-debug-process-output` (defcustom, default nil). Zero overhead when disabled.

6. **Scrolling already batched** (from previous fix):
   - 50ms `run-at-time` timer for auto-scroll, not per-token `with-selected-window` + `recenter`.

**Status**: Needs user testing to confirm improvement. If still slow, remaining options:
- Set `process-adaptive-read-buffering` to coalesce reads at the OS level
- Tune flush interval (currently 30ms — try 50ms for less overhead, or 16ms for smoother display)
- Profile with `M-x profiler-start` to identify any remaining bottleneck

---

## Testing Status

Referencing `EMACS_INTEGRATION_TESTING.md` steps:

| Step | Description | Status |
|------|-------------|--------|
| 1 | Build Rust binaries | PASS |
| 2 | Test `--keep-alive` from terminal | PASS |
| 3 | Completion server health endpoint | Not tested |
| 4 | Elisp batch load (syntax check) | PASS |
| 5 | Interactive: load the mode | PASS |
| 6 | Chat UI (no subprocess) | PASS |
| 7 | Bridge connection + streaming | PASS (6 perf fixes applied, needs retest) |
| 8 | EmacsCommand reverse channel | Not tested |
| 9 | Tool approval | Not tested |
| 10 | IDE layout | Not tested |
| 11 | Code completion | Not tested |
| 12 | Session persistence | Not tested |
| 13 | Editor context (diagnostics, regions) | Partial — explain-region works |

---

## Keybinding Reference

All under `C-c q` prefix:

| Key | Command | Status |
|-----|---------|--------|
| `C-c q q` | Toggle chat panel | Working |
| `C-c q s` | Send prompt | Working |
| `C-c q l` | Toggle IDE layout | Not tested |
| `C-c q r` | Resume session | Not tested |
| `C-c q c` | Command palette | Not tested |
| `C-c q e` | Explain region | Working (after fix #9) |
| `C-c q f` | Fix region | Not tested |
| `C-c q d` | Send diagnostics | Not tested |
| `C-c q .` | Trigger completion | Not tested |
| `C-c q w` | List Emacs commands | Not tested |

---

## Configuration Required

```elisp
;; In ~/.emacs or init.el:
(add-to-list 'load-path "~/.emacs.d/lisp/")
(setq llminate-bridge-executable
      "~/Code/rust_projects/llminate/target/release/llminate")
(require 'llminate-mode)
(llminate-mode 1)
```

---

## Next Priority

1. **Test streaming performance** — 6 optimizations applied, needs user verification
2. Test remaining integration steps (8-13)
3. Polish error handling and edge cases
