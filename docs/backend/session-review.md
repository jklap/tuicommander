# Session Diff Review (`session_review.rs`)

Reconstructs a step-by-step, per-file edit review from a Claude Code session
transcript — the backend for the "Session Diff Review" tab
([`docs/FEATURES.md`](../FEATURES.md) §7.5). This is a read-only
reconstruction from files Claude Code already writes; no new agent
instrumentation is involved.

## Data source

`~/.claude/projects/<slug>/<uuid>.jsonl` (or `<CLAUDE_CONFIG_DIR>/projects/...`
when that env var is set — resolved via `agent_session::claude_project_dir`,
**not** `claude_usage.rs`'s own `claude_projects_dir`, which is a duplicate
that ignores the override). One JSON record per line. The `user`-type
record's top-level `toolUseResult` field (a sibling of `message`, not nested
in it) carries everything needed for an Edit/Write step: `filePath`,
`oldString`/`newString` (Edit) or `content` (Write), `structuredPatch`,
`replaceAll`, `userModified`.

Two format traps this module accounts for, found by inspecting real
transcripts rather than assuming a shape:

- **`structuredPatch` is empty (`[]`) for a file *create*.** The new content
  lives only in `content` and must be turned into a synthetic all-added
  hunk — a naive reader that trusts `structuredPatch` silently drops every
  newly-created file from the review.
- **`toolUseResult` is sometimes a bare string**, not an object (Bash tool
  results). Must deserialize as `serde_json::Value` and type-check before
  treating it as an edit.

Subagent edits live in **sibling files**
(`<slug>/<uuid>/subagents/agent-*.jsonl`), not inline with an
`isSidechain:true` flag in the main transcript. They're merged in by
default (`include_subagents`, default true) — a real edit shouldn't
silently vanish from a review just because a subagent made it — flagged
`is_sidechain` + `agent_name` (the filename's `agent-<id>` prefix stripped
for display).

`~/.claude/file-history/<uuid>/<name>@v1` holds a byte-exact pre-edit
snapshot the *first* time a file is touched each session. This directory is
pruned by Claude Code itself, so it's read best-effort and never assumed to
exist.

Sessions are large in practice (18–24 MB observed on real transcripts) — a
byte-level pre-filter (`line_is_interesting`) runs before any JSON parse,
admitting only lines that contain `"toolUseResult"`, `"file-history-delta"`,
`"custom-title"`, `"last-prompt"`, or `"type":"user"` (a real human prompt —
needed for turn start times/previews; also matches most
`toolUseResult`-bearing user records, which the first check already lets
through). Earlier, `custom-title`/`last-prompt` records were excluded from
this filter, which meant `SessionReview.title`/`last_prompt` were
effectively always `None` — the title-matching logic downstream never got a
chance to see those records. So the giant `attachment`/`assistant`/`system`
records that dominate a
transcript's bytes are never deserialized.

## Base resolution

Reconstructing a file's session-start content, tried in order (each yields a
[`BaseSource`] the frontend shows as a confidence badge):

1. **`CreatedInSession`** — the *first* recorded edit for the path is a
   [`StepKind::Create`] (`toolUseResult.type == "create"`). This signal comes
   from the transcript itself, not from `file-history` — it's both necessary
   and sufficient, so base resolution doesn't depend on a `file-history`
   backup existing at all for this case.
2. **`ToolResult`** — the first edit's `toolUseResult.originalFile` is
   present (observed non-null on ~25% of real Edit results).
3. **`Reconstructed`** — reverse-fold from the current on-disk content: undo
   each recorded edit in reverse chronological order. Exact unless something
   *else* touched the file since. [`StepKind::Overwrite`] can never be
   reversed this way (there's no way to know its prior content without an
   anchor), so a file whose history includes an overwrite with nothing
   upstream of it correctly bottoms out at tier 4:
4. **`Unknown`** — no cumulative diff is offered for the file, but its
   step-by-step timeline is still complete; a file-level resolution failure
   must never drop step data.

`~/.claude/file-history`'s `@v1` backup is used for two things *independent*
of the above: the byte-exact `restore_backup` revert method, and
`FileReview.backup_available`. It is deliberately **not** consulted for
cumulative-diff base resolution — the first-edit-kind signal above is more
robust and needs no extra I/O.

A file edited back to its original content within the session naturally
yields `NetChange::Unchanged` with an empty cumulative patch — computed from
content equality, not summed per-step line counts, so a net-zero edit never
misreports non-zero `+`/`-` counts. `drifted_from_disk` compares the folded
final content against what's actually on disk right now; a mismatch means
something outside this session touched the file since.

Diffs (`patch`, `cumulative_patch`) are unified-diff strings rendered
in-process by `unified_patch`, which always goes through the shared
whitespace/case engine in the Tauri-free `tuic-git` crate
(`crates/tuic-git/src/diff_options.rs`, re-exported to the app crate as
`crate::diff_options`; built on `gix::diff::blob`/imara-diff) — the same engine
Branch Diff uses (see [`docs/backend/git.md`](./git.md#diff-options-whitespacecase-insensitive-comparison)).
Note: `gix-diff`'s `blob` feature pulls in `gix-imara-diff` transitively but does
**not** forward that crate's own `unified_diff` feature, so
`Diff::unified_diff`/`BasicLineDiffPrinter`/`UnifiedDiffConfig` are
unavailable through `gix::diff::blob` unless `gix-imara-diff` is *also*
declared as a direct dependency with that feature on — `tuic-git`'s
`Cargo.toml` does (see the comment next to it); Cargo's feature unification
then turns it on for the one shared instance of the package. The app crate
itself no longer depends on `gix` directly.

`get_session_review` optionally takes a `DiffOptions` (ignore leading/trailing
whitespace, whitespace amount, or case). With all four off (the default) the
engine produces the ordinary byte-exact diff; with any on, lines that differ only
in what the options ignore compare equal. It's
part of the review cache key (see Caching below), so two different option
sets for the same session never collide, and `NetChange` itself respects the
active options (a whitespace-only change reports `Unchanged`, not `Modified`
with an empty diff).

### Per-step line numbers

Each `EditStep.patch` normally comes from folding the file forward/backward
through the transcript, which yields real file-relative line numbers. When
that fold fails (`base_source == Unknown`, or a specific step's replay can't
locate its `old_string`), the step's patch used to be built by diffing only
`old_string`/`new_string` from scratch — a from-scratch diff always starts
its hunk header at `@@ -1`, which is wrong for anywhere but the top of the
file, and "open at this change" would jump to the wrong line. Fixed: when the
transcript record carries a non-empty `toolUseResult.structuredPatch` (Claude
Code's own hunk list, with real `oldStart`/`newStart` line numbers),
`patch_from_structured_hunks` renders those hunks directly instead of
re-deriving them. A `Create`/`Overwrite` step's `structuredPatch` is empty for
a create — it still gets an all-added hunk from `content`, unaffected by this
fallback path.

### Turns

A **turn** is one user prompt's worth of edits (`EditStep.turn_index`,
`SessionReview.turns: Vec<TurnSummary>`). `scan_transcript` collects every
real user-prompt record's `promptId` → `(timestamp, first line of the prompt
text)`, and every tool-result record's own `tool_use_id` → the `promptId` that
triggered it (needed for subagent inheritance below). `assign_turns` then
walks the chronologically-ordered edit list once: a main-session step's turn
comes from its own record's `promptId`; a subagent step has no `promptId` of
its own, so it inherits the turn of the parent transcript's `Agent`/`Task`
tool_use call that spawned it, matched via the subagent's `meta.json`
`toolUseId` field against the parent's `tool_use_id → promptId` map. A step
whose `promptId` can't be resolved either way (an older transcript format, or
an unmatched subagent) attaches to whichever turn is currently open rather
than going unassigned. `TurnSummary.prompt_preview` is `None` for a turn
that only ever got a fallback timestamp (no real prompt record was found).

### Agent identity

`EditStep.agent_id` is the raw hex subagent id (from the transcript filename,
`subagent_name_from_path`); `EditStep.agent_display_name` is a friendlier
name for it: `read_subagent_meta` reads the subagent's sibling
`agent-<id>.meta.json` (when present) and prefers, in order, its `name` →
`description` → `agentType` field, falling back to the raw hex id when none
of those exist or the file itself is missing/malformed (never a hard
failure). A main-session step's `agent_display_name` is the session's own
title, falling back to the literal string `"main"`.

### Change fingerprint

`FileReview.revision` is a short hash of `cumulative_patch` plus the last
touching step's `tool_use_id` — lets a caller detect "did this file's review
content actually change" between two fetches (used by the frontend's live
in-place-update/flash feature) without diffing the patch text itself. Files
in `SessionReview.files` are already ordered by first touch, so a newly
appended file naturally lands at the end of the list.

## Revert — two mechanisms, deliberately different

- **Per-step** (`revert_session_step`, keyed by the transcript's own
  `tool_use_id` — never a client-supplied patch): in-repo files go through
  `git apply --reverse` (`apply_reverse_patch_impl`, shared with
  `git_apply_reverse_patch`'s hunk-revert feature, extended with a
  `check_only` dry-run mode). This should legitimately *fail* when a later
  step touched the same region — `git apply`'s own hunk-context matching
  gives that for free. Out-of-repo files (which `git apply` refuses) fall
  back to reversing the recorded substitution directly against the file's
  *current* content (`method: "string_substitution"`) — the same
  region-scoped tolerance as the in-repo path, just implemented by hand.
  `revert_step_via_substitution`'s Edit arm delegates to `apply_reverse`
  rather than re-deriving its own find/replace-backward logic, so the two
  paths can't drift apart.
- **Per-file, revert-to-session-start** (`revert_file_to_session_start`): a
  **direct content restore**, not a cumulative reverse-patch apply — byte-copy
  the `@v1` backup when available (`restore_backup`), write the reconstructed
  base text otherwise (`write_base`), or delete the file if the session
  created it (`delete_file`). Chosen over a reverse-patch apply because: a
  backup is more exact than a patch that can fail on drift; a write can't
  fail halfway the way a multi-hunk patch can; and a session-created file's
  correct revert is deletion, which a synthetic patch only awkwardly
  expresses. Refuses when `drifted_from_disk` unless `force: true`; refuses
  outright when `base_source == Unknown` (never write a guess).

**Where a revert may write (both paths).** The transcript names every path,
so a path being in the session's touched-file set is necessary but not
sufficient: `resolve_revert_target` also requires an absolute path with no
`.`/`..` component whose canonicalized parent lies inside an allowed root —
the git repo/worktree containing `repo_path` (plus a linked worktree's main
checkout), every registered repository, or `<CLAUDE_CONFIG_DIR or
~/.claude>/plans/` — and refuses a symlink at the leaf (never written or
deleted through). Anything else is an error, and nothing is touched. The
write/delete then goes to the resolved `canonical_parent/name`, not the raw
transcript string. A session's out-of-repo edit to a file outside those roots
is still reviewable, just not revertable from here.

**Bounded reads of transcript-named files.** A `backupFileName` must be a
bare file name (no separator, `.`/`..`, `:` or NUL) inside
`file-history/<session_id>/`; anything else counts as "no backup". The backup
read is capped at `MAX_BACKUP_BYTES` (16 MiB) and a subagent's
`agent-<id>.meta.json` at `MAX_SUBAGENT_META_BYTES` (64 KiB); past either the
file is ignored (the base falls back to the next tier / the hex id).

Both revert paths call `git::bump_working_tree_epoch` so the sidebar/Changes
tab refresh, exactly as `git_discard_files` already does, and evict any
cached review for that transcript (see Caching below) so a re-fetch right
after a revert reflects the new working-tree state rather than a stale one.

A pure-deletion edit (`new_string == ""`) cannot be located unambiguously in
content by a plain substring search — `str::find("")` matches at offset 0
unconditionally, which would otherwise reinsert the deleted text at the
wrong location instead of failing. Both substitution-based paths
(`apply_reverse`'s Edit arm, `revert_step_via_substitution`) treat an empty
search string as unlocatable and report "not found" rather than guessing.

## Caching

An in-memory, full-review cache (last `MAX_CACHED_REVIEWS` = 4 sessions) —
not an incremental byte-offset-resume cache. Given the byte-level pre-filter
above, a full re-scan is fast enough that incremental resume wasn't worth the
added complexity for a first version; the cache still avoids re-parsing an
unchanged transcript on every poll. The cache key is `(len, mtime,
include_subagents, subagent_fp, options)`:

- `include_subagents` — changes what the built review contains without
  changing the main transcript file at all.
- `subagent_fp` (`SubagentFingerprint`: count + max mtime + total byte
  length of every `<session>/subagents/*.jsonl`) — a revert or growth of a
  *subagent* transcript never touches the main transcript's own `(len,
  mtime)`; without this, a session with subagent edits could keep serving a
  stale review indefinitely after a subagent transcript grew.
- `options` (the `DiffOptions` above) — two different option sets for the
  same session must never collide.

Eviction is true LRU, not "evict whatever `HashMap::keys().next()` returns"
(the latter is arbitrary, not actually least-recently-used) — each entry
carries a `last_used` tick from a monotonic `next_use_tick()` counter,
touched on every cache hit, and the entry with the smallest tick is evicted
when a new one is inserted past capacity.

Both revert commands evict the cached entry for their transcript on success,
for the same reason the cache key above exists: a revert mutates the
*working tree*, which the built review also depends on
(`base_source`/`net_change`/`drifted_from_disk`), without changing the
transcript's `(len, mtime)`.

## Live watcher

`session_review_watcher.rs` (modeled on `dir_watcher.rs`) pushes a change
notification instead of relying on a client to poll. `watch_session_review`
watches, per subscription: the main transcript file non-recursively, the
project directory non-recursively (for a brand-new session `.jsonl`
appearing), and — if it already exists at watch-start time — the session's
own `<project_dir>/<session_id>/` subfolder recursively (where subagent
transcripts live). Ref-counted per `(project_dir, session_id)`: multiple
subscribers to the same session share one underlying `notify` watcher, and
the last `unwatch_session_review` tears it down. Bounded: at most
`MAX_SESSION_REVIEW_WATCHERS` (64) distinct watchers and
`MAX_SESSION_REVIEW_WATCH_REFS` (32) subscribers per watcher — past either, a
`watch` is refused and takes no ref (the frontend then never unwatches for it,
and an unwatch with no ref behind it is a no-op rather than stealing another
subscriber's). Debounced ~400ms (shorter
than `dir_watcher`'s 500ms — a live diff view benefits from feeling
responsive to an agent's own rapid tool-call bursts).

On a debounced fire it invalidates the review cache for that session and
`emit_dual`s `SessionReviewChanged{repo_path, session_id}` (a change to the
watched session) or `ReviewSessionsChanged{repo_path}` (a new session file
appeared). The first change observed for a session since it started being
watched also fires `AgentEditObserved{tuic_session_id, claude_session_id,
repo_path}` exactly once per session (tracked in
`AppState.announced_edit_sessions`, cleared when the last subscriber unwatches
or when the TUIC PTY session that owns that Claude session closes) — the signal an auto-open/notification
feature keys off (see the `session_diff_auto_open` setting in
[`docs/backend/config.md`](./config.md)).

**Known gap:** not dynamic about a subagent subfolder that doesn't exist yet
— if a session has no `subagents/` directory when `watch_session_review` is
called, this watcher does not notice one appearing later. A session that
gains its first subagent after the watch started needs a fresh
`watch_session_review` call to pick it up.

## Claude session ↔ TUIC session map

Every terminal running Claude Code already reports its Claude session id via
tuic-hook's `SessionStart` OSC 7770 `ccsession` metadata — previously emitted
by `pty.rs`'s output parser but never consumed. It now populates
`AppState.claude_session_map`/`tuic_to_claude_session` (kept in sync both
ways), cleaned up when the TUIC PTY session closes. A TUIC session that reports
a different `ccsession` retires its previous mapping, and both cleanups drop a
forward entry only while this TUIC session still owns it (a second tab that
resumed the same Claude session keeps its mapping). `SessionSummary` and
`SessionReview` both carry a `tuic_session_id: Option<String>` resolved from
this map — `None` when there's no live PTY session currently running that
Claude session. `session_review.rs` itself is a pure disk reader with no
`AppState` access, so `list_review_sessions_impl`/`get_session_review_impl`
never populate this field themselves; each transport's own thin wrapper
(`list_review_sessions`/`get_session_review` desktop commands,
`list_sessions_http`/`get_review_http` HTTP handlers) does it afterward — see
the doc comment on `SessionSummary::tuic_session_id` for the current
per-transport status.

A tmux-shim teammate pane is its own separate Claude process in its own PTY,
so it maps through this exact same mechanism with no special-casing — no
distinct "teammate session" concept exists at this layer.

`list_review_sessions` never parses a full transcript for the picker:
`session_id`/`size_bytes`/timestamps come from the filename and
`metadata()`; `title`/`last_prompt` come from a tail-read of the last 64 KB
(both `custom-title`/`last-prompt` record types recur every turn, so the
last copy is always near EOF); full edit/file counts are opt-in
(`include_counts`) and scanned only for the first `limit` entries. When
`include_counts` is set, those per-session full-transcript scans run
concurrently across `std::thread::scope` threads (sound here — the whole
command already runs inside `spawn_blocking`, off the async reactor) rather
than one after another, so picker load/refresh time doesn't scale linearly
with session count.

## Commands

See [`docs/api/tauri-commands.md`](../api/tauri-commands.md#session-diff-review-session_reviewrs)
and [`docs/api/http-api.md`](../api/http-api.md#session-diff-review) for the
four commands/routes and their wire types.
