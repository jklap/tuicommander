/**
 * Extended COMMAND_TABLE entries — desktop/Settings-only Tauri<->HTTP command
 * mappings that mobile.html's browser-mode bundle does not need.
 *
 * mobile.html serves phone/PWA clients (always browser mode — no Tauri IPC
 * bridge), and its initial JS bundle has a hard 100KB-gzip budget
 * (scripts/report-frontend-bundles.mjs). transport.ts's COMMAND_TABLE is one
 * flat object literal that a bundler can't tree-shake per entry, so every
 * command mapping ships to every bundle that imports mapCommandToHttp/rpc —
 * including mobile, which (verified by tracing mobile's real static + plugin
 * import graph, 2026-09-23) never calls the desktop-only ACP, GitHub PR/issue,
 * Git Panel, Worktrees, AI chat/watchers/scheduler, Remote Connections/SSH/
 * Tunnels, dictation, provider-registry, terminal-grid-frame, or File Browser
 * command families below.
 *
 * This file holds exactly that unused-by-mobile subset, split out into its own
 * module so mobile's static import graph never pulls it in. Only the desktop
 * entry (src/index.tsx) imports this file, for its registration side effect
 * below — mobile's entry (src/mobile/index.tsx) must never import it, directly
 * or transitively, or the split stops doing anything.
 *
 * If you add a command mobile might plausibly call, put it in transport.ts's
 * COMMAND_TABLE instead of here — a missing entry throws "No HTTP mapping for
 * command" at the FIRST call site that needs it, with no fallback.
 *
 * The COMMAND_TABLE → router parity gate (transport.test.ts) reads both this
 * file and transport.ts and treats their entries identically — see
 * docs/api/http-api.md -> "Route Parity Gate".
 */

import { type CommandTableEntry, isRecord, registerCommandTableEntries } from "./transport";

const EXTENDED_COMMAND_TABLE: Record<string, CommandTableEntry> = {
	// --- Dictation ---
	get_dictation_status: { map: () => ({ method: "GET", path: "/dictation/status" }) },
	get_model_info: { map: () => ({ method: "GET", path: "/dictation/models" }) },
	download_whisper_model: {
		map: (args) => ({ method: "POST", path: "/dictation/models/download", body: { model: args.model_name } }),
	},
	delete_whisper_model: {
		map: (args) => ({ method: "POST", path: "/dictation/models/delete", body: { model: args.model_name } }),
	},
	start_dictation: { map: () => ({ method: "POST", path: "/dictation/start" }) },
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
	set_dictation_config: {
		map: (args) => ({ method: "PUT", path: "/dictation/config", body: args.config }),
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
	// One entry per acp_* command, session-scoped like the routes they map to.
	// The bodies carry exactly the arguments the Tauri command takes, because
	// the client's refusals are computed in Rust and must be identical on both
	// transports — a body that dropped a field would move a decision here.
	acp_connect: {
		map: (args) => ({ method: "POST", path: "/acp/connections", body: { root: args.root } }),
	},
	acp_connection_snapshot: {
		map: (_args, p) => ({ method: "GET", path: `/acp/connections/${p("connectionId")}` }),
	},
	acp_disconnect: {
		map: (_args, p) => ({ method: "DELETE", path: `/acp/connections/${p("connectionId")}` }),
	},
	acp_kill: {
		map: (_args, p) => ({ method: "POST", path: `/acp/connections/${p("connectionId")}/kill` }),
	},
	acp_reconnect: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/reconnect`,
			body: { root: args.root },
		}),
	},
	acp_session_new: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions`,
			body: { authority: args.authority },
		}),
	},
	acp_session_list: {
		// Query params are appended via `path +=`, not interpolated into the base template
		// literal below — extractPathTemplate's static extraction only captures that base
		// assignment, so folding the querystring into it (e.g. `${suffix}` inline) makes
		// normalizeRouteShape collapse it into a second, spurious `:param` glued onto
		// "sessions" with no separator. Match the convention every other optional-query
		// mapper in this file already uses (get_recent_branches, get_tunnel_audit, etc).
		map: (args, p) => {
			const query = new URLSearchParams();
			if (args.cwd !== undefined && args.cwd !== null) query.set("cwd", String(args.cwd));
			if (args.cursor !== undefined && args.cursor !== null) query.set("cursor", String(args.cursor));
			let path = `/acp/connections/${p("connectionId")}/sessions`;
			if (query.toString()) path += `?${query.toString()}`;
			return { method: "GET", path };
		},
	},
	acp_session_load: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/load`,
			body: { authority: args.authority },
		}),
	},
	acp_session_resume: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/resume`,
			body: { authority: args.authority },
		}),
	},
	acp_session_fork: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/fork`,
			body: { authority: args.authority },
		}),
	},
	acp_session_delete: {
		map: (_args, p) => ({
			method: "DELETE",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}`,
		}),
	},
	acp_session_close: {
		map: (_args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/close`,
		}),
	},
	acp_session_prompt: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/prompt`,
			body: { prompt: args.prompt },
		}),
	},
	acp_session_cancel: {
		map: (_args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/cancel`,
		}),
	},
	acp_session_set_config_option: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/config`,
			body: { configId: args.configId, value: args.value },
		}),
	},
	acp_turn_pause: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/pause`,
			body: { requestId: args.requestId },
		}),
	},
	acp_turn_resume: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/resume-turn`,
			body: { requestId: args.requestId },
		}),
	},
	acp_session_compact: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/compact`,
			body: { requestId: args.requestId },
		}),
	},
	acp_pending_interactions: {
		map: (_args, p) => ({ method: "GET", path: `/acp/connections/${p("connectionId")}/interactions` }),
	},
	acp_respond_permission: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/permissions/${p("requestId")}/response`,
			body: { outcome: args.outcome },
		}),
	},
	acp_respond_elicitation: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/elicitations/${p("requestId")}/response`,
			body: { action: args.action },
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
	terminal_get_lines: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/lines?start=${args.start}&end=${args.end}`,
			transform: (data) => (data as { lines: string[] }).lines,
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
			path: `/sessions/${args.sessionId}/terminal/selection-text?startRow=${args.startRow}&startCol=${args.startCol}&endRow=${args.endRow}&endCol=${args.endCol}`,
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
	get_claude_usage_api: { map: () => ({ method: "GET", path: "/claude/usage" }) },
	get_claude_project_list: { map: () => ({ method: "GET", path: "/claude/projects" }) },
	get_codex_usage_api: { map: () => ({ method: "GET", path: "/codex/usage" }) },
	get_codex_usage_stats: { map: () => ({ method: "GET", path: "/codex/stats" }) },
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

	// --- Config: provider registry ---
	load_provider_registry: { map: () => ({ method: "GET", path: "/config/provider-registry" }) },
	save_provider_registry: {
		map: (args) => ({ method: "PUT", path: "/config/provider-registry", body: args.registry }),
	},
	// --- Story 072: provider API keys (keyring-proxied) + slot/ollama checks ---
	get_provider_api_key_exists: {
		map: (_args, p) => ({ method: "GET", path: `/config/provider-key/exists?providerId=${p("providerId")}` }),
	},
	save_provider_api_key: {
		map: (args) => ({
			method: "POST",
			path: "/config/provider-key",
			body: { providerId: args.providerId, key: args.key },
		}),
	},
	delete_provider_api_key: {
		map: (args) => ({ method: "DELETE", path: "/config/provider-key", body: { providerId: args.providerId } }),
	},
	test_slot_connection: {
		map: (args) => ({ method: "POST", path: "/config/slot-test", body: { slot: args.slot } }),
	},
	check_ollama_models: {
		map: (args) => ({ method: "POST", path: "/config/ollama-models", body: { providerId: args.providerId } }),
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
			return { method: "GET", path: url };
		},
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
	create_issue_from_proposal: {
		map: (args) => ({
			method: "POST",
			path: "/repo/create-issue-from-proposal",
			body: { repoPath: args.repoPath, proposal: args.proposal },
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
			path: `/repo/ci-failure-logs?repoPath=${p("repoPath")}&branch=${p("branch")}`,
		}),
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
	load_ai_prompts: {
		map: () => ({ method: "GET", path: "/config/ai-prompts" }),
	},
	save_ai_prompts: {
		map: (args) => ({ method: "PUT", path: "/config/ai-prompts", body: args.config }),
	},
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
	list_themes: {
		map: () => ({ method: "GET", path: "/config/themes" }),
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
	// --- Story 070: AI watchers (RPC; fires surface as session-created SSE) ---
	watcher_list: {
		map: () => ({ method: "GET", path: "/ai/watchers" }),
	},
	watcher_create: {
		map: (args) => ({
			method: "POST",
			path: "/ai/watchers",
			body: {
				name: args.name,
				sessionId: args.sessionId,
				trigger: args.trigger,
				instructions: args.instructions,
				promptId: args.promptId,
				repoPath: args.repoPath,
				maxFires: args.maxFires,
				cooldownSecs: args.cooldownSecs,
			},
		}),
	},
	watcher_update: {
		map: (args) => ({
			method: "POST",
			path: "/ai/watchers/update",
			body: {
				id: args.id,
				name: args.name,
				trigger: args.trigger,
				instructions: args.instructions,
				promptId: args.promptId,
				repoPath: args.repoPath,
				maxFires: args.maxFires,
				cooldownSecs: args.cooldownSecs,
			},
		}),
	},
	watcher_delete: {
		map: (args) => ({ method: "POST", path: "/ai/watchers/delete", body: { id: args.id } }),
	},
	watcher_toggle: {
		map: (args) => ({
			method: "POST",
			path: "/ai/watchers/toggle",
			body: { id: args.id, enabled: args.enabled },
		}),
	},
	watcher_attach: {
		map: (args) => ({
			method: "POST",
			path: "/ai/watchers/attach",
			body: { templateId: args.templateId, sessionId: args.sessionId },
		}),
	},
	watcher_detach: {
		map: (args) => ({ method: "POST", path: "/ai/watchers/detach", body: { id: args.id } }),
	},
	// --- Story 069: AI chat config + conversation CRUD (chat_subscribe stream = WS, later) ---
	load_ai_chat_config: {
		map: () => ({ method: "GET", path: "/ai/chat/config" }),
	},
	save_ai_chat_config: {
		map: (args) => ({ method: "PUT", path: "/ai/chat/config", body: args.config }),
	},
	list_conversations: {
		map: () => ({ method: "GET", path: "/ai/chat/conversations" }),
	},
	load_conversation: {
		map: (_args, p) => ({ method: "GET", path: `/ai/chat/conversation?id=${p("id")}` }),
	},
	save_conversation: {
		map: (args) => ({ method: "POST", path: "/ai/chat/conversation", body: args.conversation }),
	},
	delete_conversation: {
		map: (args) => ({
			method: "POST",
			path: "/ai/chat/conversation/delete",
			body: { id: args.id },
		}),
	},
	new_conversation_id: {
		map: () => ({ method: "POST", path: "/ai/chat/new-id" }),
	},
	// --- Story 068: agent loop control + knowledge + scheduler (start_conversation = WS, later) ---
	cancel_conversation: {
		map: (args) => ({
			method: "POST",
			path: "/ai/conversation/cancel",
			body: { sessionId: args.sessionId },
		}),
	},
	pause_conversation: {
		map: (args) => ({
			method: "POST",
			path: "/ai/conversation/pause",
			body: { sessionId: args.sessionId },
		}),
	},
	resume_conversation: {
		map: (args) => ({
			method: "POST",
			path: "/ai/conversation/resume",
			body: { sessionId: args.sessionId },
		}),
	},
	approve_conversation_action: {
		map: (args) => ({
			method: "POST",
			path: "/ai/conversation/approve",
			body: { sessionId: args.sessionId, approved: args.approved },
		}),
	},
	get_session_knowledge: {
		map: (_args, p) => ({
			method: "GET",
			path: `/ai/session-knowledge?sessionId=${p("sessionId")}`,
		}),
	},
	toggle_ai_suggestions: {
		map: (args) => ({
			method: "POST",
			path: "/ai/suggestions/toggle",
			body: { sessionId: args.sessionId },
		}),
	},
	list_knowledge_sessions: {
		map: (args) => ({
			method: "POST",
			path: "/ai/knowledge/sessions",
			body: { filter: args.filter, limit: args.limit },
		}),
	},
	get_knowledge_session_detail: {
		map: (_args, p) => ({
			method: "GET",
			path: `/ai/knowledge/session?sessionId=${p("sessionId")}`,
		}),
	},
	load_scheduler_config: {
		map: () => ({ method: "GET", path: "/ai/scheduler/config" }),
	},
	save_scheduler_config: {
		map: (args) => ({ method: "PUT", path: "/ai/scheduler/config", body: args.config }),
	},
	// Diff triage (event-bridge plan Step 2): trigger over HTTP; progress
	// frames arrive over the `/events` SSE bridge as "triage-progress".
	run_diff_triage: {
		map: (args) => ({
			method: "POST",
			path: "/ai/triage/run",
			body: { repoPath: args.repoPath, refresh: args.refresh },
		}),
	},
	run_pr_review: {
		map: (args) => ({
			method: "POST",
			path: "/ai/review/pr",
			body: { repoPath: args.repoPath, prNumber: args.prNumber },
		}),
	},
	run_improvement_scan: {
		map: (args) => ({
			method: "POST",
			path: "/ai/improvements/scan",
			body: { repoPath: args.repoPath, focus: args.focus },
		}),
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
	get_worktree_paths: {
		map: (_args, p) => ({ method: "GET", path: `/worktrees/paths?path=${p("repoPath")}` }),
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
			const overrideBusy = args.overrideBusy === true ? "&overrideBusy=true" : "";
			return {
				method: "DELETE",
				path: `/worktrees/${p("workspaceId")}?repoPath=${p("repoPath")}&deleteBranch=${args.deleteBranch ?? true}${force}${overrideBusy}`,
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
	remove_orphan_worktree: {
		map: (args) => ({
			method: "POST",
			path: "/repo/remove-orphan",
			body: { repoPath: args.repoPath, worktreePath: args.worktreePath },
			transform: (data) => (data as { archivePath: string }).archivePath,
		}),
	},
	delete_orphan_worktree: {
		map: (args) => ({
			method: "POST",
			path: "/repo/delete-orphan",
			body: { repoPath: args.repoPath, worktreePath: args.worktreePath },
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
	execute_api_prompt: {
		map: (args) => ({
			method: "POST",
			path: "/prompt/execute-api",
			body: {
				systemPrompt: args.systemPrompt,
				content: args.content,
				timeoutMs: args.timeoutMs,
			},
		}),
	},

	// --- Agents ---
	verify_agent_session: {
		map: (args) => ({
			method: "POST",
			path: "/agents/verify-session",
			body: { agentType: args.agentType, sessionId: args.sessionId, cwd: args.cwd },
		}),
	},
	detect_agents: { map: () => ({ method: "GET", path: "/agents" }) },
	detect_all_agent_binaries: {
		map: (args) => ({ method: "POST", path: "/agents/detect-all", body: { binaries: args.binaries } }),
	},
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
	spawn_agent: {
		map: (args) => {
			const ptyConfig = isRecord(args.pty_config) ? args.pty_config : {};
			const agentConfig = isRecord(args.agent_config) ? args.agent_config : {};
			return {
				method: "POST",
				path: "/sessions/agent",
				body: { ...ptyConfig, ...agentConfig },
				transform: (data) => {
					if (isRecord(data) && typeof data.session_id === "string") return data.session_id;
					throw new Error("spawn_agent HTTP response missing session_id");
				},
			};
		},
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

	// --- File browser ---
	list_directory: {
		map: (_args, p) => ({ method: "GET", path: `/fs/list?repoPath=${p("repoPath")}&subdir=${p("subdir")}` }),
	},
	search_files: {
		map: (args, p) => {
			let path = `/fs/search?repoPath=${p("repoPath")}&query=${p("query")}`;
			if (args.limit != null) path += `&limit=${encodeURIComponent(String(args.limit))}`;
			return { method: "GET", path };
		},
	},
	fs_read_file: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read?repoPath=${p("repoPath")}&file=${p("file")}` }),
	},
	read_editor_file: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read-editor?repoPath=${p("repoPath")}&file=${p("file")}` }),
	},
	read_external_file: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read-external?path=${p("path")}` }),
	},
	read_editor_file_external: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read-editor-external?path=${p("path")}` }),
	},
	write_file: {
		map: (args) => ({
			method: "POST",
			path: "/fs/write",
			body: { repoPath: args.repoPath, file: args.file, content: args.content },
		}),
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
	// Returns Option<ResolvedFilePath>: a miss serializes to JSON null, so the
	// transform passes null straight through (no empty-body error).
	resolve_terminal_path: {
		map: (_args, p) => ({
			method: "GET",
			path: `/fs/resolve-terminal-path?cwd=${p("cwd")}&candidate=${p("candidate")}`,
			transform: (data) => data ?? null,
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
	stat_path: {
		map: (_args, p) => ({ method: "GET", path: `/fs/stat?path=${p("path")}` }),
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
		map: (args) => ({ method: "PUT", path: "/config/remote-connections", body: args.connection }),
	},
	delete_remote_connection: {
		map: (_args, p) => ({ method: "DELETE", path: `/config/remote-connections/${p("id")}` }),
	},
	// Test Connection (story: SSH Tunnels + Remote Servers consolidation, Phase 2).
	// `args.request` is the whole `{ transport, auth_username, password }` shape —
	// mirrors the Rust `test_connection(request: TestConnectionRequest)` command's
	// single named parameter, same convention as `save_remote_connection` passing
	// `args.connection` straight through as the body.
	test_connection: {
		map: (args) => ({ method: "POST", path: "/config/remote-connections/test", body: args.request }),
	},
	// Remote connection password (keyring-proxied) — plan Phase 3 auth wiring.
	remote_connection_password_exists: {
		map: (_args, p) => ({ method: "GET", path: `/config/remote-connections/${p("id")}/password/exists` }),
	},
	save_remote_connection_password: {
		map: (args, p) => ({
			method: "POST",
			path: `/config/remote-connections/${p("id")}/password`,
			body: { password: args.password },
		}),
	},
	delete_remote_connection_password: {
		map: (_args, p) => ({ method: "DELETE", path: `/config/remote-connections/${p("id")}/password` }),
	},
	// Direct connection TLS proxy — plan Phase 4 ("Self-signed HTTPS for Direct").
	probe_direct_tls_connection: {
		map: (args) => ({
			method: "POST",
			path: "/config/remote-connections/probe-direct-tls",
			body: { url: args.url, tls_fingerprint: args.tlsFingerprint ?? null },
		}),
	},
	start_direct_proxy: {
		map: (args, p) => ({
			method: "POST",
			path: `/config/remote-connections/${p("connectionId")}/direct-proxy`,
			body: {
				url: args.url,
				tls_fingerprint: args.tlsFingerprint ?? null,
				use_native_roots: args.useNativeRoots ?? false,
			},
		}),
	},
	stop_direct_proxy: {
		map: (_args, p) => ({ method: "DELETE", path: `/config/remote-connections/${p("connectionId")}/direct-proxy` }),
	},
	// Local transport's Connect flow — resolve a named instance's port off disk.
	get_local_instance_port: {
		map: (_args, p) => ({
			method: "GET",
			path: `/config/remote-connections/local-instance-port/${p("instanceId")}`,
		}),
	},
	// SSH remote daemon provisioning (Phase 5).
	configure_ssh_daemon_password: {
		map: (_args, p) => ({
			method: "POST",
			path: `/config/remote-connections/${p("connectionId")}/configure-ssh-password`,
		}),
	},
	probe_ssh_daemon: {
		map: (args) => ({ method: "POST", path: "/config/ssh-daemon/probe", body: { ssh: args.ssh, port: args.port } }),
	},
	install_ssh_daemon: {
		map: (args) => ({ method: "POST", path: "/config/ssh-daemon/install", body: { ssh: args.ssh } }),
	},
	start_ssh_remote_daemon: {
		map: (args) => ({
			method: "POST",
			path: "/config/ssh-daemon/start",
			body: { ssh: args.ssh, instance_id: args.instanceId ?? null, port: args.port },
		}),
	},
	stop_ssh_remote_daemon: {
		map: (args) => ({ method: "POST", path: "/config/ssh-daemon/stop", body: { ssh: args.ssh, port: args.port } }),
	},
	set_ssh_remote_password: {
		map: (args) => ({
			method: "POST",
			path: "/config/ssh-daemon/set-password",
			body: {
				ssh: args.ssh,
				instance_id: args.instanceId ?? null,
				username: args.username,
				password: args.password,
			},
		}),
	},
	check_remote_version: {
		map: (args) => ({
			method: "POST",
			path: "/config/ssh-daemon/check-version",
			body: { local_version: args.localVersion, remote_version: args.remoteVersion },
		}),
	},

	// --- Tunnels ---
	list_tunnel_profiles: { map: () => ({ method: "GET", path: "/tunnels/profiles" }) },
	save_tunnel_profile: { map: (args) => ({ method: "POST", path: "/tunnels/profiles", body: args.profile }) },
	delete_tunnel_profile: { map: (args) => ({ method: "DELETE", path: `/tunnels/profiles/${args.id}` }) },
	start_tunnel: { map: (args) => ({ method: "POST", path: `/tunnels/start/${args.id}` }) },
	stop_tunnel: { map: (args) => ({ method: "POST", path: `/tunnels/stop/${args.id}` }) },
	list_active_tunnels: { map: () => ({ method: "GET", path: "/tunnels/active" }) },
	get_tunnel_status: { map: (args) => ({ method: "GET", path: `/tunnels/status/${args.id}` }) },
	get_tunnel_audit: { map: (args) => ({ method: "GET", path: `/tunnels/audit/${args.id}?limit=${args.limit || 20}` }) },
	list_ssh_config_hosts: { map: () => ({ method: "GET", path: "/tunnels/ssh-hosts" }) },
	list_ssh_agent_keys: { map: () => ({ method: "GET", path: "/tunnels/agent-keys" }) },
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
