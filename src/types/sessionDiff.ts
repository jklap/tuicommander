/**
 * TS mirror of `src-tauri/src/session_review.rs`'s wire types, field-for-field.
 * The Rust side serializes plain snake_case (no `rename_all`), matching the
 * rest of this codebase's git.rs-derived types (e.g. `RecentCommit.short_hash`),
 * so these fields are intentionally NOT camelCased.
 */

/** One Claude Code session available for review in a given repo. */
export interface SessionSummary {
	session_id: string;
	transcript_path: string;
	cwd: string | null;
	git_branch: string | null;
	/** ISO-8601. */
	started_at: string | null;
	/** ISO-8601. */
	ended_at: string | null;
	title: string | null;
	last_prompt: string | null;
	size_bytes: number;
	/** `null` unless the caller passed `include_counts: true`. */
	edit_count: number | null;
	file_count: number | null;
	has_subagents: boolean;
}

export type StepKind = "create" | "overwrite" | "edit";

/** One file mutation, in transcript order. */
export interface EditStep {
	/** Display-order position — NOT stable across a live/growing transcript.
	 *  `tool_use_id` is the stable handle; use it for revert/selection keys. */
	step_index: number;
	tool_use_id: string;
	/** ISO-8601. */
	timestamp: string | null;
	kind: StepKind;
	abs_path: string;
	/** `null` when the file lives outside `repo_path`. */
	rel_path: string | null;
	in_repo: boolean;
	/** A `git apply`-able unified diff for this step alone. Empty for a no-op step. */
	patch: string;
	additions: number;
	deletions: number;
	is_sidechain: boolean;
	agent_name: string | null;
	user_modified: boolean;
	replace_all: boolean;
}

/** Where a file's session-start content came from — a confidence signal for the UI. */
export type BaseSource = "backup" | "created_in_session" | "tool_result" | "reconstructed" | "unknown";

/** Net effect of the whole session on one file. */
export type NetChange = "added" | "modified" | "unchanged" | "deleted";

export interface FileReview {
	abs_path: string;
	rel_path: string | null;
	in_repo: boolean;
	/** `rel_path` when in-repo, else `abs_path` — the label to show. */
	display_path: string;
	net_change: NetChange;
	base_source: BaseSource;
	/** Empty when `net_change === "unchanged"` or `base_source === "unknown"`. */
	cumulative_patch: string;
	additions: number;
	deletions: number;
	/** Indices into `SessionReview.steps`, in chronological order. */
	step_indices: number[];
	/** True when the file changed outside this session since it was last touched. */
	drifted_from_disk: boolean;
	/** True when a `@v1` backup exists on disk right now (enables byte-exact revert). */
	backup_available: boolean;
	is_binary: boolean;
}

export interface SessionReview {
	session_id: string;
	transcript_path: string;
	repo_path: string;
	started_at: string | null;
	ended_at: string | null;
	title: string | null;
	/** Chronological across the main thread + any merged subagent transcripts. */
	steps: EditStep[];
	/** Ordered by first-touch time. */
	files: FileReview[];
	/** Non-fatal parse problems — render as a dismissible banner, never drop. */
	warnings: string[];
	included_subagents: boolean;
}

export type RevertMethod = "git_apply_reverse" | "string_substitution" | "restore_backup" | "write_base" | "delete_file";

export interface RevertResult {
	applied: boolean;
	method: RevertMethod;
	abs_path: string;
	message: string | null;
}
