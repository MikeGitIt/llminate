# Building a Production-Grade AI Coding Assistant Inside Emacs
## YouTube Tutorial Transcript — llminate + Emacs Deep Integration

---

### INTRO [0:00]

Hey everyone. Today I'm going to walk you through something I've been building — a complete AI coding assistant that runs natively inside Emacs. Not as a plugin that shells out to some web API. Not a thin wrapper around an external tool. This is a full Rust-based agentic engine called **llminate** that communicates bidirectionally with Emacs over a JSON-lines protocol.

In this session, we tackled nine major areas: streaming performance, markdown rendering with three switchable backends, an activity monitor, chat log persistence in multiple formats, AI code completion with CAPF/corfu integration, a security whitelist for reverse Emacs command execution, and critical fixes to the Rust subprocess protocol. Let's dive in.

---

### PART 1: THE ARCHITECTURE [1:30]

Before we get into the changes, let me show you the architecture. llminate is a Rust binary — it's a full port of a production agentic coding assistant. It runs as a long-lived subprocess with `--keep-alive`, speaking JSON-lines over stdin and stdout.

On the Emacs side, we have eight elisp files that form the integration layer:

- **llminate-bridge.el** — the process bridge. It spawns the subprocess, parses JSON-lines, dispatches events.
- **llminate-chat.el** — the chat UI. Streaming display, markdown rendering, prompt history.
- **llminate-layout.el** — the IDE layout. Treemacs, chat panel, activity log, prompt input.
- **llminate-session.el** — session persistence. Save, resume, and now — multi-format chat export.
- **llminate-mode.el** — the global minor mode with all the keybindings.
- **llminate-emacs-commands.el** — the reverse channel. llminate can call Emacs functions.
- **llminate-completion.el** — code completion integration.
- **llminate-session.el** — handles session persistence and resume.

The protocol events flow like this:

```
Emacs → llminate:  Message (user prompts), ToolApprovalResponse, EmacsEvalResult
llminate → Emacs:  Start, Ready, Message, ToolUse, ToolResult, ToolApproval,
                   EmacsEval, Error, End
```

Every token from the LLM comes through as a `Message` event with `role: "assistant"`. Every tool call is a `ToolUse` followed by a `ToolResult`. And llminate can even call Emacs functions via `EmacsEval` — so the AI can run magit commands, navigate buffers, trigger eglot.

---

### PART 2: FIXING THE RUST PROTOCOL LAYER [4:00]

Let's start with the Rust side, because these fixes are foundational. If the protocol isn't clean, nothing else works.

#### Fix 1: Tracing Was Polluting stdout

Here's the problem. llminate uses Rust's `tracing` crate for debug logging. In print mode — which is what the Emacs bridge uses — stdout is reserved exclusively for the JSON-lines protocol. But the tracing initialization had a fallback branch that wrote to `io::stdout()`.

```rust
// BEFORE — the bug in src/main.rs
} else {
    // This fallback fires in print mode because all
    // other branches fail — and it writes to STDOUT!
    registry
        .with(fmt::layer().with_writer(io::stdout).compact())
        .init();
}
```

So Emacs would receive JSON-lines mixed with tracing output like `DEBUG hyper::client ...`. The process filter would try to parse that as JSON and fail.

The fix was two changes:

In `src/cli.rs`, we auto-enable stderr logging when both `--print` and `--debug` are active:

```rust
enable_stderr_logging: Some(self.stderr_logs || (self.print && self.debug)),
```

In `src/main.rs`, the fallback branch now checks for print mode:

```rust
} else if is_print_mode {
    // No tracing output — stdout reserved for JSON protocol
    registry.init();  // no-op subscriber
} else {
    // Non-print mode: stdout is fine
    registry
        .with(fmt::layer().with_writer(io::stdout).compact())
        .init();
}
```

Result: stdout is 100% clean JSON-lines. Stderr is empty by default, and `--debug` routes tracing to stderr.

#### Fix 2: BrokenPipe Crashes

When you stop the bridge from Emacs — `C-c C-c` or killing the buffer — the Rust process would get a BrokenPipe error and crash with a full stack trace.

The fix was in `src/tui/print_mode.rs`. The `emit_event` function now returns `Result<bool>` instead of `Result<()>`:

```rust
async fn emit_event<W: AsyncWrite + Unpin>(
    writer: &mut BufWriter<W>,
    event: &StreamEvent,
) -> Result<bool> {
    match writer.write_all(json.as_bytes()).await {
        Err(e) if e.kind() == ErrorKind::BrokenPipe => return Ok(false),
        Err(e) => return Err(e.into()),
        Ok(()) => {}
    }
    // ... same pattern for newline and flush
    Ok(true)
}
```

Every caller — about 12 call sites in `process_keep_alive_turn` and `run_keep_alive` — checks the boolean and exits cleanly:

```rust
if !emit_event(&mut writer, &event).await? {
    return Ok(false);  // host disconnected, exit gracefully
}
```

Now the process exits with code 0 and no error output.

---

### PART 3: STREAMING PERFORMANCE — SIX OPTIMIZATIONS [8:00]

This was the big one. The user reported that streaming was "HELLA SLOW" — text appeared in the chat buffer much slower than tokens actually arrived. We applied six optimizations across two files.

#### Optimization 1: Drop the Overlay

This was the biggest win. The old code used an Emacs overlay to display streaming text:

```elisp
;; BEFORE — called on EVERY token
(insert text)
(move-overlay ov (overlay-start ov) (point))
```

The problem: `move-overlay` forces Emacs's display engine to re-evaluate the entire overlay region. As the response grows — hundreds of characters, then thousands — each `move-overlay` forces a redisplay of ALL the overlaid text. That's O(n) per token, O(n²) total.

The fix: markers instead of overlays.

```elisp
;; AFTER — marker with right-gravity advances automatically
(setq llminate-chat--stream-insert-marker (copy-marker pos t))
```

Text is inserted at the marker position with face applied as a text property — not an overlay. No `move-overlay`, no per-insertion redisplay of the growing region.

#### Optimization 2: Batched Text Insertion

Instead of inserting every token into the buffer individually, we accumulate them in a string and flush every 30 milliseconds:

```elisp
(defun llminate-chat--stream-chunk (text)
  (setq llminate-chat--stream-pending
        (concat llminate-chat--stream-pending text))
  (unless llminate-chat--stream-flush-timer
    (setq llminate-chat--stream-flush-timer
          (run-at-time 0.03 nil #'llminate-chat--flush-stream))))
```

This drops buffer operations from potentially hundreds per second to about 33 per second. Each flush does one `insert` with one `add-text-properties` call.

#### Optimization 3: Native JSON Parsing

We switched from `json-read-from-string` — the pure-elisp JSON parser — to `json-parse-string` — the native C implementation built into Emacs 27+:

```elisp
;; BEFORE
(let* ((json-object-type 'plist)
       (json-array-type 'list)
       (json-key-type 'keyword)
       (event (json-read-from-string line)))

;; AFTER — 5-10x faster for small objects
(let* ((event (json-parse-string line
               :object-type 'plist
               :array-type 'list
               :null-object nil
               :false-object nil)))
```

For the tiny JSON objects in our streaming protocol, this is 5 to 10 times faster.

#### Optimization 4: Optimized Line Buffer

The old process filter used `split-string`, `butlast`, and `(car (last ...))` — three full-list operations on every filter call. The new version does a single-pass scan:

```elisp
(while (setq nl (string-search "\n" buf start))
  (when (> nl start)
    (llminate-bridge--handle-line (substring buf start nl)))
  (setq start (1+ nl)))
(setq llminate-bridge--line-buffer
      (if (= start 0) buf (substring buf start)))
```

No list allocation. One `substring` for the remaining tail. If no newline was found, no allocation at all.

#### Optimization 5: Conditional Debug Buffer

Every process filter call used to write raw output to the `*llminate-process*` debug buffer — a `with-current-buffer` context switch plus `insert` on every chunk. Now it's gated behind a flag:

```elisp
(defcustom llminate-bridge-debug-process-output nil
  "When non-nil, write raw subprocess output to the process buffer.")
```

Disabled by default. Zero overhead in production.

#### Optimization 6: Scroll Batching

Already done in the previous session, but worth mentioning — auto-scrolling uses a 50ms timer instead of per-token `with-selected-window` + `recenter`.

---

### PART 4: MARKDOWN RENDERING — THREE BACKENDS [14:00]

Raw markdown in a chat buffer looks terrible. Code blocks show as triple backticks. Bold text shows asterisks. Headers show hash marks. We implemented three rendering backends, switchable at runtime.

#### The Architecture

Rendering happens once at the end of each turn — zero per-token cost. The raw markdown is rendered in a temp buffer using the appropriate major mode, then the text with all its properties is copied back into the chat buffer.

```elisp
(defcustom llminate-chat-render-backend 'markdown
  "Backend for rendering assistant markdown responses."
  :type '(choice
          (const :tag "Markdown (gfm-view-mode font-lock)" markdown)
          (const :tag "HTML (shr + pandoc)" shr)
          (const :tag "Org (pandoc + org-mode)" org)))
```

#### Backend 1: Markdown (gfm-view-mode)

This uses the `markdown-mode` package's `gfm-view-mode` — GitHub Flavored Markdown with font-lock syntax highlighting and hidden markup:

```elisp
(defun llminate-chat--render-via-markdown (beg end md-text)
  (let ((rendered
         (with-temp-buffer
           (insert md-text)
           (delay-mode-hooks (gfm-view-mode))
           (setq-local markdown-fontify-code-blocks-natively t)
           (setq-local markdown-hide-markup t)
           (font-lock-ensure)
           (llminate-chat--delete-invisible-text)
           (buffer-string))))
    (delete-region beg end)
    (goto-char beg)
    (insert rendered)
    ...))
```

The key trick: `gfm-view-mode` marks delimiters like `**` and `#` as invisible. But our chat buffer doesn't have the matching `buffer-invisibility-spec`. So `llminate-chat--delete-invisible-text` walks backwards through the buffer and actually deletes those invisible characters. Clean output.

#### Backend 2: SHR (pandoc → HTML)

This pipes markdown through pandoc to get HTML, then uses Emacs's built-in `shr` HTML renderer:

```elisp
(call-process-region (point-min) (point-max)
                     "pandoc" t t nil
                     "-f" "markdown" "-t" "html" "--standalone")
(let ((dom (libxml-parse-html-region (point-min) (point-max))))
  (shr-insert-document dom))
```

This gives the richest visual output — proper spacing, styled code blocks, clickable links.

#### Backend 3: Org (pandoc → org-mode)

Same pattern but targeting org format:

```elisp
(call-process-region ... "pandoc" ... "-f" "markdown" "-t" "org")
(delay-mode-hooks (org-mode))
(font-lock-ensure)
```

You get org-mode's rich fontification — outline navigation, code block highlighting, structured headings.

#### Switching Backends Live

Both user and assistant messages are rendered. The original markdown source is stored in a text property:

```elisp
(put-text-property beg new-end 'llminate-md-source md-text)
```

So you can switch backends at any time and re-render the entire conversation:

```elisp
M-x llminate-chat-set-render-backend RET shr RET
```

All existing responses re-render using the new backend. No data loss.

---

### PART 5: THE ACTIVITY BUFFER [19:00]

The `*llminate Activity*` buffer is part of the IDE layout. It shows a timestamped log of everything happening behind the scenes — tool calls, results, errors, approvals, session events.

The problem was: it was completely empty. The hook handlers that populate it were only registered when the layout was toggled on — and if you reloaded the elisp file, the hooks were lost.

We added handlers for all event types:

```elisp
;; Session lifecycle
(defun llminate-layout--on-start ()
  (llminate-layout-log-activity "Session" (format "Started (model: %s)" ...)))

(defun llminate-layout--on-ready ()
  (llminate-layout-log-activity "Session" "Ready — awaiting input"))

;; User messages
(defun llminate-layout--on-message (role content)
  (when (string= role "user")
    (llminate-layout-log-activity "User" (or content "(empty)"))))

;; Turn completion
(defun llminate-layout--on-end (reason)
  (llminate-layout-log-activity "End" (format "Turn complete%s" ...)))
```

Plus the existing tool handlers for ToolUse, ToolResult, EmacsEval, Error, and ToolApproval.

And the critical fix — auto-re-register on reload:

```elisp
(when llminate-layout--active-p
  (llminate-layout--unregister-hooks)
  (llminate-layout--register-hooks))
```

This runs at load time. If the layout is already active when you reload the file, the hooks are refreshed automatically.

---

### PART 6: CHAT LOG PERSISTENCE [22:00]

This is where it all comes together. Every conversation is now automatically saved in three formats.

#### The Message Log

As messages flow through the chat UI, we accumulate a structured log:

```elisp
(defvar llminate-chat--message-log nil
  "Ordered list of (:role :content :timestamp) plists.")
```

User messages are logged in `llminate-chat-send`. Assistant messages are logged in `end-assistant-turn` from the bridge's accumulated text.

#### Three-Format Export

After each assistant turn, a hook fires that exports the entire conversation:

```elisp
;; Builds a clean markdown document with YAML frontmatter
(defun llminate-session--build-markdown ()
  (with-temp-buffer
    (insert "---\nsession: ...\nmodel: ...\ndate: ...\n---\n\n")
    (dolist (msg messages)
      (insert (format "## %s — %s\n\n%s\n\n---\n\n" role timestamp content)))
    (buffer-string)))
```

That markdown is saved as `{session_id}.md`, then piped through pandoc twice:

```
pandoc -f markdown -t html --standalone  →  {session_id}.html
pandoc -f markdown -t org               →  {session_id}.org
```

All three files land in `.claude/conversations/` alongside the existing Rust-side JSON:

```
.claude/conversations/
├── abc123.json   ← Rust engine (raw message data)
├── abc123.md     ← Clean markdown transcript
├── abc123.html   ← Standalone HTML (shareable)
└── abc123.org    ← Org-mode (Emacs-native)
```

The HTML version is standalone — you can open it in a browser and share it. The org version opens natively in Emacs with full outline navigation. The markdown is clean and readable anywhere.

#### Auto-Save

Export happens automatically:

```elisp
;; After each turn
(add-hook 'llminate-chat-turn-end-hook #'llminate-session--auto-export-chatlog)

;; On Emacs exit
(add-hook 'kill-emacs-hook #'llminate-session--auto-save)
```

You can also trigger it manually with `M-x llminate-session-export-chatlog`.

---

### PART 7: THE USER MESSAGE FIX [25:00]

One thing that bugged the user — and rightfully so — was that markdown rendering only applied to assistant responses. If you pasted a code block in your prompt, it showed as raw triple-backtick text while the assistant's code blocks were beautifully rendered.

Simple fix — apply the same rendering to user messages in `llminate-chat-send`:

```elisp
(let ((msg-beg (point)))
  (insert prompt "\n")
  (llminate-chat--render-response msg-beg (point)))
```

Now both sides of the conversation look consistent.

---

### PART 8: AI CODE COMPLETION [27:00]

This is one of the most powerful features of the integration — AI-powered code completion that plugs directly into Emacs's native `completion-at-point` framework and works with corfu.

#### How It Works

llminate-completion.el manages a separate completion server — `ruemacs_completion_server` — as a subprocess. This server runs on localhost, accepts HTTP POST requests with editor context, and streams back completions via Server-Sent Events (SSE).

The flow looks like this:

```
You type code → 0.5s idle debounce → collect EditorContext
  → HTTP POST localhost:3000/complete → SSE response
  → parse completion text → inject into CAPF → corfu popup
```

#### The EditorContext

When a completion is triggered, the system snapshots your entire editing state:

```elisp
(defun llminate-completion--collect-context ()
  "Build an EditorContext plist from the current buffer state."
  `((file_path . ,file-path)
    (language_id . ,lang-id)        ; "rust", "python", "elisp", etc.
    (content . ,content)             ; full buffer text
    (cursor_position . ((line . ,line) (character . ,character)))
    ,@(when selection
        `((selection_range . ,selection)))))
```

This includes the file path, language ID (mapped from 30+ major modes), buffer content, cursor position, and active selection range. The server gets complete context to generate relevant completions.

#### Language Detection

The system maps Emacs major modes to standard language identifiers:

```elisp
'((rust-mode         . "rust")
  (rust-ts-mode      . "rust")
  (python-mode       . "python")
  (emacs-lisp-mode   . "elisp")
  (go-mode           . "go")
  (typescript-ts-mode . "typescript")
  ...)  ; 30+ modes including tree-sitter variants
```

If your mode isn't in the list, it falls back to stripping `-mode` and `-ts` suffixes — so `zig-mode` becomes `"zig"` automatically.

#### Debounced Requests

You don't want to fire an AI completion request on every keystroke. The system uses an idle timer:

```elisp
(defcustom llminate-completion-debounce-delay 0.5
  "Seconds of idle time before triggering an AI completion request.")
```

Only after 0.5 seconds of no typing does the request fire. And it only fires in `prog-mode` buffers with a file on disk — no completions in scratch buffers or minibuffers.

#### CAPF Integration

The completion function plugs into Emacs's standard `completion-at-point-functions`:

```elisp
(defun llminate-completion-capf ()
  "A completion-at-point function providing AI completions via corfu."
  (when (and (not (minibufferp))
             (derived-mode-p 'prog-mode)
             (buffer-file-name))
    ...
    (list start end
          (or llminate-completion--candidates
              (lambda (_string _predicate action) ...))
          :exclusive 'no
          :annotation-function (lambda (_cand) " [AI]"))))
```

The `:exclusive 'no` means it doesn't block other completion sources — eglot/LSP completions still work. AI completions show up alongside them, annotated with `[AI]` so you know which is which. And it's added with priority 90 — after eglot — so LSP completions take precedence.

#### Multi-Provider Support

The server supports both OpenAI and Anthropic as backends:

```elisp
(defcustom llminate-completion-provider "openai"
  "AI provider to use (\"openai\" or \"anthropic\").")
```

You can tune temperature and max tokens:

```elisp
(setq llminate-completion-temperature 0.2)   ; deterministic
(setq llminate-completion-max-tokens 256)     ; keep completions focused
```

#### Server Management

The completion server starts automatically when needed, but you can manage it manually:

```
M-x llminate-completion-start-server   ; start manually
M-x llminate-completion-stop-server    ; stop
M-x llminate-completion-enable         ; enable in all prog-mode buffers
M-x llminate-completion-disable        ; disable everywhere
```

Or trigger completion on demand with `C-c q .` — which calls `completion-at-point`.

---

### PART 9: EMACS COMMAND SECURITY WHITELIST [31:00]

Here's something unique about this integration. llminate doesn't just send text to Emacs — it can call Emacs functions directly via the `EmacsEval` protocol event. The AI can run magit commands, navigate to definitions, format buffers, check diagnostics.

But you obviously can't let an AI call arbitrary Emacs functions. That's where `llminate-emacs-commands.el` comes in — a whitelist-based security layer.

#### Three Security Levels

Every command has one of three security levels:

| Level | Behavior |
|-------|----------|
| `allow` | Execute immediately, return result |
| `prompt` | Show approval dialog before executing |
| `deny` | Never execute, return error |

Commands not in the whitelist are implicitly denied.

#### The Registry

The whitelist covers six categories of operations:

```elisp
;; File / Buffer operations — mostly 'allow'
("find-file" . allow)  ("save-buffer" . allow)  ("buffer-string" . allow)

;; Magit (git) — destructive ops require 'prompt'
("magit-status" . allow)  ("magit-commit" . prompt)  ("magit-push" . prompt)

;; Eglot / LSP — renames require 'prompt'
("xref-find-definitions" . allow)  ("eglot-rename" . prompt)

;; Compilation
("compile" . prompt)  ("recompile" . allow)

;; Project operations — all read-only, all 'allow'
("project-root" . allow)  ("project-files" . allow)

;; Read-only queries — always safe
("point" . allow)  ("line-number-at-pos" . allow)
```

The philosophy: read-only operations are `allow`. Anything that changes state or is visible externally is `prompt`. Anything dangerous stays out of the whitelist entirely.

#### The Approval Dialog

When llminate tries to call a `prompt`-level command, you see:

```
[llminate] Execute `magit-commit'? (y)es (n)o (a)lways:
```

Three choices:
- **y** — approve this one time
- **n** — deny this request
- **a** — upgrade this command to `allow` permanently (for the session)

#### Result Serialization

Emacs values don't map directly to JSON. The serializer handles the conversion:

- Buffers → their name (string)
- Markers → their position (number)
- Symbols → their name (string)
- Hash tables → JSON objects
- Alists → JSON objects
- Regular lists → JSON arrays
- nil → null, t → true

So when llminate calls `(buffer-file-name)` and gets `#<buffer init.el>`, the response goes back as `"init.el"` — clean JSON that the Rust engine can process.

#### Managing the Whitelist

You can view and modify the whitelist at runtime:

```
C-c q w                               ; list all registered commands
M-x llminate-emacs-commands-list       ; same thing
```

This opens a formatted buffer showing every command and its security level. You can also add commands programmatically:

```elisp
(llminate-emacs-commands-add "my-custom-function" 'allow)
(llminate-emacs-commands-remove "dangerous-function")
```

#### The Modeline Shows It

When llminate calls an Emacs function, the modeline updates in real-time:

```
llm[emacs:magit-stat]   ; truncated to 12 chars for readability
```

So you always know what the AI is doing inside your editor.

---

### PART 10: RECAP & DEMO [34:00]

Let me show you all of this working together.

**Step 1**: Start Emacs, toggle the IDE layout with `C-c q l`. You see treemacs on the left, your editor in the center, the chat log on the right, the activity buffer at the bottom left, and the prompt input at the bottom right.

**Step 2**: Type a prompt in the input buffer, hit `C-c C-c`. Watch the activity buffer — it logs "User: your prompt". The assistant starts streaming. Text appears smoothly — no freezing, no lag — thanks to the batched insertion and marker-based streaming.

**Step 3**: When the response completes, markdown rendering kicks in. Code blocks get syntax highlighting. Bold text is actually bold with the delimiters hidden. Headers are sized and colored. Lists have proper bullet characters.

**Step 4**: Check the activity buffer — it shows "End: Turn complete" and "Session: Ready — awaiting input".

**Step 5**: Switch rendering backends: `M-x llminate-chat-set-render-backend RET shr RET`. Every response in the conversation re-renders through pandoc and shr. Rich HTML rendering right in your Emacs buffer.

**Step 6**: Check `.claude/conversations/`. Your conversation is saved as `.md`, `.html`, and `.org`. Open the HTML in a browser — it's a clean, shareable transcript. Open the `.org` in Emacs — full org-mode with outline folding.

**Step 7**: Open a Rust file and start typing. After half a second of idle time, the completion server kicks in — corfu shows an `[AI]`-annotated suggestion alongside your LSP completions. Press `C-c q .` to trigger it manually.

**Step 8**: Check the modeline. It shows `llm[idle]` when waiting, `llm[streaming]` during a response, `llm[tool:Bash]` when executing a tool, and `llm[emacs:magit-stat]` when calling Emacs functions. The command whitelist ensures the AI only calls functions you've approved — `C-c q w` shows the full registry.

**Step 9**: Try the command palette with `C-c q c`. It gives you a completing-read menu of every llminate command — start/stop server, switch render backend, list sessions, explain region, and more.

**Step 10**: Stop the bridge with `C-c C-c`. The Rust process exits cleanly — no crash, no stack trace, exit code 0.

---

### SUMMARY [36:00]

Here's what we built in this session:

| Area | Changes | Impact |
|------|---------|--------|
| **Rust Protocol** | Clean stdout, graceful BrokenPipe | Reliable subprocess communication |
| **Streaming** | 6 optimizations | Smooth, lag-free token display |
| **Markdown** | 3 switchable backends | Professional-quality rendering |
| **Activity** | Full event logging | Visibility into tool execution |
| **Persistence** | 3-format auto-export | Shareable, searchable chat history |
| **Consistency** | User message rendering | Both sides look good |
| **Completion** | CAPF/corfu + completion server | AI code completion in any prog-mode buffer |
| **Security** | Whitelist-based command execution | Safe reverse channel (AI → Emacs) |

Eight elisp files form the integration layer, backed by Rust protocol fixes. The completion system adds a dedicated server for AI-powered code completion that works alongside eglot/LSP. The command whitelist ensures llminate can leverage Emacs's full power — magit, eglot, project.el — without compromising security.

This is what I love about Emacs — it's not just a text editor. It's a platform. And with a proper bidirectional protocol, you can build deeply integrated tools that feel native. The AI doesn't just talk to you in a chat buffer — it can read your diagnostics, navigate your code, stage your commits, and complete your code, all with security controls you define.

Thanks for watching. If you want to try this yourself, all the code is in the llminate repository. Drop a comment if you have questions.

---

### KEYBINDING REFERENCE

| Key | Command | What It Does |
|-----|---------|-------------|
| `C-c q q` | Toggle chat panel | Show/hide the chat side windows |
| `C-c q l` | Toggle IDE layout | Full IDE layout with activity buffer |
| `C-c q s` | Send prompt | Send from any buffer |
| `C-c q e` | Explain region | Send selected code for explanation |
| `C-c q f` | Fix region | Send selected code for fixing |
| `C-c q r` | Resume session | Pick and resume a saved session |
| `C-c q d` | Send diagnostics | Send flymake/eglot diagnostics |
| `C-c q .` | Completion at point | Trigger AI code completion |
| `C-c q w` | Command whitelist | List allowed Emacs commands |
| `C-c q m` | Render backend | Switch markdown rendering backend |
| `C-c q c` | Command palette | Completing-read of all commands |
| `C-c C-c` | Send (in prompt) | Send the prompt buffer contents |
| `C-c C-c` | Stop (in chat) | Stop the llminate subprocess |
| `M-p / M-n` | History nav | Navigate prompt history |

### CUSTOMIZATION REFERENCE

```elisp
;; Markdown rendering backend
(setq llminate-chat-render-backend 'markdown)  ; or 'shr or 'org

;; Auto-export chat logs after each turn
(setq llminate-session-chatlog-auto-save t)

;; Debug subprocess output (off by default for performance)
(setq llminate-bridge-debug-process-output nil)

;; Pandoc path (if not in PATH)
(setq llminate-session-pandoc-executable "/opt/homebrew/bin/pandoc")

;; AI completion — provider and tuning
(setq llminate-completion-provider "openai")       ; or "anthropic"
(setq llminate-completion-temperature 0.2)         ; lower = more deterministic
(setq llminate-completion-max-tokens 256)           ; max tokens per completion
(setq llminate-completion-debounce-delay 0.5)       ; seconds of idle before request
(setq llminate-completion-context-lines 50)         ; lines of surrounding context
(setq llminate-completion-server-port 3000)         ; completion server port

;; Enable/disable AI completion globally
;; (llminate-completion-enable)   ; add to prog-mode-hook
;; (llminate-completion-disable)  ; remove from all buffers

;; Command whitelist — add custom commands
;; (llminate-emacs-commands-add "my-function" 'allow)
;; (llminate-emacs-commands-add "risky-function" 'prompt)
```
