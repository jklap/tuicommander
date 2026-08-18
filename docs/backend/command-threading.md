# Command threading

Where a `#[tauri::command]` runs, and how to decide for a new one.

## The rule

A command declared as a plain `fn` gets `ExecutionContext::Blocking` and runs
**inline in the IPC handler**. On macOS that is the main thread, so anything slow
in it freezes the WebView — not "makes the app feel slow", freezes it, cursor and
all, until the call returns.

A command declared `async fn` runs on the Tokio executor instead. That takes it
off the UI thread, but an `async fn` full of `std::fs` calls still parks a Tokio
worker for the whole operation. Blocking work needs `spawn_blocking` **as well**.

So there are three placements, and the choice is not a style preference:

| Shape | Runs on | Use for |
|---|---|---|
| `fn` | IPC thread (macOS main thread) | Work bounded by a few µs: a cached getter, an atomic load, a pure transform |
| `async fn` | Tokio executor | Work that awaits, or that is bounded and short |
| `async fn` + `spawn_blocking` | Blocking pool | Filesystem, subprocesses, locks that a busy thread may hold, anything unbounded |

`fs::spawn_blocking_fs` is the helper for the third row. It flattens the
`JoinError` into the closure's own `Result<T, String>`, so the caller sees the
error the operation actually produced rather than an opaque join failure.

## IPC and HTTP must agree

Every command has an HTTP twin. Both transports drive the *same* work, so both
must make the same threading decision — otherwise the same operation blocks a
Tokio worker over HTTP while running fine over IPC, or vice versa, and the
divergence is invisible until someone profiles the transport nobody tested.

The way to keep them in step is structural, not disciplinary: **the HTTP route
calls the command**, and the command owns the `spawn_blocking`. A route that
reaches past the command into a `*_impl` has opted out of the guarantee and needs
a comment saying why.

Before story 607-f483 the two had drifted in both directions: `search_files`
offloaded over HTTP but not over IPC, while `search_content_all` offloaded over
IPC but not over HTTP.

## Audit (2026-08-18)

391 `#[tauri::command]` declarations, 222 of them syntactically sync. Most are
correctly sync — cached getters, atomic reads, in-memory state. What follows is
the part of the inventory that matters, so this class of bug does not regrow
unnoticed.

### Moved off the UI thread in this story

`fs.rs`: `write_file`, `create_directory`, `delete_path`, `rename_path`,
`copy_path`, `copy_path_abs`, `move_path_abs`, `add_to_gitignore`,
`fs_read_file`, `list_directory`, `search_files`.
`lib.rs`: `read_file`, `read_editor_file`, `read_external_file`,
`read_editor_file_external`, `write_external_file`.
`ai_agent/tools.rs`: `read_file`, `write_file`, `edit_file`, `list_files`,
`search_files`, `search_code` — routed through `blocking_fs_tool()`, which is a
table rather than six match arms precisely so a test can assert the routing.

`list_directory` is the one that looks harmless and is not: it runs
`git status --porcelain` as a subprocess for the requested subdir.

### Known gaps, with reasons

| Command | Why it is still where it is |
|---|---|
| `fs_transfer_paths` (`fs.rs`) | Still sync, so its recursive directory copy runs on the main thread. It is the backend of a drag-drop, and the D&D surface needs Boss's approval before it is touched. Conversion is mechanical when that comes — see the `DEFERRED` note at the site. |
| `resolve_terminal_path` (`fs.rs`) | A single `canonicalize` + `is_dir`. Microseconds on a local disk; a stale network mount could stall it, which is a real but unobserved risk. |
| `warm_content_index` (`fs.rs`) | `ensure_index` is a map entry plus a spawn — the build itself already runs in the background. |
| Terminal grid reads (`pty.rs`) | ~14 commands (`terminal_search`, `terminal_search_buffer`, `terminal_styled_rows`, `terminal_get_lines`, `terminal_get_selection_text`, …) still take the VT mutex inline on the IPC thread. This is finding F95 and is the remaining half of story 607-f483. |
| `worktree.rs`, `tuic_cli.rs`, `tunnels/`, `dictation/`, `plugins.rs`, `agent.rs` | Sync commands running git subprocesses, keyring calls, hardware enumeration and recursive deletes. Each is a real main-thread stall; none was in this story's scope. They are listed here so the next sweep starts from a list instead of a grep. |

### Comments that asserted a cost the code did not have

Three were found and corrected. They are recorded because a wrong comment is
worse than no comment — each one had already talked a reader out of checking:

- `mcp_http/fs_routes.rs` — "a single `read_dir` + sort, which completes in
  microseconds". It runs `git status` as a subprocess.
- `mcp_http/fs_routes.rs` — the BM25 path is an "in-memory query (fast, stays on
  the executor)". Its grep phase opens up to 50 files.
- `fs.rs` — "BM25 phase: get top-ranked files (~1ms)". Nothing bounds it; the
  cost is proportional to the index.
