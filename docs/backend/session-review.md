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
byte-level pre-filter (`line_is_interesting`, rejecting any line without
`"toolUseResult"` or `"file-history-delta"`) runs before any JSON parse, so
the giant `attachment`/`assistant`/`system` records that dominate a
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
in-process via `gix::diff::blob` (imara-diff) — no new diff engine. Note:
`gix-diff`'s `blob` feature pulls in `gix-imara-diff` transitively but does
**not** forward that crate's own `unified_diff` feature, so
`Diff::unified_diff`/`BasicLineDiffPrinter`/`UnifiedDiffConfig` are
unavailable through `gix::diff::blob` unless `gix-imara-diff` is *also*
declared as a direct dependency with that feature on (see the comment next
to it in `Cargo.toml`) — Cargo's feature unification then turns it on
crate-wide for the one shared instance of the package.

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

Both revert paths call `git::bump_working_tree_epoch` so the sidebar/Changes
tab refresh, exactly as `git_discard_files` already does.

## Caching

An in-memory, `(len, mtime)`-validated full-review cache (last 4 sessions,
evicted oldest) — not an incremental byte-offset-resume cache. Given the
byte-level pre-filter above, a full re-scan is fast enough that incremental
resume wasn't worth the added complexity for a first version; the cache
still avoids re-parsing an unchanged transcript on every poll.

`list_review_sessions` never parses a full transcript for the picker:
`session_id`/`size_bytes`/timestamps come from the filename and
`metadata()`; `title`/`last_prompt` come from a tail-read of the last 64 KB
(both `custom-title`/`last-prompt` record types recur every turn, so the
last copy is always near EOF); full edit/file counts are opt-in
(`include_counts`) and scanned only for the first `limit` entries.

## Commands

See [`docs/api/tauri-commands.md`](../api/tauri-commands.md#session-diff-review-session_reviewrs)
and [`docs/api/http-api.md`](../api/http-api.md#session-diff-review) for the
four commands/routes and their wire types.
