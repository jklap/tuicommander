/**
 * The single source of truth for Smart Prompts' `{var}` context variables —
 * consolidates what used to be four independently hand-maintained copies:
 * `smartPromptsBuiltIn.ts`'s `VARIABLE_DESCRIPTIONS`, two near-identical
 * `CONTEXT_VARIABLES` arrays (`SmartPromptsTab.tsx`, `PromptDrawer.tsx`), and
 * `SmartPromptsTab.tsx`'s `REPO_CONTROLLED_VARIABLES` — which had drifted out
 * of sync (missing entries, a stray undocumented `branch_name`, inconsistent
 * descriptions).
 *
 * Resolution logic itself stays wherever it already lives (Rust's
 * `prompt.rs::resolve_single_var`/`resolve_vars` for `source: "rust"`;
 * `useSmartPrompts.ts`'s `resolveFrontendVars` for `source: "frontend"`; the
 * three host surfaces for `source: "host:*"`) — this module is only the
 * *catalog*: names, descriptions, grouping, and two cross-cutting flags.
 * See `src/__tests__/contextVariablesParity.test.ts` for the guard that keeps
 * this in sync with `prompt.rs::ALL_VARS` and `script_env.rs`.
 */

/** Who actually resolves this variable's value. */
export type VarSource =
	| "rust" // prompt.rs::resolve_single_var / resolve_vars — needs only a tree path
	| "frontend" // useSmartPrompts.ts::resolveFrontendVars — needs app stores (PR data, active terminal)
	| "host:file" // fileContextVariables, placement="file-context" hosts
	| "host:issue" // GitHubPanel's expanded-issue popover
	| "host:branch"; // usePluginContextActions.ts's branch right-click menu

export type VarGroup = "Git" | "Worktree" | "GitHub" | "Terminal" | "File" | "Issue";

export interface ContextVariableDef {
	name: string;
	description: string;
	group: VarGroup;
	source: VarSource;
	/** Value is influenced by repo/branch/PR contents a bad actor with commit
	 *  access could control — must be shell-quoted before reaching `sh -c`/
	 *  `cmd /C` (handled by `process_prompt_content_shell_safe`). Drives the
	 *  shell-injection warning banner in the Settings editor. */
	repoControlled?: boolean;
	/** Also exposed to Setup/Archive/Run scripts (and Smart Prompt shell/
	 *  headless children) as an environment variable named `TUIC_` + this
	 *  name uppercased — see `script_env.rs::ScriptContext::pairs`. Only a
	 *  subset of variables have a script-env counterpart; most don't. */
	script?: boolean;
}

export const CONTEXT_VARIABLES: readonly ContextVariableDef[] = [
	// --- Git (rust) ---
	{
		name: "branch",
		description: "Current git branch name",
		group: "Git",
		source: "rust",
		repoControlled: true,
		script: true,
	},
	{
		name: "base_branch",
		description: "Main/master/develop branch detected in the repo",
		group: "Git",
		source: "rust",
		repoControlled: true,
		script: true,
	},
	{ name: "diff", description: "Unstaged changes (git diff)", group: "Git", source: "rust", repoControlled: true },
	{
		name: "staged_diff",
		description: "Staged changes (git diff --staged)",
		group: "Git",
		source: "rust",
		repoControlled: true,
	},
	{
		name: "changed_files",
		description: "List of modified files (git status --short)",
		group: "Git",
		source: "rust",
		repoControlled: true,
	},
	{
		name: "dirty_files_count",
		description: "Number of modified files",
		group: "Git",
		source: "rust",
	},
	{
		name: "commit_log",
		description: "Last 20 commits (one-line format)",
		group: "Git",
		source: "rust",
		repoControlled: true,
	},
	{
		name: "last_commit",
		description: "Latest commit hash and message",
		group: "Git",
		source: "rust",
		repoControlled: true,
	},
	{
		name: "conflict_files",
		description: "Files with unresolved merge conflicts",
		group: "Git",
		source: "rust",
		repoControlled: true,
	},
	{
		name: "stash_list",
		description: "Current git stash entries",
		group: "Git",
		source: "rust",
		repoControlled: true,
	},
	{
		name: "branch_status",
		description: "Ahead/behind counts vs. the upstream tracking branch",
		group: "Git",
		source: "rust",
	},
	{
		name: "remote_url",
		description: "Remote origin URL",
		group: "Git",
		source: "rust",
		repoControlled: true,
	},
	{ name: "current_user", description: "Git user.name", group: "Git", source: "rust", repoControlled: true },
	{
		name: "repo_owner",
		description: "GitHub owner parsed from the remote URL",
		group: "Git",
		source: "rust",
		repoControlled: true,
	},
	{
		name: "repo_slug",
		description: "Repository name parsed from the remote URL",
		group: "Git",
		source: "rust",
		repoControlled: true,
	},
	{
		name: "repo_name",
		description: "Main checkout's directory name (not a worktree's)",
		group: "Git",
		source: "rust",
		repoControlled: true,
		script: true,
	},
	{
		name: "repo_path",
		description:
			"The tree the variables were resolved against — the worktree root when a worktree terminal is focused, else the repo root",
		group: "Git",
		source: "rust",
	},

	// --- Worktree (rust) ---
	{
		name: "worktree_path",
		description: "The worktree root (same value as repo_path when not in a linked worktree)",
		group: "Worktree",
		source: "rust",
		script: true,
	},
	{
		name: "main_repo_path",
		description: "The main checkout — always the repo root itself, never a worktree",
		group: "Worktree",
		source: "rust",
		script: true,
	},
	{
		name: "worktree_name",
		description: "Basename of worktree_path",
		group: "Worktree",
		source: "rust",
		script: true,
	},
	{
		name: "is_worktree",
		description: "'true' if the current tree is a linked worktree, else 'false'",
		group: "Worktree",
		source: "rust",
		script: true,
	},

	// --- GitHub (frontend) ---
	{
		name: "pr_number",
		description: "Pull request number for the current branch",
		group: "GitHub",
		source: "frontend",
	},
	{ name: "pr_title", description: "Pull request title", group: "GitHub", source: "frontend", repoControlled: true },
	{
		name: "pr_url",
		description: "GitHub pull request URL",
		group: "GitHub",
		source: "frontend",
		repoControlled: true,
	},
	{
		name: "pr_state",
		description: "PR state: OPEN, MERGED, or CLOSED",
		group: "GitHub",
		source: "frontend",
		repoControlled: true,
	},
	{
		name: "pr_author",
		description: "PR author username",
		group: "GitHub",
		source: "frontend",
		repoControlled: true,
	},
	{
		name: "pr_labels",
		description: "PR labels (comma-separated)",
		group: "GitHub",
		source: "frontend",
		repoControlled: true,
	},
	{ name: "pr_additions", description: "Lines added in the PR", group: "GitHub", source: "frontend" },
	{ name: "pr_deletions", description: "Lines deleted in the PR", group: "GitHub", source: "frontend" },
	{
		name: "merge_status",
		description: "Merge status: MERGEABLE, CONFLICTING, or BEHIND",
		group: "GitHub",
		source: "frontend",
		repoControlled: true,
	},
	{
		name: "review_decision",
		description: "Review status: APPROVED, CHANGES_REQUESTED, or REVIEW_REQUIRED",
		group: "GitHub",
		source: "frontend",
		repoControlled: true,
	},
	{
		name: "pr_checks",
		description: "CI check summary (e.g. '3 passed, 1 failed')",
		group: "GitHub",
		source: "frontend",
		repoControlled: true,
	},

	// --- Terminal (frontend) ---
	{
		name: "agent_type",
		description: "Active agent type (claude, aider, codex, etc.)",
		group: "Terminal",
		source: "frontend",
		repoControlled: true,
	},
	{
		name: "cwd",
		description: "Active terminal's working directory",
		group: "Terminal",
		source: "frontend",
		repoControlled: true,
	},
	{
		name: "session_id",
		description: "Stable per-tab session UUID (same value the terminal's $TUIC_SESSION carries)",
		group: "Terminal",
		source: "frontend",
	},

	// --- Issue (host:issue — GitHub sidebar's expanded-issue popover) ---
	{ name: "issue_number", description: "GitHub issue number", group: "Issue", source: "host:issue" },
	{ name: "issue_title", description: "GitHub issue title", group: "Issue", source: "host:issue" },
	{ name: "issue_author", description: "GitHub issue author", group: "Issue", source: "host:issue" },
	{ name: "issue_labels", description: "Comma-separated issue labels", group: "Issue", source: "host:issue" },
	{ name: "issue_state", description: "Issue state: OPEN or CLOSED", group: "Issue", source: "host:issue" },
	{ name: "issue_url", description: "GitHub issue URL", group: "Issue", source: "host:issue" },
	{
		name: "issue_assignees",
		description: "Comma-separated assignee usernames",
		group: "Issue",
		source: "host:issue",
	},
	{ name: "issue_milestone", description: "Issue milestone name", group: "Issue", source: "host:issue" },
	{
		name: "issue_comments_count",
		description: "Number of comments on the issue",
		group: "Issue",
		source: "host:issue",
	},

	// --- File (host:file — placement="file-context" hosts: file browser, diff editor, markdown/editor tabs) ---
	{
		name: "file_path",
		description: "Absolute path of the selected file or folder",
		group: "File",
		source: "host:file",
	},
	{
		name: "file_rel_path",
		description: "Path relative to the repository root",
		group: "File",
		source: "host:file",
	},
	{ name: "file_name", description: "Basename of the file (e.g. foo.ts)", group: "File", source: "host:file" },
	{
		name: "file_ext",
		description: "File extension including the dot (e.g. .ts)",
		group: "File",
		source: "host:file",
	},
	{ name: "file_dir", description: "Parent directory absolute path", group: "File", source: "host:file" },
	{
		name: "file_is_dir",
		description: "'true' if the target is a folder, else 'false'",
		group: "File",
		source: "host:file",
	},

	// --- Branch (host:branch — the sidebar's branch right-click menu) ---
	{
		name: "branch_name",
		description: "The branch you right-clicked — distinct from {branch}, the currently checked-out one",
		group: "Git",
		source: "host:branch",
		repoControlled: true,
	},
];

/** name -> description, for VariableInputDialog and the dropdown tooltip.
 *  Replaces `smartPromptsBuiltIn.ts`'s former standalone export. */
export const VARIABLE_DESCRIPTIONS: Record<string, string> = Object.fromEntries(
	CONTEXT_VARIABLES.map((v) => [v.name, v.description]),
);

/** Variables whose runtime value is controlled by repository/PR/branch
 * contents (branch names, commit messages, PR titles, remote URLs). When
 * substituted into a shell-execution template, quoting is mandatory —
 * otherwise a crafted branch like `main'; rm -rf ~ ;#` escapes `sh -c`. The
 * Rust backend quotes these for us via `process_prompt_content_shell_safe`;
 * this set drives a UI warning so the author knows the risk surface. */
export const REPO_CONTROLLED_VARIABLES: ReadonlySet<string> = new Set(
	CONTEXT_VARIABLES.filter((v) => v.repoControlled).map((v) => v.name),
);

/** Return the subset of repo-controlled variables referenced in content.
 *  NOTE: this regex (`{([^{}]+)}`) is deliberately its own thing, distinct
 *  from both `prompt.rs::extract_variables` (any char until the next `}`)
 *  and `templateInterpolation.ts` (`\w+` only) — see that file's own
 *  comment for why the three aren't unified in this change. */
export function repoControlledVarsInContent(content: string): string[] {
	const found = new Set<string>();
	const re = /\{([^{}]+)\}/g;
	let match: RegExpExecArray | null;
	while ((match = re.exec(content)) !== null) {
		if (REPO_CONTROLLED_VARIABLES.has(match[1])) {
			found.add(match[1]);
		}
	}
	return [...found].sort();
}

/** Names Rust's `resolve_vars`/`ALL_VARS` can resolve — used by the drift
 *  guard and available for any future "is this variable git-backed" check. */
export const RUST_RESOLVED_VARIABLES: readonly string[] = CONTEXT_VARIABLES.filter((v) => v.source === "rust").map(
	(v) => v.name,
);

/** Variables also exposed to Setup/Archive/Run scripts (and Smart Prompt
 *  shell/headless children) as `TUIC_<NAME_UPPERCASED>` — see
 *  `script_env.rs::ScriptContext::pairs`. Every entry here must have
 *  `source: "rust"`: a Setup/Archive script runs with no frontend at all, so
 *  a frontend-sourced variable could never actually be resolved for it. */
export const SCRIPT_ENV_VARIABLES: readonly ContextVariableDef[] = CONTEXT_VARIABLES.filter((v) => v.script);

export interface PickerOptions {
	/** Which host-supplied variable groups this picker's surface actually
	 *  populates. Omit entirely for a picker with no host context (e.g. a
	 *  generic run-config editor with no file/issue/branch scope). */
	hosts?: ReadonlyArray<"file" | "issue" | "branch">;
}

/** The variables a given picker surface should render — `rust`/`frontend`
 *  entries always show (they're available everywhere a prompt can run);
 *  `host:*` entries show only when the caller declares that host present.
 *  Replaces two independently hand-maintained `CONTEXT_VARIABLES` copies
 *  (`SmartPromptsTab.tsx` declared `hosts: ["file"]`; `PromptDrawer.tsx`
 *  declared none) with one rule. */
export function variablesForPicker(opts: PickerOptions = {}): readonly ContextVariableDef[] {
	const hosts = new Set(opts.hosts ?? []);
	return CONTEXT_VARIABLES.filter((v) => {
		switch (v.source) {
			case "host:file":
				return hosts.has("file");
			case "host:issue":
				return hosts.has("issue");
			case "host:branch":
				return hosts.has("branch");
			default:
				return true;
		}
	});
}
