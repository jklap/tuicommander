/**
 * Extended COMMAND_TABLE entries — desktop/Settings-only Tauri<->HTTP command
 * mappings that mobile.html's browser-mode bundle does not need.
 *
 * mobile.html serves phone/PWA clients (always browser mode — no Tauri IPC
 * bridge), and its initial JS bundle has a hard 100KB-gzip budget
 * (scripts/report-frontend-bundles.mjs). transport.ts's COMMAND_TABLE is one
 * flat object literal that a bundler can't tree-shake per entry, so every
 * command mapping ships to every bundle that imports mapCommandToHttp/rpc —
 * including mobile, which never calls most of them.
 *
 * This file holds the commands that NO module reachable from mobile's entry
 * references — traced over mobile's full import graph, its lazily loaded
 * screens included (chat/ACP, Files, Settings, Activity, session detail),
 * 2026-10-07. Everything mobile can reach stays in transport.ts's own table.
 * Only the non-mobile entries import this file, for its registration side
 * effect below: the desktop app (src/appEntry.tsx) and the dev fixture pages
 * (src/dev/*). Mobile's entry (src/mobile/index.tsx) must never import it,
 * directly or transitively, or the split stops doing anything.
 *
 * If you add a command mobile might plausibly call — including from one of its
 * lazy screens or from a shared store/component it renders — put it in
 * transport.ts's COMMAND_TABLE instead of here: a missing entry throws "No HTTP
 * mapping for command" at the FIRST call site that needs it, with no fallback.
 *
 * The COMMAND_TABLE → router parity gate (transport.test.ts) reads both this
 * file and transport.ts and treats their entries identically — see
 * docs/api/http-api.md -> "Route Parity Gate".
 */

import { type CommandTableEntry, diffOptionsQueryString, isRecord, registerCommandTableEntries } from "./transport";

const EXTENDED_COMMAND_TABLE: Record<string, CommandTableEntry> = {
	secret_form_submit: { map: (args) => ({ method: "POST", path: "/secrets/forms/submit", body: args.submission }) },

	// --- Dictation ---
	get_dictation_status: { map: () => ({ method: "GET", path: "/dictation/status" }) },
	get_model_info: { map: () => ({ method: "GET", path: "/dictation/models" }) },
	download_whisper_model: {
		map: (args) => ({ method: "POST", path: "/dictation/models/download", body: { model: args.modelName } }),
	},
	delete_whisper_model: {
		map: (args) => ({ method: "POST", path: "/dictation/models/delete", body: { model: args.modelName } }),
	},
	get_speech_assets: { map: () => ({ method: "GET", path: "/dictation/speech/assets" }) },
	download_speech_asset: {
		map: (args) => ({ method: "POST", path: "/dictation/speech/assets/download", body: { asset: args.asset } }),
	},
	cancel_speech_download: {
		map: (args) => ({ method: "POST", path: "/dictation/speech/assets/cancel", body: { asset: args.asset } }),
	},
	delete_speech_asset: {
		map: (args) => ({ method: "POST", path: "/dictation/speech/assets/delete", body: { asset: args.asset } }),
	},
	get_speech_voices: {
		map: (args) => ({
			method: "GET",
			path: `/dictation/speech/voices?language=${encodeURIComponent(String(args.language))}`,
		}),
	},
	get_edge_voices: {
		map: (args) => ({
			method: "GET",
			path: `/dictation/speech/edge-voices?language=${encodeURIComponent(String(args.language))}`,
		}),
	},
	import_speech_voice: {
		map: (args) => ({
			method: "POST",
			path: "/dictation/speech/voices/import",
			body: { language: args.language, name: args.name, dataBase64: args.dataBase64 },
		}),
	},
	delete_speech_voice: {
		map: (args) => ({
			method: "POST",
			path: "/dictation/speech/voices/delete",
			body: { language: args.language, name: args.name },
		}),
	},
	preview_speech_voice: {
		map: (args) => ({
			method: "POST",
			path: "/dictation/speech/voices/preview",
			body: { language: args.language, voice: args.voice, text: args.text },
		}),
	},
	speak_reply: {
		map: (args) => ({
			method: "POST",
			path: "/dictation/speech/speak",
			body: { text: args.text, turn: args.turn },
		}),
	},
	stop_speech: { map: () => ({ method: "POST", path: "/dictation/speech/stop" }) },
	pause_speech: { map: () => ({ method: "POST", path: "/dictation/speech/pause" }) },
	resume_speech: { map: () => ({ method: "POST", path: "/dictation/speech/resume" }) },
	get_speech_status: {
		map: (args) => ({
			method: "GET",
			path: args.utterance
				? `/dictation/speech/status?utterance=${encodeURIComponent(String(args.utterance))}`
				: "/dictation/speech/status",
		}),
	},
	start_dictation: { map: (args) => ({ method: "POST", path: "/dictation/start", body: { source: args.source } }) },
	stop_dictation_and_transcribe: { map: () => ({ method: "POST", path: "/dictation/stop" }) },
	get_correction_map: { map: () => ({ method: "GET", path: "/dictation/corrections" }) },
	set_correction_map: {
		map: (args) => ({ method: "PUT", path: "/dictation/corrections", body: { map: args.map } }),
	},
	list_audio_devices: { map: () => ({ method: "GET", path: "/dictation/devices" }) },
	inject_text: {
		map: (args) => ({ method: "POST", path: "/dictation/inject", body: { text: args.text } }),
	},
	get_dictation_config: { map: () => ({ method: "GET", path: "/dictation/config" }) },
	get_hands_free_status: { map: () => ({ method: "GET", path: "/dictation/hands-free" }) },
	get_hands_free_default_notice: {
		map: () => ({ method: "GET", path: "/dictation/hands-free/default-notice" }),
	},
	// camelCase on the wire in both directions: the axum request type renames to
	// match the IPC argument names, so the same store code works on either.
	arm_hands_free_dictation: {
		map: (args) => ({
			method: "POST",
			path: "/dictation/hands-free/arm",
			body: { sessionId: args.sessionId, owner: args.owner },
		}),
	},
	disarm_hands_free_dictation: {
		map: () => ({ method: "POST", path: "/dictation/hands-free/disarm" }),
	},
	set_dictation_config: {
		map: (args) => ({ method: "PUT", path: "/dictation/config", body: { base: args.base, config: args.config } }),
	},

	// --- MCP upstream config (proxied through server for keyring access) ---
	load_mcp_upstreams: { map: () => ({ method: "GET", path: "/mcp/upstreams" }) },
	get_mcp_upstream_status: { map: () => ({ method: "GET", path: "/mcp/upstream-status" }) },
	save_mcp_upstreams: {
		map: (args) => ({
			method: "PUT",
			path: "/mcp/upstreams",
			body: { base: args.base, config: args.config },
		}),
	},
	reconnect_mcp_upstream: {
		map: (args) => ({ method: "POST", path: "/mcp/upstreams/reconnect", body: { name: args.name } }),
	},
	save_mcp_upstream_credential: {
		map: (args) => ({
			method: "POST",
			path: "/mcp/upstreams/credential",
			body: { name: args.name, token: args.token },
		}),
	},
	delete_mcp_upstream_credential: {
		map: (args) => ({ method: "DELETE", path: "/mcp/upstreams/credential", body: { name: args.name } }),
	},

	// --- ACP (ego) ---
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
	acp_session_delete: {
		map: (_args, p) => ({
			method: "DELETE",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}`,
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

	// --- Session lifecycle ---
	// The tab's verdict on `session action=suspend`; the MCP call waits for it.
	session_suspend_response: {
		map: (args) => ({
			method: "POST",
			path: "/mcp/suspend-response",
			body: { request_id: args.requestId, ok: args.ok, reason: args.reason ?? null },
		}),
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
	terminal_hyperlink_at: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/hyperlink?row=${args.row}&col=${args.col}`,
			transform: (data) => (data as { url: string | null }).url,
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

	// --- Orchestrator ---
	get_orchestrator_stats: { map: () => ({ method: "GET", path: "/stats" }) },
	get_session_metrics: { map: () => ({ method: "GET", path: "/metrics" }) },
	get_process_stats: { map: () => ({ method: "GET", path: "/process/stats" }) },

	// --- Claude Usage dashboard ---
	get_claude_usage_api: {
		map: (args, p) => ({
			method: "GET",
			path: args.sessionId == null ? "/claude/usage" : `/claude/usage?sessionId=${p("sessionId")}`,
		}),
	},
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
	can_spawn_session: {
		map: () => ({
			method: "GET",
			path: "/stats",
			transform: (data) => {
				const stats = data as { active_sessions: number; max_sessions: number };
				return stats.active_sessions < stats.max_sessions;
			},
		}),
	},

	// --- Config: defaults (Settings "expert mode") ---
	get_config_defaults: { map: () => ({ method: "GET", path: "/config/defaults" }) },

	// --- Project Progress ---
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

	// --- Git/GitHub ---
	get_repo_info: {
		map: (_args, p) => ({ method: "GET", path: `/repo/info?path=${p("path")}` }),
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
	get_workspace_lifecycle: {
		map: (_args, p) => ({
			method: "GET",
			path: `/worktrees/lifecycle?repoPath=${p("repoPath")}&workspaceId=${p("workspaceId")}`,
		}),
	},
	list_base_ref_options: {
		map: (_args, p) => ({ method: "GET", path: `/repo/base-ref-options?repoPath=${p("repoPath")}` }),
	},
	generate_clone_branch_name_cmd: {
		map: (args) => ({
			method: "POST",
			path: "/repo/clone-branch-name",
			body: { sourceBranch: args.sourceBranch, existingNames: args.existingNames },
		}),
	},
	get_commit_graph: {
		map: (args, p) => {
			let path = `/repo/commit-graph?path=${p("path")}`;
			if (args.count != null) path += `&count=${encodeURIComponent(String(args.count))}`;
			return { method: "GET", path };
		},
	},
	create_branch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/create-branch",
			body: { path: args.path, name: args.name, startPoint: args.startPoint, checkout: args.checkout },
		}),
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
	switch_branch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/switch-branch",
			body: { repoPath: args.repoPath, branchName: args.branchName, force: args.force, stash: args.stash },
		}),
	},
	merge_and_archive_worktree: {
		map: (args) => ({
			method: "POST",
			path: "/repo/merge-archive-worktree",
			body: {
				repoPath: args.repoPath,
				branchName: args.branchName,
				workspaceId: args.workspaceId,
				targetBranch: args.targetBranch,
				afterMerge: args.afterMerge,
				force: args.force,
				...(args.expectedFingerprint ? { expectedFingerprint: args.expectedFingerprint } : {}),
			},
		}),
	},
	get_diff_stats: {
		map: (_args, p) => ({ method: "GET", path: `/repo/diff-stats?path=${p("path")}` }),
	},
	get_changed_files: {
		map: (_args, p) => ({ method: "GET", path: `/repo/files?path=${p("path")}` }),
	},
	get_file_diff: {
		map: (args, p) => {
			let diffUrl = `/repo/file-diff?path=${p("path")}&file=${p("file")}`;
			if (args?.scope) diffUrl += `&scope=${encodeURIComponent(String(args.scope))}`;
			if (args?.untracked) diffUrl += `&untracked=true`;
			diffUrl += diffOptionsQueryString(args?.options);
			return { method: "GET", path: diffUrl };
		},
	},
	list_review_sessions: {
		map: (args, p) => {
			let url = `/repo/session-review/sessions?path=${p("repoPath")}`;
			if (args?.limit != null) url += `&limit=${encodeURIComponent(String(args.limit))}`;
			if (args?.includeCounts) url += `&include_counts=true`;
			return { method: "GET", path: url };
		},
	},
	get_session_review: {
		map: (args, p) => {
			let url = `/repo/session-review?path=${p("repoPath")}&session_id=${p("sessionId")}`;
			if (args?.includeSubagents === false) url += `&include_subagents=false`;
			url += diffOptionsQueryString(args?.options);
			return { method: "GET", path: url };
		},
	},
	watch_session_review: {
		map: (args) => ({
			method: "POST",
			path: "/repo/session-review/watch",
			body: { path: args.repoPath, session_id: args.sessionId },
		}),
	},
	unwatch_session_review: {
		map: (args) => ({
			method: "POST",
			path: "/repo/session-review/unwatch",
			body: { path: args.repoPath, session_id: args.sessionId },
		}),
	},
	revert_session_step: {
		map: (args) => ({
			method: "POST",
			path: "/repo/session-review/revert-step",
			body: { path: args.repoPath, session_id: args.sessionId, tool_use_id: args.toolUseId, dry_run: args.dryRun },
		}),
	},
	revert_file_to_session_start: {
		map: (args) => ({
			method: "POST",
			path: "/repo/session-review/revert-file",
			body: {
				path: args.repoPath,
				session_id: args.sessionId,
				abs_path: args.absPath,
				force: args.force,
				dry_run: args.dryRun,
			},
		}),
	},
	get_github_status: {
		map: (_args, p) => ({ method: "GET", path: `/repo/github?path=${p("path")}` }),
	},
	get_repo_pr_statuses: {
		map: (_args, p) => ({ method: "GET", path: `/repo/prs?path=${p("path")}` }),
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
	get_github_viewer_login: {
		map: () => ({ method: "GET", path: "/github/viewer-login" }),
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
	github_set_pr_hide_drafts: {
		map: (args) => ({ method: "POST", path: "/github/pr-hide-drafts", body: { hide: args.hide } }),
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
			path: "/github/auth/poll",
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
	github_add_account: {
		map: (args) => ({ method: "POST", path: "/github/accounts", body: { host: args.host, pat: args.pat } }),
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
	save_note_image: {
		map: (args) => ({
			method: "POST",
			path: "/config/note-image",
			body: { noteId: args.noteId, dataBase64: args.dataBase64, extension: args.extension },
		}),
	},
	set_project_mcp_upstreams: {
		map: (args) => ({
			method: "POST",
			path: "/config/project-mcp-upstreams",
			body: { repoPath: args.repoPath, upstreamNames: args.upstreamNames },
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
	fetch_plugin_registry: {
		map: () => ({ method: "GET", path: "/registry/plugins" }),
	},
	github_start_polling: {
		map: (args) => ({
			method: "POST",
			path: "/repo/github-poller/start",
			body: {
				paths: args.paths,
				issueFilter: args.issueFilter,
				prHideDrafts: args.prHideDrafts,
			},
		}),
	},
	github_stop_polling: {
		map: () => ({ method: "POST", path: "/repo/github-poller/stop" }),
	},
	github_set_visibility: {
		map: (args) => ({
			method: "POST",
			path: "/repo/github-poller/visibility",
			body: { visible: args.visible },
		}),
	},
	github_set_issue_filter: {
		map: (args) => ({
			method: "POST",
			path: "/repo/github-poller/set-issue-filter",
			body: { filter: args.filter },
		}),
	},
	get_merged_branches: {
		map: (_args, p) => ({ method: "GET", path: `/repo/branches/merged?path=${p("repoPath")}` }),
	},
	get_repo_summary: {
		map: (_args, p) => ({ method: "GET", path: `/repo/summary?path=${p("repoPath")}` }),
	},
	get_repo_structure: {
		map: (_args, p) => ({ method: "GET", path: `/repo/structure?path=${p("repoPath")}` }),
	},
	get_repo_diff_stats: {
		map: (_args, p) => ({ method: "GET", path: `/repo/diff-stats/batch?path=${p("repoPath")}` }),
	},
	get_ci_checks: {
		map: (_args, p) => ({ method: "GET", path: `/repo/ci?path=${p("path")}&pr_number=${p("prNumber")}` }),
	},
	get_pr_review_threads: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/pr-review-threads?path=${p("path")}&pr_number=${p("prNumber")}`,
		}),
	},
	rename_branch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/branch/rename",
			body: { path: args.path, old_name: args.oldName, new_name: args.newName },
		}),
	},
	get_initials: {
		map: (_args, p) => ({ method: "GET", path: `/repo/initials?name=${p("name")}` }),
	},
	check_is_main_branch: {
		map: (_args, p) => ({ method: "GET", path: `/repo/is-main-branch?branch=${p("branch")}` }),
	},
	get_remote_url: {
		map: (_args, p) => ({ method: "GET", path: `/repo/remote-url?path=${p("path")}` }),
	},
	get_git_panel_context: {
		map: (_args, p) => ({ method: "GET", path: `/repo/panel-context?path=${p("path")}` }),
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

	// --- Worktrees ---
	list_worktrees: { map: () => ({ method: "GET", path: "/worktrees" }) },
	get_worktrees_dir: {
		map: (args) => {
			const rp = args?.repoPath as string | undefined;
			return {
				method: "GET",
				path: rp ? `/worktrees/dir?repo_path=${encodeURIComponent(rp)}` : "/worktrees/dir",
				transform: (data) => (data as { dir: string }).dir,
			};
		},
	},
	create_worktree: {
		map: (args) => ({
			method: "POST",
			path: "/worktrees",
			body: { base_repo: args.baseRepo, branch_name: args.branchName, base_ref: args.baseRef },
		}),
	},
	remove_worktree: {
		map: (args, p) => {
			const force = args.force === true ? "&force=true" : "";
			const overrideLock = args.overrideLock === true ? "&overrideLock=true" : "";
			const expectedFingerprint = args.expectedFingerprint ? `&expectedFingerprint=${p("expectedFingerprint")}` : "";
			const confirmMissingCheckout = args.confirmMissingCheckout === true ? "&confirmMissingCheckout=true" : "";
			const deleteBranch = args.deleteBranch ?? args.force !== true;
			return {
				method: "DELETE",
				path: `/worktrees/${p("workspaceId")}?repoPath=${p("repoPath")}&deleteBranch=${deleteBranch}${force}${overrideLock}${expectedFingerprint}${confirmMissingCheckout}`,
			};
		},
	},
	generate_worktree_name_cmd: {
		map: (args) => ({
			method: "POST",
			path: "/worktrees/generate-name",
			body: { existing_names: args.existingNames },
		}),
	},
	finalize_merged_worktree: {
		map: (args) => ({
			method: "POST",
			path: "/worktrees/finalize",
			body: {
				repoPath: args.repoPath,
				workspaceId: args.workspaceId,
				action: args.action,
				force: args.force,
				...(args.expectedFingerprint ? { expectedFingerprint: args.expectedFingerprint } : {}),
			},
		}),
	},
	checkout_remote_branch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/checkout-remote",
			body: { repoPath: args.repoPath, branchName: args.branchName },
		}),
	},
	detect_orphan_worktrees: {
		map: (_args, p) => ({ method: "GET", path: `/repo/orphan-worktrees?repoPath=${p("repoPath")}` }),
	},
	assess_orphan_cleanup: {
		map: (_args, p) => ({ method: "GET", path: `/repo/orphan-cleanup-assessment?repoPath=${p("repoPath")}` }),
	},
	begin_orphan_cleanup: {
		map: (args) => ({
			method: "POST",
			path: "/repo/orphan-cleanup/begin",
			body: { repoPath: args.repoPath, paths: args.paths },
		}),
	},
	pending_orphan_cleanup_answer: {
		map: (_args, p) => ({ method: "GET", path: `/repo/orphan-cleanup/pending?repoPath=${p("repoPath")}` }),
	},
	clear_orphan_cleanup: {
		map: (args) => ({
			method: "POST",
			path: "/repo/orphan-cleanup/clear",
			body: { repoPath: args.repoPath, kept: args.kept },
		}),
	},
	remove_orphan_worktree: {
		map: (args) => ({
			method: "POST",
			path: "/repo/remove-orphan",
			body: {
				repoPath: args.repoPath,
				worktreePath: args.worktreePath,
				safeOnly: args.safeOnly ?? false,
				confirmedSessions: args.confirmedSessions ?? [],
			},
		}),
	},
	run_setup_script: {
		map: (args) => ({
			method: "POST",
			path: "/worktrees/run-script",
			body: { script: args.script, cwd: args.cwd },
		}),
	},
	merge_pr_via_github: {
		map: (args) => ({
			method: "POST",
			path: "/repo/merge-pr",
			body: { repoPath: args.repoPath, prNumber: args.prNumber, mergeMethod: args.mergeMethod },
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
	list_local_branches: {
		map: (_args, p) => ({ method: "GET", path: `/repo/local-branches?path=${p("repoPath")}` }),
	},

	// --- Prompt processing ---
	process_prompt_content: {
		map: (args) => ({
			method: "POST",
			path: "/prompt/process",
			body: { content: args.content, variables: args.variables },
		}),
	},
	extract_prompt_variables: {
		map: (args) => ({
			method: "POST",
			path: "/prompt/extract-variables",
			body: { content: args.content },
		}),
	},
	resolve_context_variables: {
		map: (args) => ({
			method: "POST",
			path: "/prompt/resolve-variables",
			body: { repoPath: args.repoPath },
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
	detect_all_agent_binaries: {
		map: (args) => ({ method: "POST", path: "/agents/detect-all", body: { binaries: args.binaries } }),
	},
	detect_agent_binary: {
		map: (_args, p) => ({ method: "GET", path: `/agents/detect?binary=${p("binary")}` }),
	},
	prepare_agent_launch_args: {
		map: (args) => ({
			method: "POST",
			path: "/agents/launch-args",
			body: {
				agentType: args.agentType,
				binaryPath: args.binaryPath,
				args: args.args,
			},
		}),
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
	// Settings → Agents only (the consent prompt itself answers through the core
	// table's agent_wrap_prompt_response, which mobile renders too).
	get_agent_wrap_user_function: {
		map: (_args, p) => ({
			method: "GET",
			path: `/config/agents/${p("agentType")}/wrap-user-function`,
			transform: (data) => (data as { value: boolean | null }).value,
		}),
	},
	set_agent_wrap_user_function: {
		map: (args, p) => ({
			method: "PUT",
			path: `/config/agents/${p("agentType")}/wrap-user-function`,
			body: { value: args.value },
		}),
	},

	// --- Watchers ---
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

	// --- File browser ---
	read_editor_file: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read-editor?repoPath=${p("repoPath")}&file=${p("file")}` }),
	},
	read_external_file: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read-external?path=${p("path")}` }),
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
	resolve_markdown_link: {
		map: (args) => ({
			method: "POST",
			path: "/fs/resolve-markdown-link",
			body: { root: args.root, currentFile: args.currentFile, href: args.href },
		}),
	},
	write_external_file: {
		map: (args) => ({
			method: "POST",
			path: "/fs/write-external",
			body: { path: args.path, content: args.content },
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

	// --- Remote Connections ---
	list_remote_connections: { map: () => ({ method: "GET", path: "/config/remote-connections" }) },
	save_remote_connection: {
		map: (args) => ({
			method: "PUT",
			path: "/config/remote-connections",
			body: { base: args.base, connection: args.connection },
		}),
	},
	delete_remote_connection: {
		map: (_args, p) => ({ method: "DELETE", path: `/config/remote-connections/${p("id")}` }),
	},
	set_remote_connection_password: {
		map: (args, p) => ({
			method: "PUT",
			path: `/config/remote-connections/${p("id")}/password`,
			body: { password: args.password },
		}),
	},
	remote_connection_password_exists: {
		map: (_args, p) => ({ method: "GET", path: `/config/remote-connections/${p("id")}/password` }),
	},
	fetch_remote_connection_token: {
		map: (args, p) => ({
			method: "POST",
			path: `/config/remote-connections/${p("id")}/token`,
			body: { baseUrl: args.baseUrl, username: args.username },
		}),
	},
	// The live half: status is what the backend knows about a connection right
	// now, connect and disconnect ask it to change that. The state machine runs
	// there, so these three are the whole client surface (#790-ef85).
	remote_connection_statuses: {
		map: () => ({ method: "GET", path: "/config/remote-connections/status" }),
	},
	prepare_remote_update: {
		map: (_args, p) => ({ method: "GET", path: `/config/remote-connections/${p("id")}/update` }),
	},
	update_and_restart_remote: {
		map: (args, p) => ({
			method: "POST",
			path: `/config/remote-connections/${p("id")}/update`,
			body: {
				confirmedSessions: args.confirmedSessions,
				expectedSha256: args.expectedSha256,
			},
		}),
	},
	connect_remote_connection: {
		map: (_args, p) => ({ method: "POST", path: `/config/remote-connections/${p("id")}/connect` }),
	},
	disconnect_remote_connection: {
		map: (_args, p) => ({ method: "DELETE", path: `/config/remote-connections/${p("id")}/connect` }),
	},
	install_remote_daemon: {
		map: (_args, p) => ({ method: "POST", path: `/config/remote-connections/${p("id")}/install` }),
	},
	uninstall_remote_daemon: {
		map: (_args, p) => ({ method: "DELETE", path: `/config/remote-connections/${p("id")}/install` }),
	},
	// Test Connection (story: SSH Tunnels + Remote Servers consolidation, Phase 2).
	// `args.request` is the whole `{ transport, auth_username, password }` shape —
	// mirrors the Rust `test_connection(request: TestConnectionRequest)` command's
	// single named parameter, same convention as `save_remote_connection` passing
	// `args.connection` straight through as the body.
	test_connection: {
		map: (args) => ({ method: "POST", path: "/config/remote-connections/test", body: args.request }),
	},
	// What certificate a Direct URL presents, before the user pins it. There is
	// deliberately no command to start/stop the pinned relay: only the backend's
	// connect flow starts one, from the saved connection.
	probe_direct_tls_connection: {
		map: (args) => ({
			method: "POST",
			path: "/config/remote-connections/probe-direct-tls",
			body: { url: args.url, tls_fingerprint: args.tlsFingerprint ?? null },
		}),
	},
	// SSH remote daemon provisioning (`ssh_provision.rs`). Every command names a
	// STORED connection by id and nothing else that reaches the remote host; an
	// execute call carries only the digest of the plan the user accepted.
	plan_ssh_daemon_provision: {
		map: (args, p) => ({
			method: "GET",
			path: `/config/ssh-daemon/${p("id")}/plan?action=${encodeURIComponent(String(args.action))}`,
		}),
	},
	start_ssh_daemon: {
		map: (args, p) => ({
			method: "POST",
			path: `/config/ssh-daemon/${p("id")}/start`,
			body: { plan_digest: args.planDigest },
		}),
	},
	stop_ssh_daemon: {
		map: (_args, p) => ({ method: "POST", path: `/config/ssh-daemon/${p("id")}/stop` }),
	},
	configure_ssh_daemon_password: {
		map: (args, p) => ({
			method: "POST",
			path: `/config/remote-connections/${p("id")}/configure-ssh-password`,
			body: { plan_digest: args.planDigest },
		}),
	},

	// --- Tunnels ---
	start_design_mode: {
		map: (args) => ({ method: "POST", path: "/design-mode/start", body: { sessionId: args.sessionId } }),
	},
	stop_design_mode: {
		map: (args) => ({ method: "POST", path: "/design-mode/stop", body: { repoPath: args.repoPath } }),
	},
	get_design_mode_status: { map: () => ({ method: "GET", path: "/design-mode" }) },
	list_tunnel_profiles: { map: () => ({ method: "GET", path: "/tunnels/profiles" }) },
	save_tunnel_profile: { map: (args) => ({ method: "POST", path: "/tunnels/profiles", body: args.profile }) },
	delete_tunnel_profile: { map: (args) => ({ method: "DELETE", path: `/tunnels/profiles/${args.id}` }) },
	start_tunnel: { map: (args) => ({ method: "POST", path: `/tunnels/start/${args.id}` }) },
	stop_tunnel: { map: (args) => ({ method: "POST", path: `/tunnels/stop/${args.id}` }) },
	list_active_tunnels: { map: () => ({ method: "GET", path: "/tunnels/active" }) },
	get_tunnel_status: { map: (args) => ({ method: "GET", path: `/tunnels/status/${args.id}` }) },
	get_tunnel_audit: { map: (args) => ({ method: "GET", path: `/tunnels/audit/${args.id}?limit=${args.limit || 20}` }) },
	list_ssh_config_hosts: { map: () => ({ method: "GET", path: "/tunnels/ssh-hosts" }) },
	list_discovered_ssh_hosts: { map: () => ({ method: "GET", path: "/tunnels/ssh-hosts/discovered" }) },
	probe_discovered_ssh_host: {
		map: (args) => ({
			method: "POST",
			path: "/tunnels/ssh-hosts/probe",
			body: { target: args.target, port: args.port ?? null },
		}),
	},
	probe_ssh_config_hosts: { map: () => ({ method: "GET", path: "/tunnels/ssh-hosts/status" }) },
	list_ssh_agent_keys: { map: () => ({ method: "GET", path: "/tunnels/agent-keys" }) },

	// --- Story 071: Plugin RPC commands ---
	plugin_write_file_base64: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/fs/write-base64`,
			body: {
				path: args.path,
				content: args.content,
				...(args.maxBytes != null ? { maxBytes: args.maxBytes } : {}),
			},
		}),
	},
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
