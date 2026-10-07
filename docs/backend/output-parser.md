# Output Parser

**Module:** `src-tauri/crates/tuic-terminal/src/output_parser.rs`

Parses terminal output to detect structured events: rate limits, status lines, PR URLs, and progress indicators.

For mobile choices, `choice-prompt` includes an optional `selection_mode`.
The Claude-specific Ink AskUserQuestion parser requires a full-bleed
`Enter to select` footer and reads numbered options across description rows;
it emits `navigate-enter`, so the client sends arrow keys followed by Enter.
Other numbered confirmations retain their existing key selection path. The
screen trim keeps an open Ink dialog intact because its highlighted option
also begins with the composer glyph `❯`.

Terminal-grid row snapshots retain the base character and all stored zero-width
characters. A combining mark received in a later PTY chunk marks its base cell
dirty, so changed-row consumers receive the updated text. This preserves the
stored sequence without normalizing it or changing parser matching rules.

## Usage

```rust
let parser = OutputParser::new();
let events: Vec<ParsedEvent> = parser.parse(terminal_output);
```

## ParsedEvent Variants

### RateLimit

Detected when terminal output matches known rate limit patterns from AI agents:

```rust
ParsedEvent::RateLimit {
    pattern_name: String,    // e.g., "claude_rate_limit"
    matched_text: String,    // The matched text
    retry_after_ms: Option<u64>, // Parsed retry delay
}
```

### StatusLine

Agent status output (e.g., token usage, timing):

```rust
ParsedEvent::StatusLine {
    task_name: String,
    full_line: String,
    time_info: Option<String>,
    token_info: Option<String>,
}
```

### PrUrl

Pull request URL detected in output:

```rust
ParsedEvent::PrUrl {
    number: u32,     // PR number
    url: String,     // Full URL
    platform: String, // "github", "gitlab", etc.
}
```

### Progress

OSC 9;4 progress indicator:

```rust
ParsedEvent::Progress {
    state: u8,  // 0=remove, 1=set, 2=error, 3=indeterminate, 4=warning
    value: u8,  // 0-100 progress percentage
}
```

### Question

Agent is waiting for user input (question, confirmation, menu choice):

```rust
ParsedEvent::Question {
    prompt_text: String,  // The detected prompt line
    confident: bool,      // Protocol-backed signals are high-confidence
}
```

The emitted JSON also carries the internal `_turn_epoch`. The authoritative
session reducer ignores question and retraction events from an older input turn.

Question events have two sources:

- **Response-required OSC 777 notifications** are parsed from the raw PTY byte
  stream before VT rendering consumes the escape sequence. Only explicit
  permission or approval wording is accepted. Generic desktop notifications
  such as `Claude Code needs your attention` and `Claude is waiting for your
  input` do not prove that the composer awaits a response; the latter also
  follows an ordinary completed turn and caused a transient false badge.

  Every qualifying notification in a raw chunk is retained in stream order;
  a later OSC sequence cannot overwrite an earlier approval request merely
  because both arrived in one OS read.

  | Body | Confidence | Why |
  |------|-----------|-----|
  | `needs your permission`, `approval required` | high | A request with one reading. Cleared by the answer. |
  | `is waiting for your input` | ignored | The ready composer can produce the same notification. A visible Ink footer or choice prompt supplies the picker signal. |

  A high-confidence question is retracted by nothing but real user input; the
  generic body once latched the badge on a session that had finished 17h earlier
  (observed 2026-08-11).
- **Screen-verified silence detection** handles rendered questions (all instant
  regex patterns were removed due to false positives from Ink agent streaming):

1. `extract_question_line()` scans changed terminal rows for `?`-ending lines, applying content filters to reject code comments (`//`), markdown headers (`#`), diff context (`+/-`), code syntax (`->`, `=>`, `::`, `)?`), and wrapped `suggest: [ … ]` protocol items
2. `SilenceState` stores the candidate and starts a 10s silence timer
3. When the timer fires, a visible input box restricts detection to the latest chat content above that prompt; only an unanchored screen may use the bounded changed-row fallback
4. If verified, emits `ParsedEvent::Question { confident: false }`

- **The Ink dialog footer** (`Enter to select · …`) is the one surviving instant
  pattern, and it emits `confident: true`. It is matched at **column 0 of the
  rendered row**, not of the trimmed text, by `is_ink_dialog_footer_row()`. A
  dialog is drawn full-bleed by the TUI, while everything an agent *streams* —
  prose, code blocks, a screen it read and quoted back — is indented inside the
  agent's own frame. That indentation is the only difference between the footer
  and a copy of it: the text is otherwise identical byte for byte. Anchoring on
  the trimmed row let an agent that pasted another session's screen latch
  `question_confident` on its own tab, which nothing retracts, so the tab claimed
  the agent was blocked on the user for the rest of the turn while it worked
  (observed 2026-08-30). `ink_dialog_footer()`, which reads the same row off the
  full screen as a *level* for `rearm_awaiting_for_open_dialog`, shares the same
  predicate on purpose — the two must never disagree about what a footer is.

Guards against false positives:
- **Spinner suppression**: If a status-line event was seen within the last 10s, detection is suppressed
- **Staleness counter**: If >10 non-`?` output chunks arrived after the candidate, it's considered stale
- **Screen verification**: Candidate must still be among the last 5 visible lines at fire time
- **User echo suppression**: 500ms window after user input ignores PTY echo of typed text
- **Resize grace**: 1s suppression after terminal resize to avoid re-detection of redrawn content

Hook-instrumented agents report explicit question-tool waits through OSC 7770
`state=awaiting`. Plan and skill Ink pickers are detected by their rendered
footer, including the full-screen presence recovery when a hook does not fire.
OSC 777 desktop notifications bypass heuristic-question suppression only for
unambiguous permission or approval wording. Claude's generic `is waiting for
your input` body also appears after a completed turn at the ready composer, so
it cannot set awaiting by itself.

The full-screen Ink-footer recovery also applies with hooks enabled. A later
busy hook can clear awaiting while the dialog is still open. Before recovery,
the reader evaluates the last pending Question/UserInput event in the current
chunk: a Question followed by UserInput is a clear, not an existing question.
An already-active badge or pending question suppresses duplicate recovery;
indented quoted footers remain excluded. Once the dialog closes, normal user
input and protocol transitions clear the badge.

### QuestionCleared

The retraction of a low-confidence `Question`. It carries no payload:

```rust
ParsedEvent::QuestionCleared  // wire type: "question-cleared"
```

Emitted by the silence timer in `pty.rs`, never by a parser: it means "the
screen is quiet and the tracked question is no longer the current chat prompt".
Retraction runs independently from the one-shot emission gate. Bare Enter now
produces the same `user-input` clear and turn transition as every other input
transport; retraction remains the backstop when output changes without input.

`emit_question_cleared_if_stale()` fires only when `awaiting_input &&
!question_confident && choice_prompt.is_none()`, and the `state.rs` arm
re-checks `question_confident` before clearing. Confident questions stay sticky
on purpose: grok repaints while it waits, so absence from the current screen is
not proof that it was answered.

Claude can dismiss `AskUserQuestion` with a bare Esc, which produces no typed
line. When the new output contains `User declined to answer questions` and the
full screen has returned to the ready composer without an open Ink dialog,
`pty.rs` emits `protocol-question-cleared` with the expected question text and
turn epoch. A still-open dialog or a new question in that output chunk remains
awaiting; the reducer checks the expected text and epoch before applying the
clear.

A `progress type=blocked` report from an agent with a live PTY emits a
confident `question` with `source: "progress-blocked"`, and the reducer records
that source on the awaiting evidence. When the same PTY later reports `done`, or
journals a `delegated` hand-off, `emit_progress_entry` emits
`progress-superseded` for that PTY. The reducer clears the badge only when the
current awaiting evidence still has the `progress-blocked` source: a dialog, a
choice prompt or another confident question records a different source and
stays awaiting. A new `blocked` report replaces the question text.

### Raw Capture Regression Fixtures

Agent-state failures must be captured from the raw PTY stream before analysis.
Enable `POST /diagnostics/capture` before reproducing, stop it afterward, then
copy the reported capture file (under `<config dir>/captures/` by default, or
under the absolute `TUIC_CAPTURE_DIR` override) into
`src-tauri/src/fixtures/agent_prompts/`. `/sessions/:id/output` is not a fixture
source: it is a rendered, bounded ring snapshot and can lose one-shot escape
sequences. TUICCAP2 fixtures preserve the initial terminal geometry,
input/output direction, original chunk boundaries, ordering, and monotonic
timestamps. The decoder still accepts geometry-less TUICCAP1 and output-only
`.raw`; their replay must supply independently observed dimensions. Replay also applies the production chrome
cutoff before the rendered-row parser and hook suppression.

Output-only fixtures cannot express a missing input-side CLEAR. Framed `.tcap`
fixtures can reconstruct submitted lines, while the `Awaiting RETRACTION` block
still drives the real event-bus accumulator and asserts `SessionState` directly.

`src-tauri/src/fixtures/agent_prompts/scenario-matrix.json` is the coverage
contract derived from the local Claude and Codex transcript corpora. Histories
contribute semantic shapes (question tools, lifecycle start/complete/abort), not
terminal bytes. Each scenario therefore identifies whether its evidence is a
real raw fixture, a controlled capture template, a runtime regression, or a
synthetic concurrency/state case. A test enforces unique IDs, Claude and Codex
coverage, fixture existence, and the invariant that every state SET declares at
least one CLEAR path.

### ApiError

API errors from agents and providers (5xx server errors, auth failures):

```rust
ParsedEvent::ApiError {
    pattern_name: String,    // e.g., "claude-api-error", "openai-server-error"
    matched_text: String,    // The matched text
    error_kind: String,      // "server", "auth", or "unknown"
}
```

Detects errors from two tiers:
- **Agent-specific**: Claude Code, Aider, Codex CLI, Gemini CLI, Copilot CLI (note: the generic "request failed unexpectedly" pattern was removed from Copilot detection due to false positives on Claude Code output)
- **Provider-level**: OpenAI, Anthropic, Google, OpenRouter, MiniMax JSON error structures

Claude Code has two 5xx renderings and both are covered. `claude-api-error` matches
the JSON body (`"type":"api_error"`); `claude-server-error-friendly` matches the
prose-only form (`API Error: 500 Internal server error. This is a server-side issue…`),
anchored on the status code because the prose wraps at the terminal width and rendered
rows are joined with `\n`. The friendly pattern requires a letter after the code, so
the JSON variants above it keep priority and `API Error: 529` stays a rate limit.

`error_kind: "server"` is what drives auto-retry in `Terminal.tsx` (inject `continue`,
backoff 5s/15s/30s), gated per agent by `auto_retry_on_error` — **default `false`**.

Frontend plays an error notification sound and logs via `appLogger.error()`.

### Intent

Agent-declared intent — what the LLM is currently working on:

```rust
ParsedEvent::Intent {
    text: String,   // Short action description
    title: Option<String>,  // Optional tab title from (parenthesized) suffix
}
```

Detected as a plain-prefix token at the start of a row: `intent: <text> (<title>)`, optionally behind an agent bullet glyph or the wrap indent.

AI Chat receives ACP markdown chunks in both desktop and browser mode, without
terminal rows or wrap metadata. `protocolText.ts` interprets the joined answer
for that UI: it removes the connection acknowledgement, presents anchored
`intent:` as status, and exposes bounded bracketed `suggest:` items as replies.
It takes the title from the consecutive parenthesized marker prefix, preserving
parenthesized descriptions and same-line reply text, including replies ending in
parentheses. Ambiguous unbalanced or nested markers remain literal body text.
It leaves fenced or indented code and prose mentions literal. This is a separate
logical-line projection of the terminal grammar: reusing the Rust VT parser here would require
a new backend ACP projection and wire contract, including HTTP/IPC parity.

Two shapes the row-anchored regex cannot read on its own, both rejoined before it runs:

- **After the ack sentence.** The protocol puts the ack and the first `intent:` in the same message by construction, so an agent that writes them as one sentence run leaves the token mid-row. `ACK_SENTENCE_PREFIX` allows that one sentence and nothing else — any other leading prose is still rejected, so `The intent: of this code` stays prose.
- **Wrapped across physical rows.** `suggest:` is bounded by its closing `]`; `intent:` has no terminator, so the regex `$` cuts the token at the wrap and drops the `(title)` — the tab title — with it. `dewrap_intent_continuation` rejoins indented rows only when the preceding row plus the next word would exceed the terminal width. The parser's 2048-character budget also bounds soft-wrap traversal, allowing a 12-row intent in a narrow pane. The input-box chrome cutoff bounds continuation reads. Blank rows, bullets, prompt and chrome glyphs, and a closed `(title)` stop the join. Closing a title-less intent removes an unfinished `(` title fragment.

The PTY reader retains one open intent per session while a TUI redraws growing
prefixes. It reconstructs soft-wrapped grid lines and joins width-verified
hard-wrap rows before parsing. It reads continuation rows lazily and caches
each anchor for one reader tick, so a repaint below an intent does not
repeatedly scan the same grid rows. The grid reconstruction budget matches
the parser's 2048-character candidate budget, including UTF-8 and terminal
padding. Long intent text is redacted and truncated to the Progress journal
limit, while the parsed event keeps the full text. An unfinished `(` suffix
is recorded as titleless text when a different intent replaces it. The reader
journals on a closed `(title)`, a VTE hard
line break that did not grow the candidate, subsequent prose below the line,
replacement of the line, or an idle turn boundary after output has gone quiet.
Cursor moves, carriage returns, and growing Ink frame repaints do not close an
intent. A repaint that keeps the anchor line while temporarily replacing its
continuation also keeps the open candidate until the completed marker arrives.
A pending intent is flushed when its session is removed. Repainting the
last recorded value adds no journal row; a different intervening intent lets
the earlier value be recorded again.

Agents receive this instruction automatically via MCP init. To use manually without MCP, add to CLAUDE.md or equivalent:

```
## Intent Declaration
At the start of every user task and each distinct material work phase, emit on its own line:
intent: <action, present tense, <60 chars> (<tab title, max 3 words>)
Example: `intent: Reading auth module for token flow (Auth review)`
```

The terminal Context bar shows intent separately from the orchestrator assignment and user prompt. The activity dashboard also shows intent (crosshair icon) when available, falling back to user prompt (speech bubble) otherwise.

The frontend also reconciles `agent_intent` and `last_prompt` from authoritative
session snapshots, both during catch-up and on `session-state-changed`. Captured
context therefore does not depend on receiving the original parsed event. The
last substantial prompt still requires at least ten whitespace-separated words;
shorter submissions leave the previous qualifying prompt in place.

**Colorization:** `colorize_intent()` wraps intent text in `\x1b[2;33m` (dim yellow) for the terminal output stream. The optional `(title)` suffix is stripped from the display. Colorization is agent-gated to prevent false positives.

**PWA/REST stripping:** `LogLine::strip_structural_tokens()` removes `intent:` / `suggest:` plain-prefix tokens from log line spans before serving to mobile/browser clients. It delegates to `output_parser::strip_plain_prefix_tokens`, which is built from the same bullet class and ack prefix the parser anchors on — a second copy of the grammar lived in `vt_log.rs` and drifted, so Codex-bulleted tokens were parsed by TUIC and then shown to the user anyway. The ack sentence is kept; only the marker behind it is removed.

**Active subtask detection:** The output parser recognizes `⏵⏵` (U+23F5) and `››` (U+203A) mode-line prefixes as active subtask indicators. The `active_sub_tasks` count is tracked in `SessionState` and used to suppress premature completion notifications.

### PlanFile

Plan file path detected in agent output:

```rust
ParsedEvent::PlanFile {
    path: String,  // Absolute path to the plan file
}
```

### Suggest

Agent-proposed follow-up actions:

```rust
ParsedEvent::Suggest {
    items: Vec<String>,  // e.g., ["Run tests", "Review diff", "Deploy"]
}
```

Detected as a plain-prefix token at column 0: `suggest: [ A | B | C ]`.

**Keyword rejoin (narrow panes):** in a narrow pane the wrap can fall inside the keyword itself, so `dewrap_suggest_keyword` rejoins every split position (`s\nuggest:` … `suggest\n:`) before the regex runs. The tail is accepted behind the agent's own hanging wrap indent — Codex soft-wraps its output with two leading spaces and emits `• suggest` / `  : [ … ]`, verified live at 9 columns — while the head must still start at column 0, optionally after whitespace or an agent bullet, so prose ending in a partial word is never rewritten. The rejoin buffer is allocated only when a match qualifies: the scan keys on `prefix + newline`, which any line ending in `s` hits, and ordinary chunks must stay on the `Cow::Borrowed` path.

**Colon-alone rows:** at the narrowest widths the bullet plus `suggest` fills the row on its own, so the continuation row carries only `:` and the bracket body starts a row later (`• suggest` / `  :` / `  [ A`, captured live at 9 columns). The keyword rejoin then produces a `suggest:` with nothing after it, so `dewrap_suggest_content` matches a trailing whitespace run of `[\t ]*`, not `[\t ]+` — the pull to the next row must not require a space that this shape never has. The column-0 anchor, the bracket body and the 2–4 item count remain the guards that keep prose out.

One bounded logical line may soft-wrap across terminal rows and may begin with any parser-supported agent bullet (`●`, `⏺`, `•`, or `◦`), but the bracketed content may not contain a nested `[`/`]`. The closing bracket must be at or before the cursor; cells to the right of the cursor are ignored so stale content left by a carriage-return overwrite cannot complete a partial token. Reconstruction follows at most four soft-wrap transitions and 512 bytes. If those bounds or cursor metadata prevent reconstruction, the cursor-row structural candidate is rejected rather than parsed from rendered cells. Items are pipe-delimited (2–4 per the protocol). Parsing is agent-gated; the raw token is stripped from the log delivered to PWA/REST consumers by `strip_structural_tokens`, and concealed on the desktop canvas by the frontend overlay.

**Mobile wrapped-token masking:** both screen and scrollback log reads run `vt_log::strip_structural_blocks` before delivery. It uses `output_parser::suggest_tail_rows` to remove continuation rows only when a bracketed token closes within the parser's wrap bound; blank rows, nested brackets and new protocol tokens stop the scan. An unclosed token does not consume unrelated output. Suggestion chips share the Rust parser across desktop and mobile; desktop canvas masking remains in `suggestOverlay.ts`, separate from the mobile log filter (#1380-05d8).

### UsageLimit

Claude Code usage limit percentage:

```rust
ParsedEvent::UsageLimit {
    percentage: u8,      // 0-100
    limit_type: String,  // "weekly" or "session"
}
```

Detected via regex matching `"You've used X% of your weekly/session limit"`. Supports both ASCII and Unicode smart-quote apostrophes (`'` and `\u{2019}`).

### UsageExhausted

Claude Code usage fully exhausted (no remaining quota):

```rust
ParsedEvent::UsageExhausted {
    reset_time: Option<String>,  // Raw text, e.g. "8pm (Europe/Madrid)"
}
```

Detected via `"out of (extra) usage"` pattern. The optional `reset_time` is extracted from `"· resets <text>"` suffix. The raw string is passed to plugins for scheduling; no timezone parsing is done in Rust.

### ActiveSubtasks

Agent sub-task indicator from `›› task · N local agents` mode-line:

```rust
ParsedEvent::ActiveSubtasks {
    count: u32,       // Number of active sub-tasks (0 = all finished)
    task_type: String, // "local agents", "bash", "background tasks", etc.
}
```

### ShellState

Shell activity state derived from PTY output timing:

```rust
ParsedEvent::ShellState {
    state: String, // "busy" | "idle"
}
```

Emitted by the reader thread on real-output→busy and idle transitions. The frontend consumes this instead of deriving busy/idle from raw PTY data. See `docs/backend/pty.md` for idle detection details.

The backend foreground probe enables agent parsing for hand-launched agents in both desktop and headless sessions. HTTP foreground queries update the same stored identity. Screen activity cached before identity discovery is reclassified when that identity changes.

Ordinary output and spinner repaints cannot override a protocol idle marker.
The reader preserves that evidence until the ranked busy decision, and the
ready-screen timer preserves its rank. A new submission or accepted semantic
working marker can start activity again.

`session action=submit` uses these existing agent screen adapters only to label
an acknowledgement after the raw child-output ring moves beyond its pre-Enter
offset (`working_screen`, `ready_screen`, `interrupted_screen`, or the generic
`terminal_output`). It does not add a parsed event or a second lifecycle state
machine. The output movement is independent of local input bookkeeping and is
therefore a valid terminal receipt, but it is not semantic application
acceptance.

### AgentSessionConflict

Claude Code startup session-id failure:

```rust
ParsedEvent::AgentSessionConflict {
    matched_text: String,
    kind: String,  // "in-use" | "not-found"
}
```

Fired when the PTY emits either:
- `Session ID <uuid> is already in use.` — the injected `--session-id` collides with a live process or stale lock.
- `No conversation found with session ID: <uuid>` — a `--resume <uuid>` pointed at a missing session file (usually wrong config dir, e.g. running `c` where the session lives under `c2`'s `~/.claude-private`).

**Auto-reset behaviour:** on this event the reader thread writes a fresh `export TUIC_SESSION=<new-uuid>` (or `set -gx` under fish) into the PTY so the shell wrapper's `--session-id` auto-injection stops wedging the tab on the stale id. Guarded by a 3-second cooldown — Claude prints the error line several times as it exits, but only the first fires the reset. The frontend raises a warn toast so the user knows what happened.

### ChoicePrompt

Numbered confirmation / multiple-choice menu rendered by Claude Code (`Esc to cancel · Tab to amend`) or an opened Codex `request_user_input` panel (`enter submit   ctrl+] skip   ⌥+↓ main prompt`):

```rust
ParsedEvent::ChoicePrompt {
    title: String,                 // The question above the options
    options: Vec<ChoiceOption>,    // { index, label, destructive }
    dismiss_key: Option<String>,   // e.g. "cancel"
    amend_key: Option<String>,     // e.g. "amend"
}
```

**Detection:**
- **Footer match** extracts `dismiss_key` / `amend_key` from `Esc to <word>` / `Tab to <word>` (or locale equivalents).
- **Codex panel** requires its `Queued follow-up inputs` heading near the question. The numbered option key submits immediately; `Ctrl+]` skips and `Alt+Down` returns to the main prompt. The heading guard keeps a quoted menu from becoming an interactive choice.
- **Option regex** `^\s*(?:[❯›>]\s*)?(\d+)[.)]\s+(.+?)\s*$` — numbered items, optional cursor marker (`❯`, `›`, `>`).
- **Title heuristics** walk up past blank rows and require either a `?` suffix or a verb prefix (`do you want`, `proceed`, `continue`, `should i`, `confirm`, `apply`, `allow`) to avoid matching Markdown numbered lists.
- **Minimum two options** required to reduce false positives.

**Destructive flag:** labels matching `"no"`, `"cancel"`, `"reject"`, `"abort"`, `"deny"`, or the prefixes `"don't"` / `"do not"` are flagged so the PWA overlay and plugins can style them as destructive.

**Flow:** the payload is stored on `SessionState.choice_prompt` and dispatched via `pluginRegistry.dispatchStructuredEvent("choice-prompt", …)`. Animated status-line updates preserve the prompt and its `awaiting_input` lifecycle; resolution, disappearance, replacement, and PTY exit clear it. A disappearing or resolved dialog emits `choice-cleared` so frontend and plugin consumers do not retain stale state. Single-key replies should go through `sendPtyKey()` in `src/utils/sendCommand.ts`, never raw `text + \r`.

The mobile session header opens a queued Codex question with `Alt+Up`. Before it opens, Codex exposes a waiting signal but not the option labels; after the panel renders, `ChoicePrompt` supplies the title and options to the session list and detail overlay. A free-form answer goes to the opened panel's PTY composer, not the atomic idle-agent submission route.

Observed Codex prompt forms and evidence:

| Form | Live evidence | TUIC path |
|------|---------------|-----------|
| `request_user_input` choices and `Other` | `codex-request-user-input-20260929.tcap`; `captured_codex_request_user_input_reaches_choice_prompt` replays its rendered screen | A queued waiting signal appears first. `Alt+Up` opens the panel; `parse_choice_prompt` recognizes its `Queued follow-up inputs` heading, title, numbered options, and `ctrl+]` footer. |
| Command approval | `codex-0.157.1-approval-cancel.tcap`; `codex_canceled_approval_capture_clears_the_waiting_badge` replays an `Action Required` approval and Esc cancellation | The approval title sets the waiting question; the current screen and cancellation logic in `pty.rs` clear it when Codex returns to its composer. This capture does not establish that the approval uses the queued follow-up panel. |

A distinct plan-question screen has not been captured. If Codex displays one, verify its rendered rows and keys before treating it as either form above.

### SlashMenu

Slash command menu detected from VT100 screen rows:

```rust
ParsedEvent::SlashMenu {
    items: Vec<SlashMenuItem>,  // { command, highlighted }
}
```

Detected by `parse_slash_menu()` when `slash_mode` is active — scans the bottom screen rows for 2+ consecutive `/command` patterns. The `❯` prefix marks the highlighted item.

## VT100-Aware Parsing

### `parse_clean_lines(rows: &[ChangedRow]) -> Vec<ParsedEvent>`

Primary entry point for VT100-aware parsing. Accepts `ChangedRow` vectors from `VtLogBuffer.process()` — each row contains clean text extracted from the VT100 screen emulator. This replaces the legacy ANSI-stripping pipeline for mobile/MCP consumers.

`ChangedRow` production remains active on both the primary and alternate screens. This is intentional: fullscreen agents still emit lifecycle, intent, suggestion, and question surfaces that the parser must observe. It is independent from `VtLogBuffer`'s durable log, which reads only primary-screen scrollback. Enabling alternate-screen history therefore makes the UI scrollable without feeding repeated fullscreen snapshots into persistent logs.

### `parse_slash_menu(screen_rows: &[String]) -> Option<ParsedEvent>`

Scans screen bottom rows (from VtLogBuffer) for slash command menus. Only called when `slash_mode` is active (user typed `/`). Returns `SlashMenu` event with all detected commands.

## Pattern Detection

The parser uses regex patterns to detect:
- Rate limit messages from Claude, Aider, OpenCode, Gemini, Codex
- Questions and interactive prompts (hardcoded, Y/N, inquirer, Ink menus, generic `?` lines)
- API errors from agents and API providers (5xx, auth failures)
- GitHub/GitLab PR URLs in `gh pr create` output
- OSC 9;4 terminal progress sequences
- Agent status lines with timing/token info (see below)

Patterns are compiled once at `OutputParser::new()` and reused across calls.

### False-Positive Guards

Two guard functions prevent false-positive detection when agents read or display source code, diffs, or documentation containing error-like or question-like patterns:

- **`line_is_source_code(line)`** — Returns `true` for lines that look like source code rather than real errors. Detects: Rust raw string literals (`r"..."`, `r#"..."#`), line comments (`//`, `#`), function/const/let declarations, indented code with string delimiters (4+ leading spaces), markdown fences (`` ``` ``), bullet points (`- `, `* `), and markdown tables (`| ... |`).
- **`line_is_diff_or_code_context(raw_line, trimmed)`** — Returns `true` for lines that look like diff output or code listings. Detects: unified diff lines (`+`, `-` prefixes), line-number prefixed code (`462 -...`), Claude Code diff summary blocks (`⏺⎿`), and diff summary lines (`Added16lines`).

Both guards are applied to rate limit, API error, and question pattern matches before emitting events.

### ANSI Pre-Processing

The `strip_ansi()` function pre-processes CUF (Cursor Forward, `\x1b[nC`) escape sequences by replacing them with the equivalent number of spaces before stripping all ANSI escapes. Without this, `strip-ansi-escapes` silently drops cursor movement sequences and would concatenate surrounding text (e.g., `"hello\x1b[3Cworld"` would become `"helloworld"` instead of `"hello   world"`).

### Status Line Detection by Agent

| Agent | Pattern | Example |
|-------|---------|---------|
| Claude Code | `·`/`✢`/`✳`/`✶`/`✻`/`✽`/`*` + ellipsis | `✢Reading files… (12s)` or `· Considering…` |
| Aider | Knight Rider scanner `░█` / `█░` + task text | `░█        Waiting for claude-3-5-sonnet` |
| Aider | Token report `Tokens:` prefix | `Tokens: 5.2k sent, 1.3k received.` |
| Codex CLI | Bullet `•`/`◦` + task + parenthesized time | `• Working (4m 55s • esc to interrupt)` |
| Copilot CLI | `∴`/`●`/`○` + task + dots/ellipsis | `∴ Thinking…` or `● Read file...` |
| Gemini CLI | Braille spinner `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` + phrase | `⠋ Analyzing your codebase` |
| Amazon Q | Braille spinner + task + ASCII dots | `⠹ Thinking...` |
| Cline | Braille spinner + mode + optional timer | `⠙ Planning (45s · esc to interrupt)` |
| Generic | `[Running]` prefix | `[Running] npm test` |

### Hook-Instrumented Session Suppression

When native hook instrumentation is configured, heuristic `Question` events are
suppressed only after the session actually emits an OSC 7770 `state=` marker.
The runtime handshake avoids trusting a stale flag when hook installation or an
agent upgrade has broken delivery. Hook `busy` is authoritative over silence;
hook `idle`, a confirmed interrupted screen, or process exit ends it. For known
agents, explicit IDLE waits for a process snapshot newer than the marker before
publishing parent idle/completed lifecycle mail, so background descendants remain
working even while the shell is ready. Other parsed events continue unchanged.
See `suppress_heuristic_question()` and the
explicit-state fields in `SilenceState` (`src-tauri/src/pty.rs`).

Claude Stop hooks can block after Claude has already emitted a Stop/suggest
marker. If the current screen still contains a semantic active phase (spinner
prefix, active verb ending in an ellipsis, and parenthesized progress), the PTY
lifecycle reopens the same turn and discards the premature suggestions. A plain
empty composer or completed-duration summary does not provide this evidence.

Slash-menu parsing is gated by the input FSM's slash mode and intentionally
does not emit a per-chunk debug record. Sustained output with a stale slash flag
previously produced thousands of identical application-log writes in seconds.

## Shared ANSI stripping

`tuic-ansi` uses the workspace's patched VTE 0.15 parser to strip escape
sequences from plain log text. GitHub CI log sanitization and the test-only
`OutputParser::parse` adapter use it. It preserves printable UTF-8 and
linefeeds, and discards other executed controls, matching the previous
`strip-ansi-escapes` policy. The workspace no longer compiles VTE 0.14.

### Stored terminal marker coordinates

A hook-generated `UserInput.line` is an all-time terminal row captured by the OSC 7770 handler. Keystroke-reconstructed events keep `line = -1`. Consumers discard evicted hook rows instead of rebasing stored rows with a newer origin.

AI Chat also preserves reply text that immediately follows a parenthesized intent title on the same line. Untitled intent lines keep their existing status behavior; code examples are left intact.

### Recorded replay oracle

Run `scripts/replay-oracle.sh verify` to compare every `.tcap` under `src-tauri/`
(including terminal-crate captures) with `src-tauri/src/fixtures/replay_oracle/`.
The replay uses the production chunk processor, UTF-8 and escape buffers,
Kitty stripping, recorded initial geometry, and the production session-state
accumulator. It records ordered parsed payloads and state/awaiting/question
snapshots after each output record, plus final ring length. Input records and
capture elapsed times are not executed: this is a chunk-path parity oracle,
not an end-to-end agent or silence-timer simulation. Legacy captures without
geometry use the existing replay default of 41 rows and 128 columns.

`mail.jsonl` records a scripted scenario using the production send/inbox handler
and wake gate: paginated reads, wake coalescing and rearming after consumption.
Its callback records transport acceptance; it does not claim native PTY delivery.
Random message IDs and wall-clock cursors are projected to message positions.
Normal tests are read-only and report the first differing event, including
missing or extra events. To accept an intentional change, explicitly run
`scripts/replay-oracle.sh regenerate`, review the JSONL diff, and commit it with
the change. Add/stage a capture before regeneration so Git includes it in the
corpus; an unknown agent filename requires an explicit replay mapping.
