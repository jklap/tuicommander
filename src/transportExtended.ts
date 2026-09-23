/**
 * Extended COMMAND_TABLE entries — desktop/Settings-only Tauri<->HTTP command
 * mappings that mobile.html's browser-mode bundle does not need.
 *
 * mobile.html serves phone/PWA clients (always browser mode — no Tauri IPC
 * bridge), and its initial JS bundle has a hard 100KB-gzip budget
 * (scripts/report-frontend-bundles.mjs). transport/commandTable.ts's COMMAND_TABLE is one
 * flat object literal that a bundler can't tree-shake per entry, so every
 * command mapping ships to every bundle that imports mapCommandToHttp/rpc —
 * including mobile, which never calls most of them.
 *
 * This file holds the commands that NO module reachable from mobile's entry
 * references — traced over mobile's full import graph, its lazily loaded
 * screens included (chat/ACP, Files, Settings, Activity, session detail),
 * 2026-10-07, and re-derived on the wip→main replay (2026-10-22): a command
 * whose name appears as a string literal in ANY module mobile's entry reaches
 * (static or dynamic import) stays in the core table, even when the call site
 * looks desktop-only — mobile reaching a moved command would throw "No HTTP
 * mapping" at runtime, while an extra core entry only costs bytes.
 * Everything mobile can reach stays in transport/commandTable.ts's own table.
 * Mobile's voice control (useMobileVoice -> stores/dictation) and its ACP image
 * paste (utils/pastedImage) reach every dictation/speech command and
 * save_note_image, so those stay in the core table too.
 * Only the non-mobile entries import this file, for its registration side
 * effect below: the desktop app (src/appEntry.tsx) and the dev fixture pages
 * (src/dev/*). Mobile's entry (src/mobile/index.tsx) must never import it,
 * directly or transitively, or the split stops doing anything.
 *
 * If you add a command mobile might plausibly call — including from one of its
 * lazy screens or from a shared store/component it renders — put it in
 * transport/commandTable.ts's COMMAND_TABLE instead of here: a missing entry throws "No HTTP
 * mapping for command" at the FIRST call site that needs it, with no fallback.
 *
 * The COMMAND_TABLE → router parity gate (transport.test.ts) reads both this
 * file and transport/commandTable.ts and treats their entries identically — see
 * docs/api/http-api.md -> "Route Parity Gate".
 */

import { type CommandTableEntry, isRecord, registerCommandTableEntries } from "./transport/commandTable";

const EXTENDED_COMMAND_TABLE: Record<string, CommandTableEntry> = {
	secret_form_submit: { map: (args) => ({ method: "POST", path: "/secrets/forms/submit", body: args.submission }) },
	speak_reply: {
		map: (args) => ({
			method: "POST",
			path: "/dictation/speech/speak",
			body: { text: args.text, turn: args.turn },
		}),
	},
	reconnect_mcp_upstream: {
		map: (args) => ({ method: "POST", path: "/mcp/upstreams/reconnect", body: { name: args.name } }),
	},
	save_mcp_upstream_credential: {
		map: (args) => ({
			method: "POST",
			path: "/mcp/upstreams/credential",
			body: {
				name: args.name,
				token: args.token,
				url: args.url,
				...(args.header ? { header: args.header } : {}),
				...(args.replace !== undefined ? { replace: args.replace } : {}),
			},
		}),
	},
	delete_mcp_upstream_credential: {
		map: (args) => ({
			method: "DELETE",
			path: "/mcp/upstreams/credential",
			body: { name: args.name, ...(args.header ? { header: args.header } : {}) },
		}),
	},
	acp_kill: {
		map: (_args, p) => ({ method: "POST", path: `/acp/connections/${p("connectionId")}/kill` }),
	},
	acp_session_resume: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/resume`,
			body: { authority: args.authority },
		}),
	},
	// Not session-scoped: this one owns the connection it runs on, from launch
	// to shutdown, so there is no id to put in the path.
	acp_one_shot_prompt: {
		map: (args) => ({ method: "POST", path: "/acp/one-shot", body: { root: args.root, prompt: args.prompt } }),
	},

	// --- ego's command line (Providers) ---
	// Not part of the ACP surface above: ACP carries a session, and which model
	// a run defaults to is ego's own configuration. Both routes start a process,
	// so both are behind the spawn guard on the Rust side.
	ego_providers: {
		map: (args) => ({
			method: "GET",
			path: args.refresh ? "/ego/providers?refresh=true" : "/ego/providers",
		}),
	},
	ego_set_default_model: {
		map: (args) => ({ method: "POST", path: "/ego/providers/model", body: { model: args.model } }),
	},

	// --- Terminal grid commands ---
	set_terminal_theme_colors: {
		map: (args) => ({
			method: "POST",
			path: "/terminal/theme-colors",
			body: { foreground: args.foreground, background: args.background, cursor: args.cursor },
		}),
	},
	terminal_scroll: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/scroll`,
			body: { delta: args.delta },
		}),
	},
	terminal_scroll_to: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/scroll-to`,
			body: { line: args.line },
		}),
	},
	terminal_scroll_to_offset: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/scroll-to-offset`,
			body: { offset: args.offset },
		}),
	},
	terminal_scroll_info: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/scroll-info`,
		}),
	},
	terminal_search: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/search`,
			body: { query: args.query },
			transform: (data) => (data as { matches: unknown[] }).matches,
		}),
	},
	terminal_search_buffer: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/search-buffer`,
			body: { query: args.query },
			transform: (data) => (data as { matches: unknown[] }).matches,
		}),
	},
	terminal_get_row_text: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/row-text?row=${args.row}`,
			transform: (data) => (data as { text: string }).text,
		}),
	},
	terminal_styled_rows: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/styled-rows?start=${args.start}&count=${args.count}`,
		}),
	},
	terminal_get_cursor_line: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/cursor-line`,
			transform: (data) => (data as { text: string }).text,
		}),
	},
	terminal_hyperlink_span: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/hyperlink-span?row=${args.row}&col=${args.col}`,
			// Option<(start,end,url)> -> [start,end,url] | null; pass null through the empty-body guard.
			transform: (data) => data ?? null,
		}),
	},
	terminal_image_ref_at: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/image-ref?row=${args.row}&col=${args.col}`,
			// Option<(imageId,placementId,tileCol,tileRow)> -> [...] | null.
			transform: (data) => data ?? null,
		}),
	},
	terminal_image_bytes: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/image?id=${args.imageId}`,
		}),
	},
	terminal_image_meta: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/image-meta?id=${args.imageId}`,
			// Option<(mime,intrinsicWidth,intrinsicHeight)> -> [...] | null.
			transform: (data) => data ?? null,
		}),
	},
	terminal_image_placements: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/image-placements`,
			// Array of [placementId, imageId, absRow, col, rows, cols, zIndex]
			// tuples on both transports — no transform needed.
		}),
	},
	terminal_get_selection_text: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/selection-text?startRow=${args.startRow}&startCol=${args.startCol}&endRow=${args.endRow}&endCol=${args.endCol}${args.historyBase === undefined ? "" : `&historyBase=${args.historyBase}`}`,
			transform: (data) => (data as { text: string }).text,
		}),
	},
	terminal_get_logical_line: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/logical-line?row=${args.row}`,
		}),
	},
	terminal_request_frame: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/request-frame`,
		}),
	},
	get_process_stats: { map: () => ({ method: "GET", path: "/process/stats" }) },
	get_claude_project_list: { map: () => ({ method: "GET", path: "/claude/projects" }) },
	get_codex_usage_api: { map: () => ({ method: "GET", path: "/codex/usage" }) },
	get_codex_usage_stats: { map: () => ({ method: "GET", path: "/codex/stats" }) },
	get_grok_usage_api: { map: () => ({ method: "GET", path: "/grok/usage" }) },
	get_claude_usage_timeline: {
		map: (args, p) => {
			let path = `/claude/timeline?scope=${p("scope")}`;
			if (args.days != null) path += `&days=${encodeURIComponent(String(args.days))}`;
			return { method: "GET", path };
		},
	},
	get_claude_session_stats: {
		map: (_args, p) => ({ method: "GET", path: `/claude/session-stats?scope=${p("scope")}` }),
	},

	// --- Config: defaults (Settings "expert mode") ---
	get_config_defaults: {
		map: (args) =>
			args.preview
				? { method: "POST", path: "/config/defaults", body: args.preview }
				: { method: "GET", path: "/config/defaults" },
	},
	story_action_command: {
		map: (args, p) => ({
			method: "POST",
			path: `/stories/action?path=${p("project")}`,
			body: { action: args.action, sessionId: args.sessionId },
		}),
	},
	story_capabilities: {
		map: () => ({ method: "GET", path: "/stories/capabilities" }),
	},
	workflow_definition_action: {
		map: (args, p) => ({
			method: "POST",
			path: `/workflows/definition/action?path=${p("project")}`,
			body: args.action,
		}),
	},
	workflow_run_action: {
		map: (args, p) => ({
			method: "POST",
			path: `/workflows/run/action?path=${p("project")}`,
			body: args.action,
		}),
	},

	// --- Git panel (story 064) ---
	get_gutter_changes: {
		map: (args, p) => {
			let path = `/repo/gutter-changes?path=${p("path")}&file=${p("file")}`;
			if (args.scope != null) path += `&scope=${encodeURIComponent(String(args.scope))}`;
			return { method: "GET", path };
		},
	},
	get_branches_detail: {
		map: (_args, p) => ({ method: "GET", path: `/repo/branches-detail?path=${p("path")}` }),
	},
	get_recent_branches: {
		map: (args, p) => {
			let path = `/repo/recent-branches?path=${p("path")}`;
			if (args.limit != null) path += `&limit=${encodeURIComponent(String(args.limit))}`;
			return { method: "GET", path };
		},
	},
	get_branch_base: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/branch-base?path=${p("path")}&branchName=${p("branchName")}`,
			// Option<String> -> null on miss; pass null through the empty-body guard.
			transform: (data) => data ?? null,
		}),
	},
	check_worktree_dirty: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/worktree-dirty?repoPath=${p("repoPath")}&workspaceId=${p("workspaceId")}`,
		}),
	},
	get_commit_graph: {
		map: (args, p) => {
			let path = `/repo/commit-graph?path=${p("path")}`;
			if (args.count != null) path += `&count=${encodeURIComponent(String(args.count))}`;
			return { method: "GET", path };
		},
	},
	delete_branch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/delete-branch",
			body: { path: args.path, name: args.name, force: args.force },
		}),
	},
	delete_local_branch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/delete-local-branch",
			body: {
				repoPath: args.repoPath,
				branchName: args.branchName,
				workspaceId: args.workspaceId,
				keepWorktree: args.keepWorktree,
			},
		}),
	},
	update_from_base: {
		map: (args) => ({
			method: "POST",
			path: "/repo/update-from-base",
			body: { path: args.path, branchName: args.branchName, strategy: args.strategy },
		}),
	},
	get_all_pr_statuses: {
		map: (args) => ({
			method: "POST",
			path: "/repo/prs/batch",
			body: { paths: args.paths, include_merged: args.includeMerged },
		}),
	},
	close_issue: {
		map: (args) => ({
			method: "POST",
			path: "/repo/issues/close",
			body: { repoPath: args.repoPath, issueNumber: args.issueNumber },
		}),
	},
	reopen_issue: {
		map: (args) => ({
			method: "POST",
			path: "/repo/issues/reopen",
			body: { repoPath: args.repoPath, issueNumber: args.issueNumber },
		}),
	},
	get_issue_detail: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/issue-detail?repoPath=${p("repoPath")}&issueNumber=${p("issueNumber")}`,
		}),
	},
	create_pr: {
		map: (args) => ({
			method: "POST",
			path: "/repo/create-pr",
			body: {
				repoPath: args.repoPath,
				title: args.title,
				body: args.body,
				base: args.base,
				head: args.head,
				draft: args.draft ?? false,
			},
		}),
	},
	create_issue: {
		map: (args) => ({
			method: "POST",
			path: "/repo/create-issue",
			body: { repoPath: args.repoPath, title: args.title, body: args.body },
		}),
	},
	post_pr_review: {
		map: (args) => ({
			method: "POST",
			path: "/repo/post-pr-review",
			body: {
				repoPath: args.repoPath,
				prNumber: args.prNumber,
				body: args.body,
				event: args.event,
				comments: args.comments ?? [],
			},
		}),
	},
	fetch_ci_failure_logs: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/ci-failure-logs?repoPath=${p("repoPath")}&branch=${p("branch")}${_args.checkUrl ? `&checkUrl=${p("checkUrl")}` : ""}${_args.headSha ? `&headSha=${p("headSha")}` : ""}`,
		}),
	},
	circleci_token_status: {
		map: () => ({ method: "GET", path: "/circleci/token" }),
	},
	circleci_set_token: {
		map: (args) => ({ method: "POST", path: "/circleci/token", body: { token: args.token } }),
	},
	circleci_delete_token: {
		map: () => ({ method: "DELETE", path: "/circleci/token" }),
	},
	github_start_login: {
		map: () => ({ method: "POST", path: "/github/auth/start" }),
	},
	github_poll_login: {
		map: (args) => ({
			method: "POST",
			path: "/github/auth/poll",
			body: { deviceCode: args.deviceCode },
		}),
	},
	github_poll_add_account: {
		map: (args) => ({
			method: "POST",
			path: "/github/accounts/poll",
			body: { deviceCode: args.deviceCode },
		}),
	},
	github_logout: {
		map: () => ({ method: "POST", path: "/github/auth/logout" }),
	},
	github_disconnect: {
		map: () => ({ method: "POST", path: "/github/auth/disconnect" }),
	},
	github_auth_status: {
		map: () => ({ method: "GET", path: "/github/auth/status" }),
	},
	github_diagnostics: {
		map: () => ({ method: "GET", path: "/github/diagnostics" }),
	},
	// Multi-account: accounts + repo bindings
	github_list_accounts: {
		map: () => ({ method: "GET", path: "/github/accounts" }),
	},
	github_remove_account: {
		map: (args) => ({ method: "POST", path: "/github/accounts/remove", body: { id: args.id } }),
	},
	github_list_bindings: {
		map: () => ({ method: "GET", path: "/github/bindings" }),
	},
	github_bind_repo: {
		map: (args) => ({
			method: "POST",
			path: "/github/bindings",
			body: { repoPath: args.repoPath, accountId: args.accountId, remoteName: args.remoteName },
		}),
	},
	github_unbind_repo: {
		map: (args) => ({ method: "POST", path: "/github/bindings/remove", body: { repoPath: args.repoPath } }),
	},
	github_resolve_repo: {
		map: (_args, p) => ({ method: "GET", path: `/github/resolve-repo?repoPath=${p("repoPath")}` }),
	},
	github_resolve_repos: {
		map: (args) => ({ method: "POST", path: "/github/resolve-repos", body: { repoPaths: args.repoPaths } }),
	},
	// --- Story 066: config / themes / notes / misc ---
	save_repo_local_config: {
		map: (args) => ({
			method: "POST",
			path: "/config/repo-local-config",
			body: { repoPath: args.repoPath },
		}),
	},
	execute_shell_script: {
		map: (args) => ({
			method: "POST",
			path: "/exec/shell-script",
			body: {
				scriptContent: args.scriptContent,
				timeoutMs: args.timeoutMs,
				repoPath: args.repoPath,
			},
		}),
	},
	list_audio_output_devices: {
		map: () => ({ method: "GET", path: "/audio/output-devices" }),
	},
	discover_agent_session: {
		map: (args) => ({
			method: "POST",
			path: "/agent/discover-session",
			body: {
				agentType: args.agentType,
				cwd: args.cwd,
				claimedIds: args.claimedIds,
				agentPid: args.agentPid,
				envOverrides: args.envOverrides,
			},
		}),
	},
	open_in_custom: {
		map: (args) => ({
			method: "POST",
			path: "/agent/open-in-custom",
			body: { executable: args.executable, args: args.args, ctx: args.ctx },
		}),
	},
	generate_value: {
		map: (args) => ({ method: "POST", path: "/generators/generate", body: { request: args.request } }),
	},
	get_pr_review_threads: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/pr-review-threads?path=${p("path")}&pr_number=${p("prNumber")}`,
		}),
	},
	get_initials: {
		map: (_args, p) => ({ method: "GET", path: `/repo/initials?name=${p("name")}` }),
	},
	check_is_main_branch: {
		map: (_args, p) => ({ method: "GET", path: `/repo/is-main-branch?branch=${p("branch")}` }),
	},
	run_git_command: {
		map: (args) => ({
			method: "POST",
			path: "/repo/run-git",
			body: { path: args.path, args: args.args },
		}),
	},
	get_working_tree_status: {
		map: (_args, p) => ({ method: "GET", path: `/repo/working-tree-status?path=${p("path")}` }),
	},
	git_stage_files: {
		map: (args) => ({
			method: "POST",
			path: "/repo/stage",
			body: { path: args.path, files: args.files },
		}),
	},
	git_unstage_files: {
		map: (args) => ({
			method: "POST",
			path: "/repo/unstage",
			body: { path: args.path, files: args.files },
		}),
	},
	git_discard_files: {
		map: (args) => ({
			method: "POST",
			path: "/repo/discard",
			body: { path: args.path, files: args.files },
		}),
	},
	git_apply_reverse_patch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/apply-reverse-patch",
			body: { path: args.path, patch: args.patch, scope: args.scope },
		}),
	},
	git_commit: {
		map: (args) => ({
			method: "POST",
			path: "/repo/commit",
			body: { path: args.path, message: args.message, amend: args.amend },
		}),
	},
	get_commit_log: {
		map: (args, p) => {
			let url = `/repo/commit-log?path=${p("path")}`;
			if (args.count != null) url += `&count=${args.count}`;
			if (args.after) url += `&after=${encodeURIComponent(String(args.after))}`;
			return { method: "GET", path: url };
		},
	},
	get_stash_list: {
		map: (_args, p) => ({ method: "GET", path: `/repo/stash?path=${p("path")}` }),
	},
	git_stash_apply: {
		map: (args) => ({
			method: "POST",
			path: "/repo/stash/apply",
			body: { path: args.path, stash_ref: args.stashRef },
		}),
	},
	git_stash_pop: {
		map: (args) => ({
			method: "POST",
			path: "/repo/stash/pop",
			body: { path: args.path, stash_ref: args.stashRef },
		}),
	},
	git_stash_drop: {
		map: (args) => ({
			method: "POST",
			path: "/repo/stash/drop",
			body: { path: args.path, stash_ref: args.stashRef },
		}),
	},
	git_stash_show: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/stash/show?path=${p("path")}&stash_ref=${p("stashRef")}`,
		}),
	},
	get_file_history: {
		map: (args, p) => {
			let url = `/repo/file-history?path=${p("path")}&file=${p("file")}`;
			if (args.count != null) url += `&count=${args.count}`;
			if (args.after) url += `&after=${encodeURIComponent(String(args.after))}`;
			return { method: "GET", path: url };
		},
	},
	get_file_blame: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/file-blame?path=${p("path")}&file=${p("file")}`,
		}),
	},
	get_pr_diff: {
		map: (args, p) => ({
			method: "GET",
			path: `/repo/pr-diff?path=${p("repoPath")}&pr=${args.prNumber}`,
		}),
	},
	get_merged_prs: {
		map: (args, p) => ({
			method: "GET",
			path:
				`/repo/merged-prs?path=${p("repoPath")}` +
				(args.sinceTag ? `&sinceTag=${encodeURIComponent(String(args.sinceTag))}` : ""),
		}),
	},
	generate_changelog: {
		map: (args, p) => ({
			method: "GET",
			path:
				`/repo/changelog?path=${p("repoPath")}` +
				(args.sinceTag ? `&sinceTag=${encodeURIComponent(String(args.sinceTag))}` : ""),
		}),
	},
	run_pr_review: {
		map: (args) => ({
			method: "POST",
			path: "/repo/pr-review",
			body: { repoPath: args.repoPath, prNumber: args.prNumber },
		}),
	},
	run_improvement_scan: {
		map: (args) => ({
			method: "POST",
			path: "/repo/improvement-scan",
			body: { repoPath: args.repoPath, focus: args.focus },
		}),
	},
	create_issue_from_proposal: {
		map: (args) => ({
			method: "POST",
			path: "/repo/create-issue-from-proposal",
			body: { repoPath: args.repoPath, proposal: args.proposal },
		}),
	},
	start_conflict_assist: {
		map: (args) => ({
			method: "POST",
			path: "/repo/conflict-assist",
			body: { repoPath: args.repoPath, prNumber: args.prNumber },
		}),
	},
	approve_pr: {
		map: (args) => ({
			method: "POST",
			path: "/repo/approve-pr",
			body: { repoPath: args.repoPath, prNumber: args.prNumber },
		}),
	},
	update_pr_branch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/update-pr-branch",
			body: { repoPath: args.repoPath, prNumber: args.prNumber, expectedHeadSha: args.expectedHeadSha },
		}),
	},
	close_pr: {
		map: (args) => ({
			method: "POST",
			path: "/repo/close-pr",
			body: { repoPath: args.repoPath, prNumber: args.prNumber },
		}),
	},
	resolve_prompt_variables: {
		map: (args) => ({
			method: "POST",
			path: "/prompt/resolve-prompt-variables",
			body: { content: args.content, repoPath: args.repoPath },
		}),
	},
	execute_headless_prompt: {
		map: (args) => ({
			method: "POST",
			path: "/prompt/execute-headless",
			body: {
				command: args.command,
				args: args.args,
				stdinContent: args.stdinContent,
				timeoutMs: args.timeoutMs,
				repoPath: args.repoPath,
				env: args.env,
			},
		}),
	},

	// --- Agents ---
	verify_agent_session: {
		map: (args) => ({
			method: "POST",
			path: "/agents/verify-session",
			body: {
				agentType: args.agentType,
				sessionId: args.sessionId,
				cwd: args.cwd,
				agentPid: args.agentPid,
				envOverrides: args.envOverrides,
			},
		}),
	},
	detect_agents: { map: () => ({ method: "GET", path: "/agents" }) },
	detect_agent_binary: {
		map: (_args, p) => ({ method: "GET", path: `/agents/detect?binary=${p("binary")}` }),
	},
	detect_claude_binary: {
		map: () => ({
			method: "GET",
			path: "/agents/detect?binary=claude",
			transform: (data) => {
				const path = isRecord(data) ? data.path : undefined;
				if (typeof path !== "string" || path.length === 0) {
					throw new Error("Claude binary not found. Install with: npm install -g @anthropic-ai/claude-code");
				}
				return path;
			},
		}),
	},
	detect_installed_ides: { map: () => ({ method: "GET", path: "/agents/ides" }) },
	start_dir_watcher: {
		map: (_args, p) => ({ method: "POST", path: `/watchers/dir?path=${p("path")}` }),
	},
	stop_dir_watcher: {
		map: (_args, p) => ({ method: "DELETE", path: `/watchers/dir?path=${p("path")}` }),
	},

	// --- MCP status ---
	get_mcp_status: { map: () => ({ method: "GET", path: "/mcp/status" }) },

	// --- Network ---
	get_local_ip: { map: () => ({ method: "GET", path: "/system/local-ip" }) },
	get_local_ips: { map: () => ({ method: "GET", path: "/system/local-ips" }) },
	get_home_directory: { map: () => ({ method: "GET", path: "/system/home-directory" }) },
	read_editor_file: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read-editor?repoPath=${p("repoPath")}&file=${p("file")}` }),
	},
	read_editor_file_external: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read-editor-external?path=${p("path")}` }),
	},
	create_directory: {
		map: (args) => ({
			method: "POST",
			path: "/fs/mkdir",
			body: { repoPath: args.repoPath, dir: args.dir },
		}),
	},
	delete_path: {
		map: (args) => ({
			method: "POST",
			path: "/fs/delete",
			body: { repoPath: args.repoPath, path: args.path },
		}),
	},
	rename_path: {
		map: (args) => ({
			method: "POST",
			path: "/fs/rename",
			body: { repoPath: args.repoPath, from: args.from, to: args.to },
		}),
	},
	copy_path: {
		map: (args) => ({
			method: "POST",
			path: "/fs/copy",
			body: { repoPath: args.repoPath, from: args.from, to: args.to },
		}),
	},
	add_to_gitignore: {
		map: (args) => ({
			method: "POST",
			path: "/fs/gitignore",
			body: { repoPath: args.repoPath, pattern: args.pattern },
		}),
	},
	// POST, unlike its single-candidate sibling: a whole screen's candidates do
	// not fit a query string, and being able to send many is the point.
	resolve_terminal_paths: {
		map: (args) => ({
			method: "POST",
			path: "/fs/resolve-terminal-paths",
			body: { cwd: args.cwd, candidates: args.candidates },
		}),
	},
	copy_path_abs: {
		map: (args) => ({ method: "POST", path: "/fs/copy-abs", body: { from: args.from, to: args.to } }),
	},
	move_path_abs: {
		map: (args) => ({ method: "POST", path: "/fs/move-abs", body: { from: args.from, to: args.to } }),
	},
	fs_transfer_paths: {
		map: (args) => ({
			method: "POST",
			path: "/fs/transfer",
			body: {
				destDir: args.destDir,
				paths: args.paths,
				mode: args.mode,
				allowRecursive: args.allowRecursive,
			},
		}),
	},
	search_content: {
		map: (args, p) => {
			let path = `/fs/search-content?repoPath=${p("repoPath")}&query=${p("query")}&caseSensitive=${p("caseSensitive")}&useRegex=${p("useRegex")}&wholeWord=${p("wholeWord")}`;
			if (args.limit != null) path += `&limit=${encodeURIComponent(String(args.limit))}`;
			return { method: "GET", path };
		},
	},
	search_content_all: {
		map: (args, p) => {
			let path = `/fs/search-content-all?query=${p("query")}&caseSensitive=${p("caseSensitive")}`;
			if (args.limit != null) path += `&limit=${encodeURIComponent(String(args.limit))}`;
			return { method: "GET", path };
		},
	},
	fetch_remote_connection_token: {
		map: (args, p) => ({
			method: "POST",
			path: `/config/remote-connections/${p("id")}/token`,
			body: { baseUrl: args.baseUrl, username: args.username },
		}),
	},
	stop_ssh_daemon: {
		map: (_args, p) => ({ method: "POST", path: `/config/ssh-daemon/${p("id")}/stop` }),
	},

	// --- Tunnels ---
	start_design_mode: {
		map: (args) => ({
			method: "POST",
			path: "/design-mode/start",
			body: {
				sessionId: args.sessionId,
				...(args.browser ? { browser: args.browser } : {}),
				...(args.url !== undefined ? { url: args.url } : {}),
			},
		}),
	},
	stop_design_mode: {
		map: (args) => ({ method: "POST", path: "/design-mode/stop", body: { repoPath: args.repoPath } }),
	},
	get_design_mode_status: { map: () => ({ method: "GET", path: "/design-mode" }) },
	get_tunnel_audit: { map: (args) => ({ method: "GET", path: `/tunnels/audit/${args.id}?limit=${args.limit || 20}` }) },
	list_ssh_config_hosts: { map: () => ({ method: "GET", path: "/tunnels/ssh-hosts" }) },
	get_plugin_readme_path: {
		map: (_args, p) => ({
			method: "GET",
			path: `/api/plugins/${p("id")}/readme`,
			// Option<String>: null means no README; pass null through.
			transform: (data) => data ?? null,
		}),
	},
};

registerCommandTableEntries(EXTENDED_COMMAND_TABLE);
