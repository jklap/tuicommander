/**
 * Transport abstraction layer — auto-detects Tauri IPC vs HTTP/WebSocket.
 *
 * In Tauri mode: uses invoke() for RPC, listen() for events.
 * In browser mode: uses fetch() for RPC, WebSocket for PTY streaming.
 */

import type { LogLine } from "./mobile/utils/logLine";
import { getRemoteBaseUrl, previewLogPayload, transportLogger } from "./transportRuntime";

// ---------------------------------------------------------------------------
// MCP upstream config types (mirrors Rust structs in mcp_upstream_config.rs)
// ---------------------------------------------------------------------------

export type UpstreamTransport =
	| { type: "http"; url: string }
	| { type: "stdio"; command: string; args?: string[]; env?: Record<string, string>; cwd?: string };

export type FilterMode = "allow" | "deny";

export interface ToolFilter {
	mode: FilterMode;
	patterns: string[];
}

export type UpstreamAuth =
	| { type: "bearer"; token: string }
	| {
			type: "oauth2";
			client_id: string;
			client_secret?: string;
			scopes?: string[];
			authorization_endpoint?: string;
			token_endpoint?: string;
	  };

export interface UpstreamMcpServer {
	id: string;
	name: string;
	transport: UpstreamTransport;
	enabled: boolean;
	timeout_secs: number;
	tool_filter?: ToolFilter;
	auth?: UpstreamAuth;
}

export interface UpstreamMcpConfig {
	servers: UpstreamMcpServer[];
}

export interface UpstreamMcpSaveRequest {
	base: UpstreamMcpConfig;
	config: UpstreamMcpConfig;
}

// ---------------------------------------------------------------------------

/** Detect whether we're running inside a Tauri webview */
export function isTauri(): boolean {
	return "__TAURI_INTERNALS__" in globalThis && !(globalThis as Record<string, unknown>).__TAURI_SHIM__;
}

/** HTTP method + path mapping for a Tauri command */
export interface HttpMapping {
	method: "GET" | "POST" | "PUT" | "DELETE";
	path: string;
	body?: unknown;
	/** Transform the HTTP response before returning (e.g. for can_spawn_session) */
	transform?: (data: unknown) => unknown;
	/**
	 * Treat an HTTP 404 as a successful `null` result instead of throwing.
	 * Bridges Tauri commands whose contract is `Option<T>` (None → null) onto
	 * REST routes that signal "not found" with 404 (e.g. read_plugin_data).
	 */
	notFoundAsNull?: boolean;
}

/** Helper to encode a required argument for URL path/query usage */
function encodeArg(command: string, args: Record<string, unknown>, key: string): string {
	const val = args[key];
	if (val === undefined || val === null) {
		throw new Error(`mapCommandToHttp(${command}): missing required argument "${key}"`);
	}
	return encodeURIComponent(String(val));
}

/** Exported for transportExtended.ts's moved entries — see its module doc. */
export function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Args accessor + URL encoder, bound to a specific command invocation */
type ArgEncoder = (key: string) => string;

/** A command table entry: a mapper function that builds the HTTP request */
export type CommandTableEntry = { map: (args: Record<string, unknown>, p: ArgEncoder) => HttpMapping };

/**
 * Table-driven mapping from Tauri command names to HTTP method/path/body.
 *
 * The `p` helper encodes a required argument for URL usage (throws if missing).
 */
const COMMAND_TABLE: Record<string, CommandTableEntry> = {
	// --- OS integration ---
	open_in_app: {
		map: (args) => ({
			method: "POST",
			path: "/agents/open-in-app",
			body: { path: args.path, app: args.app, line: args.line, col: args.col },
		}),
	},
	// --- Native audio ---
	play_notification_sound: {
		map: (args) => ({
			method: "POST",
			path: "/system/notification-sound",
			// `device` rides along: notifications.ts always sends it, so dropping it
			// here would play every browser-mode sound on the default output.
			// `choice` rides along too: notifications.ts always sends it, so dropping
			// it here would ignore per-sound preset/custom-file selection in browser mode.
			body: { sound: args.sound, volume: args.volume, device: args.device ?? null, choice: args.choice ?? null },
		}),
	},
	// --- Relay ---
	get_relay_status: { map: () => ({ method: "GET", path: "/system/relay-status" }) },
	// --- Update channel ---
	check_update_channel: {
		map: (_args, p) => ({ method: "GET", path: `/system/check-update?channel=${p("channel")}` }),
	},

	// --- Session lifecycle ---
	create_pty: {
		map: (args) => ({
			method: "POST",
			path: "/sessions",
			body: args.config as Record<string, unknown>,
			transform: (data: unknown) => (data as { session_id: string }).session_id,
		}),
	},
	create_pty_with_worktree: {
		// Browser path: createSessionWithWorktree sends { pty_config, worktree_config };
		// flatten worktree_config into the HTTP route's { config, base_repo, branch_name }.
		map: (args) => {
			const wt = (args.worktree_config ?? {}) as { task_name?: string; base_repo?: string; branch?: string | null };
			return {
				method: "POST",
				path: "/sessions/worktree",
				body: {
					config: args.pty_config,
					base_repo: wt.base_repo,
					branch_name: wt.branch ?? wt.task_name,
				},
			};
		},
	},
	write_pty: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId ?? args.id}/write`,
			body: { data: args.data },
		}),
	},
	// Not `write_pty` with the parts joined: the backend runs its per-input
	// bookkeeping once per PART, and that bookkeeping is not a function of the
	// concatenated bytes — a lone "/" opens slash mode, and an exact option key
	// clears a choice prompt. Joining silently changes what the user typed into
	// something the backend reads differently.
	write_pty_parts: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId ?? args.id}/write-parts`,
			body: { parts: args.parts },
		}),
	},
	enqueue_agent_command: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/queue`,
			body: { text: args.text },
		}),
	},
	clear_queued_agent_commands: {
		map: (args) => ({ method: "DELETE", path: `/sessions/${args.sessionId}/queue` }),
	},
	list_queued_agent_commands: {
		map: (args) => ({ method: "GET", path: `/sessions/${args.sessionId}/queue` }),
	},
	remove_queued_agent_command: {
		map: (args) => ({ method: "DELETE", path: `/sessions/${args.sessionId}/queue/${args.commandId}` }),
	},
	set_session_name: {
		map: (args) => ({
			method: "PUT",
			path: `/sessions/${args.sessionId}/name`,
			body: { name: args.name, isCustom: args.isCustom },
		}),
	},
	set_session_accent_color: {
		map: (args) => ({
			method: "PUT",
			path: `/sessions/${args.sessionId}/accent-color`,
			body: { color: args.color },
		}),
	},
	resize_pty: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/resize`,
			body: {
				rows: args.rows,
				cols: args.cols,
				cell_width_px: args.cellWidthPx,
				cell_height_px: args.cellHeightPx,
			},
		}),
	},
	pause_pty: {
		map: (args) => ({ method: "POST", path: `/sessions/${args.sessionId}/pause` }),
	},
	resume_pty: {
		map: (args) => ({ method: "POST", path: `/sessions/${args.sessionId}/resume` }),
	},
	get_kitty_flags: {
		map: (args) => ({ method: "GET", path: `/sessions/${args.sessionId}/kitty-flags` }),
	},
	close_pty: {
		map: (args) => ({ method: "DELETE", path: `/sessions/${args.sessionId}` }),
	},
	// Answering a blocked agent has to work from a browser or phone — that is the
	// whole reason the confirmation stopped being a native desktop dialog.
	mcp_confirm_response: {
		map: (args) => ({
			method: "POST",
			path: "/mcp/confirm-response",
			body: { request_id: args.requestId, confirmed: args.confirmed },
		}),
	},
	// Same reasoning as mcp_confirm_response: a client away from the desktop
	// must still be able to answer this.
	agent_wrap_prompt_response: {
		map: (args) => ({
			method: "POST",
			path: "/agent-wrap-prompt/response",
			body: {
				request_id: args.requestId,
				agent_type: args.agentType,
				decision: args.decision,
			},
		}),
	},
	get_session_foreground_process: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/foreground`,
			transform: (data) => (data as { agent: string | null }).agent,
		}),
	},
	explain_session_state: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/explain-state`,
		}),
	},
	get_pty_capture: {
		map: () => ({ method: "GET", path: "/diagnostics/capture" }),
	},
	set_pty_capture: {
		map: (args) => ({
			method: "POST",
			path: "/diagnostics/capture",
			body: { enabled: args.enabled, session_id: args.sessionId ?? null },
		}),
	},
	get_session_shell_family: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/shell-family`,
		}),
	},
	get_shell_state: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/shell-state`,
			transform: (data) => (data as { state: string | null }).state ?? null,
		}),
	},
	get_last_prompt: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/last-prompt`,
			transform: (data) => (data as { prompt: string | null }).prompt ?? null,
		}),
	},
	get_input_buffer_content: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/input-buffer`,
			transform: (data) => (data as { content: string }).content,
		}),
	},
	get_session_leaf_pid: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/leaf-pid`,
			transform: (data) => (data as { pid: number | null }).pid ?? null,
		}),
	},
	has_foreground_process: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/has-foreground`,
			transform: (data) => (data as { process: string | null }).process ?? null,
		}),
	},
	set_session_visible: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/visible`,
			body: { visible: args.visible, viewer_id: args.viewerId },
		}),
	},
	focus_session: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/focus`,
		}),
	},
	run_ui_action: {
		map: (args) => ({
			method: "POST",
			path: `/ui/action`,
			body: { name: args.name },
		}),
	},
	streamdock_status: {
		map: () => ({
			method: "GET",
			path: `/streamdock/status`,
		}),
	},
	streamdock_list_devices: {
		map: () => ({
			method: "GET",
			path: `/streamdock/devices`,
		}),
	},
	list_active_sessions: { map: () => ({ method: "GET", path: "/sessions" }) },

	// --- Config: app ---
	load_config: { map: () => ({ method: "GET", path: "/config" }) },
	save_config: { map: (args) => ({ method: "PUT", path: "/config", body: args.config }) },
	load_app_config: { map: () => ({ method: "GET", path: "/config" }) },
	save_app_config: { map: (args) => ({ method: "PUT", path: "/config", body: args.config }) },
	hash_password: {
		map: (args) => ({
			method: "POST",
			path: "/config/hash-password",
			body: { password: args.password },
			transform: (data) => (data as { hash: string }).hash,
		}),
	},

	// --- Config: notifications ---
	load_notification_config: { map: () => ({ method: "GET", path: "/config/notifications" }) },
	save_notification_config: {
		map: (args) => ({ method: "PUT", path: "/config/notifications", body: args.config }),
	},

	// --- Config: UI prefs ---
	load_ui_prefs: { map: () => ({ method: "GET", path: "/config/ui-prefs" }) },
	save_ui_prefs: {
		map: (args) => ({ method: "PUT", path: "/config/ui-prefs", body: args.config }),
	},

	// --- Config: repo settings ---
	load_repo_settings: { map: () => ({ method: "GET", path: "/config/repo-settings" }) },
	save_repo_settings: {
		map: (args) => ({ method: "PUT", path: "/config/repo-settings", body: args.config }),
	},
	check_has_custom_settings: {
		map: (_args, p) => ({ method: "GET", path: `/config/repo-settings/has-custom?path=${p("path")}` }),
	},
	load_repo_defaults: { map: () => ({ method: "GET", path: "/config/repo-defaults" }) },
	save_repo_defaults: {
		map: (args) => ({ method: "PUT", path: "/config/repo-defaults", body: args.config }),
	},

	// --- Config: repositories ---
	load_repositories: { map: () => ({ method: "GET", path: "/config/repositories" }) },
	save_repositories: {
		map: (args) => ({ method: "PUT", path: "/config/repositories", body: args.config }),
	},
	list_stale_temp_repository_candidates: {
		map: () => ({ method: "GET", path: "/config/repositories/stale-temp" }),
	},
	repair_stale_temp_repositories: {
		map: (args) => ({ method: "POST", path: "/config/repositories/stale-temp", body: { paths: args.paths } }),
	},

	// --- Config: pane layout ---
	load_pane_layout: { map: () => ({ method: "GET", path: "/config/pane-layout" }) },
	save_pane_layout: {
		map: (args) => ({ method: "PUT", path: "/config/pane-layout", body: args.layout }),
	},

	// --- Config: caches ---
	clear_caches: { map: () => ({ method: "POST", path: "/config/clear-caches" }) },
	clear_repo_caches: { map: (a) => ({ method: "POST", path: `/config/clear-repo-caches`, body: { path: a.path } }) },

	// --- Config: scrollback restore ---
	clear_saved_scrollback: {
		map: (a) => ({ method: "DELETE", path: "/scrollback", body: { session: a.session ?? null } }),
	},

	// --- Config: repo local config (.tuic.json) ---
	load_repo_local_config: {
		map: (_args, p) => ({ method: "GET", path: `/config/repo-local-config?path=${p("repoPath")}` }),
	},

	// --- Project Progress ---
	report_progress_event: {
		map: (args, p) => ({
			method: "POST",
			path: `/progress/report?path=${p("project")}`,
			body: args.report,
		}),
	},
	progress_status: { map: (_args, p) => ({ method: "GET", path: `/progress/status?path=${p("project")}` }) },
	progress_list: {
		map: (args, p) => ({ method: "POST", path: `/progress/list?path=${p("project")}`, body: args.input }),
	},
	progress_pause: { map: (_args, p) => ({ method: "POST", path: `/progress/pause?path=${p("project")}` }) },
	progress_resume: { map: (_args, p) => ({ method: "POST", path: `/progress/resume?path=${p("project")}` }) },
	progress_delete: {
		map: (args, p) => ({ method: "POST", path: `/progress/delete?path=${p("project")}`, body: args.input }),
	},
	progress_clear: {
		map: (args, p) => ({ method: "POST", path: `/progress/clear?path=${p("project")}`, body: args.input }),
	},
	progress_update: {
		map: (args, p) => ({ method: "POST", path: `/progress/update?path=${p("project")}`, body: args.input }),
	},
	progress_read: {
		map: (args, p) => ({ method: "POST", path: `/progress/read?path=${p("project")}`, body: args.input }),
	},
	progress_export: {
		map: (args, p) => ({ method: "POST", path: `/progress/export?path=${p("project")}`, body: args.input }),
	},

	// --- Config: prompt library ---
	load_prompt_library: { map: () => ({ method: "GET", path: "/config/prompt-library" }) },
	save_prompt_library: {
		map: (args) => ({ method: "PUT", path: "/config/prompt-library", body: args.config }),
	},

	// --- Config: activity ---
	load_activity: { map: () => ({ method: "GET", path: "/config/activity" }) },
	save_activity: {
		map: (args) => ({ method: "PUT", path: "/config/activity", body: args.items ?? args }),
	},

	// --- Config: keybindings ---
	load_keybindings: { map: () => ({ method: "GET", path: "/config/keybindings" }) },
	save_keybindings: {
		map: (args) => ({ method: "PUT", path: "/config/keybindings", body: args.config }),
	},

	// --- Config: agents ---
	load_agents_config: { map: () => ({ method: "GET", path: "/config/agents" }) },
	save_agents_config: {
		map: (args) => ({ method: "PUT", path: "/config/agents", body: args.config }),
	},
	// Hook instrumentation toggle: GET returns {state}, the Tauri command returns the
	// bare AgentHookState string — unwrap it. PUT's {ok:true} is discarded by callers.
	get_agent_hook_state: {
		map: (_args, p) => ({
			method: "GET",
			path: `/config/agents/${p("agentType")}/hook-instrumentation`,
			transform: (data) => (data as { state: string }).state,
		}),
	},
	set_agent_hook_instrumentation: {
		map: (args, p) => ({
			method: "PUT",
			path: `/config/agents/${p("agentType")}/hook-instrumentation`,
			body: { enabled: args.enabled },
		}),
	},
	get_agent_native_status_signals: {
		map: (_args, p) => ({
			method: "GET",
			path: `/config/agents/${p("agentType")}/native-status-signals`,
			transform: (data) => (data as { enabled: boolean }).enabled,
		}),
	},
	set_agent_native_status_signals: {
		map: (args, p) => ({
			method: "PUT",
			path: `/config/agents/${p("agentType")}/native-status-signals`,
			body: { enabled: args.enabled },
		}),
	},
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

	// --- Plugin data ---
	// Tauri contract is Option<String>: missing key → null. The route 404s on miss,
	// which notFoundAsNull bridges back to null. Found content is returned as a string
	// to match the command's String payload (the route may sniff JSON and parse it).
	read_plugin_data: {
		map: (_args, p) => ({
			method: "GET",
			path: `/api/plugins/${p("pluginId")}/data/${p("path")}`,
			notFoundAsNull: true,
			transform: (data) => (data == null ? null : typeof data === "string" ? data : JSON.stringify(data)),
		}),
	},
	// write_plugin_data: POST to the same path; content travels in the body. Fixes the
	// browser-mode credential-consent flow (pluginRegistry.ts) which threw before this.
	// delete_plugin_data has no frontend caller, so it is intentionally not mapped.
	write_plugin_data: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/data/${p("path")}`,
			body: { content: args.content },
		}),
	},
	get_git_diff: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/diff?path=${p("path")}`,
			transform: (data) => (data as { diff: string }).diff,
		}),
	},
	set_branch_label: {
		map: (args) => ({
			method: "POST",
			path: "/config/branch-label",
			body: { repoPath: args.repoPath, branchName: args.branchName, label: args.label },
		}),
	},
	delete_note_assets: {
		map: (args) => ({
			method: "POST",
			path: "/config/note-assets/delete",
			body: { noteId: args.noteId },
		}),
	},
	delete_note_assets_batch: {
		map: (args) => ({
			method: "POST",
			path: "/config/note-assets/delete-batch",
			body: { noteIds: args.noteIds },
		}),
	},
	claude_project_dir: {
		map: (args) => ({
			method: "POST",
			path: "/agent/claude-project-dir",
			body: { cwd: args.cwd, claudeConfigDir: args.claudeConfigDir },
		}),
	},
	github_poll_repo: {
		map: (args) => ({
			method: "POST",
			path: "/repo/github-poller/poll-repo",
			body: { path: args.path },
		}),
	},
	github_update_paths: {
		map: (args) => ({
			method: "POST",
			path: "/repo/github-poller/update-paths",
			body: { paths: args.paths },
		}),
	},
	get_git_branches: {
		map: (_args, p) => ({ method: "GET", path: `/repo/branches?path=${p("path")}` }),
	},

	// --- File operations ---
	list_markdown_files: {
		map: (_args, p) => ({ method: "GET", path: `/repo/markdown-files?path=${p("path")}` }),
	},
	read_file: {
		map: (_args, p) => ({ method: "GET", path: `/repo/file?path=${p("path")}&file=${p("file")}` }),
	},

	// --- Watchers ---
	start_repo_watcher: {
		map: (_args, p) => ({ method: "POST", path: `/watchers/repo?path=${p("repoPath")}` }),
	},
	stop_repo_watcher: {
		map: (_args, p) => ({ method: "DELETE", path: `/watchers/repo?path=${p("repoPath")}` }),
	},
	set_hot_repos: {
		map: (args) => ({ method: "PUT", path: "/watchers/hot-repos", body: args }),
	},
	warm_content_index: {
		map: (args) => ({ method: "POST", path: "/fs/warm-index", body: { repoPath: args.repoPath } }),
	},

	// --- Notes ---
	load_notes: { map: () => ({ method: "GET", path: "/config/notes" }) },
	save_notes: { map: (args) => ({ method: "PUT", path: "/config/notes", body: args.config }) },

	// --- Recent commits ---
	get_recent_commits: {
		map: (args, p) => ({
			method: "GET",
			path: `/repo/recent-commits?path=${p("path")}&count=${args.count ?? 5}`,
		}),
	},

	// --- Plugins ---
	list_user_plugins: { map: () => ({ method: "GET", path: "/plugins/list" }) },

	// --- App Logger ---
	push_log: {
		map: (args) => ({
			method: "POST",
			path: "/logs",
			body: {
				level: args.level,
				source: args.source,
				message: args.message,
				data_json: args.dataJson,
				audience: args.audience,
			},
		}),
	},
	get_logs: {
		map: (args) => ({ method: "GET", path: `/logs?limit=${args.limit ?? 0}` }),
	},
	clear_logs: { map: () => ({ method: "DELETE", path: "/logs" }) },

	// --- Story 071: Plugin RPC commands ---
	plugin_read_file: {
		map: (_args, p) => ({
			method: "GET",
			path: `/api/plugins/${p("pluginId")}/fs/read?path=${p("path")}`,
		}),
	},
	plugin_read_files: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/fs/read-batch`,
			body: { paths: args.paths },
		}),
	},
	plugin_read_file_base64: {
		map: (_args, p) => ({
			method: "GET",
			path: `/api/plugins/${p("pluginId")}/fs/read-base64?path=${p("path")}`,
		}),
	},
	plugin_read_file_tail: {
		map: (_args, p) => ({
			method: "GET",
			path: `/api/plugins/${p("pluginId")}/fs/tail?path=${p("path")}&maxBytes=${p("maxBytes")}`,
		}),
	},
	plugin_list_directory: {
		map: (args, p) => {
			let path = `/api/plugins/${p("pluginId")}/fs/list?path=${p("path")}`;
			if (args.pattern != null) path += `&pattern=${encodeURIComponent(String(args.pattern))}`;
			if (args.sortBy != null) path += `&sortBy=${encodeURIComponent(String(args.sortBy))}`;
			return { method: "GET", path };
		},
	},
	plugin_write_file: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/fs/write`,
			body: { path: args.path, content: args.content },
		}),
	},
	plugin_rename_path: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/fs/rename`,
			body: { from: args.from, to: args.to },
		}),
	},
	scan_build_artifacts: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/build-artifacts/scan`,
			body: {
				repoPaths: args.repoPaths,
				...(args.forceRefresh ? { forceRefresh: true } : {}),
			},
		}),
	},
	delete_build_artifact: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/build-artifacts/delete`,
			body: { path: args.path, repoPaths: args.repoPaths },
		}),
	},
	// Same body as delete; removes only the artifact's regenerable intermediates.
	trim_build_artifact: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/build-artifacts/trim`,
			body: { path: args.path, repoPaths: args.repoPaths },
		}),
	},
	plugin_exec_cli: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/exec`,
			body: { binary: args.binary, args: args.args, cwd: args.cwd },
		}),
	},
	plugin_http_fetch: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/http`,
			body: {
				url: args.url,
				method: args.method,
				headers: args.headers,
				body: args.body,
			},
		}),
	},
	plugin_read_session_output: {
		map: (args, p) => {
			let path = `/api/plugins/${p("pluginId")}/pty/output?sessionId=${p("sessionId")}`;
			if (args.maxLines != null) path += `&maxLines=${encodeURIComponent(String(args.maxLines))}`;
			return { method: "GET", path };
		},
	},
	register_loaded_plugin: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/register`,
			body: { capabilities: args.capabilities },
		}),
	},
	unregister_loaded_plugin: {
		map: (_args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/unregister`,
		}),
	},
	set_plugin_output_watchers: {
		map: (args) => ({
			method: "POST",
			path: "/api/plugins/output-watchers",
			body: { client_id: args.clientId, seq: args.seq, watchers: args.watchers },
		}),
	},
};

/**
 * Merge additional command mappings into the live COMMAND_TABLE.
 *
 * The only caller is transportExtended.ts's own module-level side effect,
 * imported solely by the desktop entry (src/index.tsx) — see that file's doc
 * comment for why the split exists and why mobile must never import it.
 */
export function registerCommandTableEntries(entries: Record<string, CommandTableEntry>): void {
	Object.assign(COMMAND_TABLE, entries);
}

/**
 * Commands that are deliberately NOT given an HTTP mapping because they are
 * native/host-only: they are cfg-gated out of the headless `tuic-remote` build
 * and/or depend on the desktop OS/window/Tauri runtime, so they cannot work from
 * a browser/PWA/remote client. Listing them here (story 073) documents the intent,
 * lets `mapCommandToHttp` raise a precise error instead of a generic "no mapping",
 * and gives any future mapping-coverage audit an explicit allowlist to skip.
 *
 * This is NOT a feature gap — these commands have no meaning off the host machine.
 */
export const INTENTIONALLY_UNMAPPED: ReadonlySet<string> = new Set<string>([
	// Multi-window management — secondary/panel windows are a desktop-only concept.
	"open_secondary_window",
	"open_panel_window",
	"close_panel_window",
	"focus_panel_window",
	"focus_main_window",
	// Native drag-and-drop (WKWebView/OS drag) — no browser equivalent.
	"start_native_drag",
	// Power management — OS sleep assertions only make sense on the host.
	"block_sleep",
	"unblock_sleep",
	// Global hotkey registration — OS-level, host-only.
	"set_global_hotkey",
	// Microphone permission — OS permission dialogs, host-only.
	"check_microphone_permission",
	"open_microphone_settings",
	// Screenshot capture response — driven by the native screenshot pipeline.
	"screenshot_response",
	// Connectivity/host identity — these describe the host server itself; a remote
	// client asking the server for its own connect URL / rotating its token / reading
	// Tailscale state is a host-administration action, not a browser feature.
	"get_connect_url",
	"regenerate_session_token",
	"get_tailscale_status",
	"recheck_tailscale_status",
	"get_self_signed_cert_status",
	"regenerate_self_signed_cert",
	// Deep-link / OAuth callback entry points — invoked by the OS URL handler, not UI.
	"deep_link_mcp_call",
	"mcp_oauth_callback",
	// MCP upstream OAuth (story 072): start spawns a desktop-loopback-bound callback
	// server + relies on the desktop browser-opener; the redirect target isn't reachable
	// from a generic browser/remote context. Desktop drives it over IPC; browser gets a
	// clean host-only error. cancel pairs with start, so it's host-only too.
	"start_mcp_upstream_oauth",
	"cancel_mcp_upstream_oauth",
	// CLI install/management — mutates the host PATH / shell integration.
	"install_cli",
	"uninstall_cli",
	"dismiss_cli_prompt",
	"get_cli_status",
	// Finder Service (macOS "New TUICommander Tab Here") install/management —
	// registers a local Finder right-click menu item. Meaningless for a
	// browser/remote client, which has no local Finder to register one for.
	"get_finder_service_status",
	"install_finder_service",
	"uninstall_finder_service",
	"dismiss_finder_service_prompt",
	// App version bookkeeping — desktop updater state.
	"get_last_seen_version",
	"set_last_seen_version",
	// mdkb daemon install/management — host binary lifecycle.
	"install_mdkb",
	"uninstall_mdkb",
	// Terminal grid push — browser uses the WS log-mode stream
	// (GET /sessions/{id}?format=log) instead of the native grid-frame protocol.
	"subscribe_terminal_grid",
	"unsubscribe_terminal_grid",
	"ack_terminal_frame",
	"terminal_exit_alt_screen",
	"read_vt_log",
	"terminal_get_block_rows",
	// WebView liveness beat. Deliberately host-only: it watches the embedded
	// WebView's main thread, and a browser client beating on the same channel
	// would mask a dead desktop one.
	"frontend_heartbeat",
	// Desktop-only terminal/session diagnostics or local visual state with no
	// faithful HTTP contract yet.
	"debug_agent_detection",
	"set_ansi_colors",
	// AI high-frequency streams are bridged by dedicated WebSockets:
	// /ai/conversation/{session_id}/stream and /ai/chat/{chat_id}/stream.
	"start_conversation",
	"chat_subscribe",
	"chat_unsubscribe",
	// Agent loop/suggestion state has partial HTTP control today, but these IPC
	// reads do not yet have byte-identical HTTP response routes.
	"agent_loop_status",
	"get_ai_suggestions_enabled",
	// GitHub issues: Tauri command is multi-repo; current HTTP route is single-repo
	// (/repo/issues), so mapping here would silently change the contract.
	"get_all_issues",
	// mdkb editor helpers are tied to the desktop-managed daemon/AppState and have
	// no HTTP routes yet.
	"mdkb_outline",
	"mdkb_goto_definition",
	"mdkb_references",
	"mdkb_code_find",
	"mdkb_status",
	// Agent MCP installation mutates local agent config files and is desktop
	// settings-only until a guarded HTTP surface exists.
	"get_agent_mcp_status",
	"install_agent_mcp",
	"remove_agent_mcp",
	"list_installed_mcp_integrations",
	"remove_all_mcp_integrations",
	"get_agent_config_path",
	"get_mcp_bridge_info",
	// Shell-safe prompt processing needs a dedicated HTTP route; mapping it to
	// /prompt/process would lose shell quoting and be a security regression.
	"process_prompt_content_shell_safe",
	// Notes image directory is a local filesystem implementation detail; browser
	// clients use note asset APIs rather than reading this directory path.
	"get_note_images_dir",
	// Plugin filesystem watch — event delivery to plugins needs AppHandle/WS — out of scope.
	"plugin_watch_path",
	"plugin_unwatch",
	// Plugin credential — OS keychain / native security tool.
	"plugin_read_credential",
	// Plugin install/uninstall — take AppHandle; local-FS install/emit.
	"install_plugin_from_zip",
	"install_plugin_from_folder",
	"install_plugin_from_url",
	"uninstall_plugin",
	// Plugin data deletion — no frontend caller.
	"delete_plugin_data",
]);

/**
 * Commands that are available off the host but carry their payload on a
 * dedicated WebSocket rather than on a request/response route.
 *
 * A third class, and a truthful one. These are NOT native/host-only — a
 * browser can and must use them — so listing them as `INTENTIONALLY_UNMAPPED`
 * would claim a feature gap that does not exist. They are also not
 * `COMMAND_TABLE` entries, because there is no single response to return: the
 * command opens a stream and events arrive for as long as it stays open.
 *
 * The value builds the WebSocket path for a given set of command arguments, so
 * the route lives here next to the HTTP ones instead of being spelled again in
 * whatever opens the socket.
 */
export const DEDICATED_WS_COMMANDS: ReadonlyMap<string, (args: Record<string, unknown>) => string> = new Map<
	string,
	(args: Record<string, unknown>) => string
>([
	[
		"acp_subscribe",
		(args) =>
			`/acp/connections/${encodeArg("acp_subscribe", args, "connectionId")}/stream?after=${encodeArg(
				"acp_subscribe",
				args,
				"afterSequence",
			)}`,
	],
]);

/** Map a Tauri invoke command + args to an HTTP method/path/body */
export function mapCommandToHttp(command: string, args: Record<string, unknown>): HttpMapping {
	const entry = COMMAND_TABLE[command];
	if (!entry) {
		if (INTENTIONALLY_UNMAPPED.has(command)) {
			throw new Error(`Command "${command}" is native/host-only and is not available in browser/remote mode.`);
		}
		if (DEDICATED_WS_COMMANDS.has(command)) {
			throw new Error(
				`Command "${command}" streams over a dedicated WebSocket and has no request/response route; open ${DEDICATED_WS_COMMANDS.get(command)?.(args)} instead.`,
			);
		}
		throw new Error(`No HTTP mapping for command: ${command}`);
	}
	const p: ArgEncoder = (key) => encodeArg(command, args, key);
	return entry.map(args, p);
}

/** Build a full URL for HTTP transport using current window origin or a remote baseUrl */
export function buildHttpUrl(path: string, baseUrl?: string): string {
	if (baseUrl) return `${baseUrl}${path}`;
	if (typeof window !== "undefined" && window.location?.origin) {
		return `${window.location.origin}${path}`;
	}
	return path;
}

/**
 * In-flight deduplication for idempotent (GET) RPC calls.
 * Concurrent identical calls share the same Promise — cleared on settle.
 */
const _inflight = new Map<string, Promise<unknown>>();

/** True if the command maps to an HTTP GET (idempotent, safe to deduplicate). */
function isIdempotentRpc(command: string, args: Record<string, unknown>): boolean {
	try {
		return mapCommandToHttp(command, args).method === "GET";
	} catch {
		return false;
	}
}

/**
 * Per-session single-flight queue: one request in flight, and everything that
 * piles up behind it leaves as ONE follow-up request.
 *
 * Two call sites need this, for opposite reasons, which is why `merge` is a
 * parameter rather than baked in:
 *
 * - `write_pty` — parallel HTTP POSTs can arrive out of order and reorder the
 *   letters the user typed, so writes must be chained. Chaining alone capped
 *   typing at one character per round trip, so keystrokes that arrive during a
 *   request accumulate. Nothing may be dropped: `merge` appends.
 * - `resize_pty` — a drag fires one per frame, the backend reflows on the
 *   blocking pool, and two in flight race for the per-session lock. Applied
 *   newest-first they leave the PTY at the OLD size with nothing to correct it.
 *   Only the newest size means anything: `merge` replaces, so an intermediate
 *   size is never in flight and can never land last.
 */
interface CoalescingQueue<P> {
	/** Resolves when the queue, including anything still pending, has drained. */
	tail: Promise<unknown>;
	/** What arrived during the in-flight request, already merged. */
	pending: P | null;
	/** Set once a flush is chained for `pending`, so it is not chained twice. */
	flushChained: boolean;
}

/**
 * @param merge Folds a newly arrived payload into whatever is already waiting.
 *   Receives `null` when nothing is waiting yet.
 */
function coalescedRpc<P>(
	queues: Map<string, CoalescingQueue<P>>,
	queueKey: string,
	payload: P,
	merge: (pending: P | null, next: P) => P,
	send: (payload: P) => Promise<unknown>,
): Promise<unknown> {
	const existing = queues.get(queueKey);
	if (existing) {
		existing.pending = merge(existing.pending, payload);
		if (!existing.flushChained) {
			existing.flushChained = true;
			// Chain on failure too: dropping the batch would reorder the line
			// just as surely as a parallel POST would.
			const flush = () => {
				const batch = existing.pending as P;
				existing.pending = null;
				existing.flushChained = false;
				return send(batch).finally(() => reapDrainedQueue(queues, queueKey, existing));
			};
			existing.tail = existing.tail.then(flush, flush);
		}
		return existing.tail;
	}
	const queue: CoalescingQueue<P> = { tail: Promise.resolve(), pending: null, flushChained: false };
	queues.set(queueKey, queue);
	queue.tail = send(payload).finally(() => reapDrainedQueue(queues, queueKey, queue));
	return queue.tail;
}

/** Forget a queue once nothing is in flight and nothing is waiting behind it. */
function reapDrainedQueue<P>(
	queues: Map<string, CoalescingQueue<P>>,
	queueKey: string,
	queue: CoalescingQueue<P>,
): void {
	if (queues.get(queueKey) === queue && !queue.flushChained) {
		queues.delete(queueKey);
	}
}

const _writeQueues = new Map<string, CoalescingQueue<string[]>>();
const _resizeQueues = new Map<string, CoalescingQueue<{ rows: number; cols: number }>>();

/** Two connections to the same session id are two different backends. */
function queueKeyFor(sessionId: string, connectionId?: string): string {
	return connectionId ? `${connectionId}:${sessionId}` : sessionId;
}

/**
 * RPC call — uses Tauri invoke() or HTTP fetch() based on environment.
 * Concurrent identical idempotent calls are coalesced into a single in-flight request.
 * write_pty calls are serialized per-session in browser mode to prevent reordering.
 * Usage: `const result = await rpc<string>("create_pty", { config });`
 */
export function rpc<T>(command: string, args: Record<string, unknown> = {}, connectionId?: string): Promise<T> {
	// Serialize write_pty per session in browser mode to prevent letter reordering
	if (command === "write_pty" && (!isTauri() || connectionId)) {
		const sessionId = (args.sessionId ?? args.id) as string;
		if (sessionId && typeof args.data === "string") {
			return coalescedRpc(
				_writeQueues,
				// Keyed with the connection: two connections to the same session id
				// are two different backends, and their bytes must not merge.
				queueKeyFor(sessionId, connectionId),
				[args.data],
				(pending, next) => (pending ?? []).concat(next),
				// One round trip either way, but the inputs stay SEPARATE: the
				// backend runs its per-input bookkeeping once per part, and joining
				// them would change what it reads (a lone "/" opens slash mode, an
				// exact option key clears a choice prompt). A solitary keystroke has
				// no batch to describe, so it keeps the single-input route.
				(parts) =>
					parts.length === 1
						? rpcImpl<T>(command, { ...args, data: parts[0] }, connectionId)
						: rpcImpl<T>("write_pty_parts", { sessionId, parts }, connectionId),
			) as Promise<T>;
		}
	}
	// Unlike writes, this is NOT browser-only: `resize_pty` is an async Tauri
	// command, so the desktop hands each call to the blocking pool too and has
	// the same newest-first hazard.
	if (command === "resize_pty") {
		const sessionId = (args.sessionId ?? args.id) as string;
		if (sessionId && typeof args.rows === "number" && typeof args.cols === "number") {
			return coalescedRpc(
				_resizeQueues,
				queueKeyFor(sessionId, connectionId),
				{ rows: args.rows, cols: args.cols },
				(_pending, next) => next,
				(dims) => rpcImpl<T>(command, { ...args, ...dims }, connectionId),
			) as Promise<T>;
		}
	}
	// Desktop talks to the backend over Tauri invoke(), never HTTP, so the
	// GET/POST distinction isIdempotentRpc()/mapCommandToHttp() computes is
	// meaningless here — but every desktop RPC still paid for a COMMAND_TABLE
	// lookup (and a thrown+caught Error for every command not in that table,
	// which is most of them) to work that out. Skip straight to invoke().
	if (!connectionId && isTauri()) {
		return rpcImpl<T>(command, args, connectionId);
	}
	if (isIdempotentRpc(command, args)) {
		const key = connectionId
			? `${connectionId}:${command}:${JSON.stringify(args)}`
			: `${command}:${JSON.stringify(args)}`;
		const existing = _inflight.get(key) as Promise<T> | undefined;
		if (existing) return existing;
		const promise = rpcImpl<T>(command, args, connectionId).finally(() => _inflight.delete(key));
		_inflight.set(key, promise as Promise<unknown>);
		return promise;
	}
	return rpcImpl<T>(command, args, connectionId);
}

/** Cached after the first resolution — `import()` of an already-loaded module
 *  is cheap, but calling it fresh on every desktop RPC (once per keystroke) still
 *  pays a module-registry lookup + Promise wrap that a plain reference avoids. */
let cachedTauriInvoke: typeof import("@tauri-apps/api/core").invoke | undefined;

async function rpcImpl<T>(command: string, args: Record<string, unknown>, connectionId?: string): Promise<T> {
	// When connectionId is provided, always use HTTP fetch (remote daemon is accessed via HTTP)
	if (!connectionId && isTauri()) {
		if (!cachedTauriInvoke) {
			({ invoke: cachedTauriInvoke } = await import("@tauri-apps/api/core"));
		}
		// Only pass args if non-empty (matches Tauri invoke signature)
		if (Object.keys(args).length > 0) {
			return cachedTauriInvoke<T>(command, args);
		}
		return cachedTauriInvoke<T>(command);
	}

	const mapping = mapCommandToHttp(command, args);
	const baseUrl = connectionId ? getRemoteBaseUrl(connectionId) : undefined;
	if (connectionId && !baseUrl) {
		throw new Error(`Remote connection ${connectionId} not connected`);
	}
	const url = buildHttpUrl(mapping.path, baseUrl);

	const controller = new AbortController();
	const timeoutId = setTimeout(() => controller.abort(), 30_000);

	const init: RequestInit = {
		method: mapping.method,
		headers: { "Content-Type": "application/json" },
		signal: controller.signal,
	};
	if (mapping.body !== undefined) {
		init.body = JSON.stringify(mapping.body);
	}

	let resp: Response;
	try {
		resp = await fetch(url, init);
	} finally {
		clearTimeout(timeoutId);
	}
	if (!resp.ok) {
		if (resp.status === 404 && mapping.notFoundAsNull) {
			return null as T;
		}
		const text = await resp.text().catch(() => resp.statusText);
		throw new Error(`RPC ${command} failed: ${resp.status} ${text}`);
	}

	const contentType = resp.headers.get("content-type") || "";
	let data: unknown;
	if (contentType.includes("application/octet-stream")) {
		// Packed binary (styled row chunks). Text/JSON decoding would corrupt it,
		// and an empty body is a legitimate "nothing to send", so this returns the
		// buffer directly and skips the empty-body guard below.
		const buffer = await resp.arrayBuffer();
		return (mapping.transform ? mapping.transform(buffer) : buffer) as T;
	}
	const text = await resp.text();
	if (text.length === 0) {
		throw new Error(`RPC ${command}: empty response body`);
	}
	if (contentType.includes("application/json")) {
		try {
			data = JSON.parse(text);
		} catch (error) {
			const detail = error instanceof Error ? `: ${error.message}` : "";
			throw new Error(`RPC ${command}: invalid JSON response${detail}`);
		}
	} else {
		// Try parsing as JSON anyway (some endpoints may not set content-type)
		try {
			data = JSON.parse(text);
		} catch {
			data = text;
		}
	}

	if (mapping.transform) {
		return mapping.transform(data) as T;
	}
	if (data === undefined) {
		throw new Error(`RPC ${command}: empty response body`);
	}
	return data as T;
}

/** Unsubscribe function returned by subscribe() */
export type Unsubscribe = () => void;

/**
 * A PTY subscription: still callable to dispose it, plus the two controls a
 * backgrounded client needs.
 *
 * It is a callable object rather than a record so every existing caller — which
 * only ever invokes the handle — keeps working unchanged.
 */
export interface PtySubscription extends Unsubscribe {
	/**
	 * Stop draining the stream and drop the socket. NOT a session exit: `onExit`
	 * stays silent, and the consumed-line cursor survives so `resume` picks up
	 * exactly where delivery stopped.
	 */
	pause(): void;
	/** Re-open from the live cursor. A no-op unless currently paused. */
	resume(): void;
}

/** Parsed event from WebSocket JSON framing */
export interface WsParsedEvent {
	type: string;
	[key: string]: unknown;
}

/**
 * Subscribe to PTY session events.
 *
 * In Tauri: uses listen() for pty-activity-{sessionId}, pty-exit-{sessionId}.
 * In browser: uses WebSocket to /sessions/{sessionId}/stream with JSON framing:
 *   - {"type":"output","data":"..."} for raw PTY output
 *   - {"type":"activity"} for the throttled "output happened" pulse
 *   - {"type":"parsed","event":{...}} for structured events (questions, rate limits)
 *   - {"type":"exit"} / {"type":"closed"} for session lifecycle
 *
 * NOTE ON `onData`: desktop delivers NO output through this subscription. The
 * canvas renders from grid frames and plugin watcher lines are assembled in
 * Rust, so no desktop consumer needs the bytes and none crosses the IPC
 * boundary. Browser/PWA still receives them — `src/mobile/OutputView.tsx` reads
 * the stream directly. Use `onActivity` for "is this session producing output",
 * which is the question both transports answer identically.
 *
 * @param sessionId - PTY session ID
 * @param onData - Called with each chunk of PTY output (browser/PWA only)
 * @param onExit - Called when the session exits
 * @param onParsed - Optional: called with structured parsed events (browser mode)
 * @returns Promise resolving to an unsubscribe function
 */
export interface SubscribePtyOptions {
	/** Request ANSI-stripped plain text from the server (for non-terminal views like mobile) */
	stripAnsi?: boolean;
	/**
	 * Use VT100-extracted log lines (`format=log`).
	 * When `onLogLines` is set, structured LogLine objects are delivered there.
	 * Otherwise `onData` is called with `\n`-joined plain text (backward compat).
	 * Overrides `stripAnsi` when set.
	 */
	format?: "log";
	/**
	 * Receive structured LogLine objects from `format=log` frames.
	 * Each LogLine has `spans: [{text, fg?, bg?, bold?, italic?, underline?}]`.
	 */
	onLogLines?: (lines: LogLine[]) => void;
	/** Receive current screen rows (LogLine objects with styled spans) pushed alongside log frames. */
	onScreenRows?: (rows: unknown[]) => void;
	/** Receive the current PTY input line text (extracted from prompt row). */
	onInputLine?: (text: string | null) => void;
	/** Starting offset for log-mode catch-up (skip lines already fetched via HTTP). */
	logOffset?: number;
	/** Receive real-time SessionState snapshots pushed by the server on parsed events. */
	onStateChange?: (state: Record<string, unknown>) => void;
	/**
	 * "This session produced output." Throttled to ~1/s by the Rust producer and
	 * payload-free: it answers whether bytes are flowing, not what they were.
	 * Delivered on both transports from one backend signal.
	 */
	onActivity?: () => void;
	onParsed?: (event: WsParsedEvent) => void;
	/** Called when WebSocket drops and reconnect is attempted (browser mode only). */
	onReconnecting?: (attempt: number, maxAttempts: number) => void;
	/** Called when WebSocket reconnect succeeds (browser mode only). */
	onReconnected?: () => void;
}

export async function subscribePty(
	sessionId: string,
	onData: (data: string) => void,
	onExit: () => void,
	onParsedOrOptions?: ((event: WsParsedEvent) => void) | SubscribePtyOptions,
): Promise<PtySubscription> {
	// Normalize overloaded 4th param: function (legacy) or options object
	const opts: SubscribePtyOptions =
		typeof onParsedOrOptions === "function" ? { onParsed: onParsedOrOptions } : (onParsedOrOptions ?? {});
	const onParsed = opts.onParsed;
	// Shared by both transports so a caller can pause without knowing which one
	// it is on. Desktop has no socket to drop, so pausing there means suppressing
	// delivery — the same observable contract, at the only cost desktop has.
	let paused = false;
	if (isTauri()) {
		const { listen } = await import("@tauri-apps/api/event");
		// No pty-output listener: nothing emits that event. It was removed from
		// Rust in cda39f31 when line assembly moved to the reader thread, and the
		// listener outlived it by a commit — silently freezing lastDataAt and the
		// unread flag on desktop (story 625-56b0).
		const unlistenActivity = await listen(`pty-activity-${sessionId}`, () => {
			if (paused) return;
			opts.onActivity?.();
		});
		// No `paused` guard: an exit is lifecycle, not data. Desktop has no
		// reconnect to eventually notice a dead session, so suppressing it here
		// would lose it for good.
		const unlistenExit = await listen(`pty-exit-${sessionId}`, () => {
			onExit();
		});
		// Idempotent dispose. Tauri's unlisten is async and REJECTS if its internal
		// registry entry is already gone (double-unregister / session-exit race:
		// listeners[eventId].handlerId on undefined). A sync try/catch can't catch an
		// async rejection — swallow the promise rejection explicitly instead.
		let disposed = false;
		const dispose = () => {
			if (disposed) return;
			disposed = true;
			Promise.resolve(unlistenActivity() as unknown).catch(() => {});
			Promise.resolve(unlistenExit() as unknown).catch(() => {});
		};
		return Object.assign(dispose, {
			pause: () => {
				paused = true;
			},
			resume: () => {
				paused = false;
			},
		});
	}

	// Browser mode: WebSocket with JSON framing and auto-reconnect
	const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
	const queryFormat = opts.format === "log" ? "log" : opts.stripAnsi ? "text" : null;

	// Track server-side write offset for delta catch-up on reconnect
	let lastTotalWritten: number | null = null;
	let disposed = false;
	let activeWs: WebSocket | null = null;
	let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

	/**
	 * `socket` is the connection the frame arrived on. `close()` is asynchronous,
	 * so a socket dropped by `pause()` can still deliver after the `resume()` that
	 * replaced it — and by then a `paused` flag reads false again. Identity
	 * against `activeWs` is the guard that survives that; the flag cannot be.
	 */
	const handleMessage = (event: MessageEvent, socket: WebSocket) => {
		if (disposed) return;
		const raw = event.data as string;
		// JSON frame detection: starts with { and contains "type"
		if (raw.startsWith("{")) {
			try {
				const frame = JSON.parse(raw) as WsParsedEvent;
				// Lifecycle is never suppressed. A session that dies while the page
				// is hidden is still dead, and swallowing this frame would leave the
				// view showing it as live until the reconnect backoff gave up ten
				// attempts later.
				if (frame.type === "exit" || frame.type === "closed") {
					disposed = true; // Session truly ended — don't reconnect
					onExit();
					return;
				}
				// Everything below is data, and only the live socket's data counts.
				// A paused subscription has no live socket, so nothing reaches the
				// consumer — and because the cursor only advances on delivery,
				// nothing is skipped either.
				if (socket !== activeWs) return;
				// Track total_written for reconnect delta
				if (typeof (frame as Record<string, unknown>).total_written === "number") {
					lastTotalWritten = (frame as Record<string, unknown>).total_written as number;
				}
				switch (frame.type) {
					case "output":
						onData(frame.data as string);
						break;
					case "activity":
						opts.onActivity?.();
						break;
					case "log": {
						// Track the monotonic line cursor so reconnect resumes from the last
						// line we consumed instead of replaying from the mount offset (which
						// duplicated the whole scrollback on every WS reconnect).
						const logCursor = (frame as Record<string, unknown>).total_lines;
						if (typeof logCursor === "number") {
							lastTotalWritten = logCursor;
						}
						const lines = frame.lines as LogLine[] | undefined;
						if (lines && lines.length > 0) {
							if (opts.onLogLines) {
								opts.onLogLines(lines);
							} else {
								// Backward compat: join span texts as plain string
								const texts = lines.map((l) => {
									if (typeof l === "string") return l;
									if (l && typeof l === "object" && "spans" in l) {
										return ((l as { spans: { text: string }[] }).spans || []).map((s) => s.text).join("");
									}
									return String(l);
								});
								onData(texts.join("\n"));
							}
						}
						const screen = frame.screen as unknown[] | undefined;
						if (screen && opts.onScreenRows) {
							opts.onScreenRows(screen);
						}
						if (opts.onInputLine && frame.screen !== undefined) {
							const il = (frame as Record<string, unknown>).input_line;
							opts.onInputLine(typeof il === "string" ? il : null);
						}
						break;
					}
					case "state":
						if (opts.onStateChange && frame.state) {
							opts.onStateChange(frame.state as Record<string, unknown>);
						}
						break;
					case "parsed":
						onParsed?.(frame);
						break;
				}
				return;
			} catch {
				// Not valid JSON — treat as raw output (backward compat)
			}
		}
		if (socket !== activeWs) return;
		onData(raw);
	};

	/** Build the WS URL with current params (including offset for reconnect). */
	const buildWsUrl = (reconnectOffset?: number | null): string => {
		const params = new URLSearchParams();
		if (queryFormat) params.set("format", queryFormat);
		if (reconnectOffset != null) {
			params.set("offset", String(reconnectOffset));
		} else if (opts.logOffset != null) {
			params.set("offset", String(opts.logOffset));
		}
		const query = params.size > 0 ? `?${params}` : "";
		return `${protocol}//${window.location.host}/sessions/${sessionId}/stream${query}`;
	};

	/** Connect (or reconnect) the WebSocket. */
	const connect = (reconnectOffset?: number | null): Promise<void> =>
		new Promise<void>((resolve, reject) => {
			const wsUrl = buildWsUrl(reconnectOffset);
			const ws = new WebSocket(wsUrl);
			activeWs = ws;

			ws.onopen = () => {
				// A pause, or a newer attempt, replaced this one while it was still
				// opening. Leaving it open would stream a second copy of the session
				// into the same consumer, so it closes itself and reports failure to
				// whoever is awaiting it.
				if (ws !== activeWs) {
					ws.close();
					reject(new Error(`WebSocket attempt superseded: ${sessionId}`));
					return;
				}
				transportLogger().debug("network", `WebSocket connected: ${sessionId}`);
				// Re-wire onclose for live session
				ws.onclose = (evt: CloseEvent) => {
					if (disposed) return;
					// This socket is no longer the live one: a pause dropped it, or a
					// resume already replaced it. Reading that as a session exit is
					// the trap this feature exists to avoid — the view would print
					// "session exited" and clear the screen on every tab switch — and
					// reconnecting on it would race the live socket.
					if (ws !== activeWs) return;
					if (evt.code === 1000 || evt.code === 1001) {
						// Normal close or going away — don't reconnect. Terminal, like
						// the exit frame and like retry exhaustion: the same news by a
						// third route. Without this the consumer is told the session
						// exited while the subscription still thinks a later resume
						// may reopen it.
						disposed = true;
						onExit();
						return;
					}
					transportLogger().debug("network", `WebSocket closed abnormally (code ${evt.code}), will reconnect`);
					scheduleReconnect();
				};
				resolve();
			};

			ws.onerror = () => {
				// onerror is always followed by onclose, so reject is handled there
			};

			ws.onclose = (evt: CloseEvent) => {
				reject(new Error(`WebSocket closed before opening (code ${evt.code}): ${evt.reason || "no reason"}`));
			};

			ws.onmessage = (event: MessageEvent) => handleMessage(event, ws);
		});

	// Reconnect with exponential backoff
	const MAX_RETRIES = 10;
	const BASE_DELAY_MS = 1000;
	const MAX_DELAY_MS = 30_000;
	let retryCount = 0;

	/**
	 * Both success paths report through here so a consumer's exception can never
	 * be mistaken for a failed connection. The callback shares a promise chain
	 * with `connect()`, and a throw landing in that rejection handler would
	 * announce a reconnect nothing broke and open a second socket beside the
	 * healthy one.
	 */
	const announceReconnected = () => {
		try {
			opts.onReconnected?.();
		} catch {
			transportLogger().warn("network", `onReconnected callback threw: ${sessionId}`);
		}
	};

	const scheduleReconnect = () => {
		if (disposed || paused) return;
		if (retryCount >= MAX_RETRIES) {
			transportLogger().warn("network", `WebSocket reconnect failed after ${MAX_RETRIES} attempts: ${sessionId}`);
			// Terminal, like the exit frame. Without this the subscription stays
			// live-looking, and a later resume would restart the whole doomed
			// budget and report the exit again when it ran out.
			disposed = true;
			onExit();
			return;
		}
		const delay = Math.min(BASE_DELAY_MS * 2 ** retryCount, MAX_DELAY_MS);
		retryCount++;
		transportLogger().debug("network", `WebSocket reconnecting in ${delay}ms (attempt ${retryCount}/${MAX_RETRIES})`);
		opts.onReconnecting?.(retryCount, MAX_RETRIES);
		reconnectTimer = setTimeout(async () => {
			if (disposed || paused) return;
			const pending = connect(lastTotalWritten);
			// connect() installs its socket synchronously, so this names THIS
			// attempt. A pause or a newer attempt during the handshake replaces it,
			// and a superseded attempt must not schedule anything of its own.
			const mine = activeWs;
			try {
				await pending;
				retryCount = 0; // Reset on success
				announceReconnected();
			} catch {
				// connect() failed (e.g. session gone → 404 triggers immediate close)
				if (mine !== activeWs) return;
				scheduleReconnect();
			}
		}, delay);
	};

	// Initial connection
	await connect();

	const dispose = () => {
		disposed = true;
		if (reconnectTimer) clearTimeout(reconnectTimer);
		activeWs?.close();
	};

	return Object.assign(dispose, {
		pause: () => {
			if (disposed || paused) return;
			paused = true;
			// A backoff timer already in flight would otherwise reopen the socket
			// behind the pause: it was scheduled before the flag was set.
			if (reconnectTimer) {
				clearTimeout(reconnectTimer);
				reconnectTimer = null;
			}
			activeWs?.close();
			activeWs = null;
		},
		resume: () => {
			if (disposed || !paused) return;
			paused = false;
			// Reconnect at once rather than through the backoff, so coming back to
			// the tab is not made to wait out a delay earned before it was hidden.
			// The retry budget is NOT refilled here: only a successful connect
			// clears it, or a dead session would get a fresh ten attempts on every
			// hide/show and never reach the exit the user needs to see.
			const pending = connect(lastTotalWritten);
			const mine = activeWs;
			// A reconnect already announced to the consumer is finished by this
			// connect, not abandoned by it. Without this, a pause landing between
			// `onReconnecting` and its backoff leaves the consumer's banner up for
			// good, even though the socket is healthy again.
			const wasReconnecting = retryCount > 0;
			pending
				.then(() => {
					retryCount = 0;
					if (wasReconnecting) announceReconnected();
				})
				.catch(() => {
					if (mine === activeWs) scheduleReconnect();
				});
		},
	});
}

/** Why `onResync` fired: events were missed, and this says how. */
export type ResyncReason = "reconnect" | "lagged";

export interface SubscribeEventsOptions {
	/** Base URL of a remote instance. Omit to talk to this one. */
	baseUrl?: string;
	/**
	 * Called when a gap opened in the stream and the consumer should re-read
	 * whatever state it derives from these events. NOT called for a normal
	 * event, and NOT called on the first connection — a consumer that has just
	 * subscribed does its own initial read.
	 *
	 * Only the SSE transport can fire it. Tauri `listen()` is in-process: there
	 * is no connection to drop and no bounded channel to fall behind, so a
	 * desktop resync path would be a second path for a transition that already
	 * has one, which is the defect the AGENTS.md fix-quality rule names.
	 */
	onResync?: (reason: ResyncReason) => void;
}

/**
 * Subscribe to application-level events (head-changed, repo-changed, etc.)
 *
 * In Tauri: delegates to individual listen() calls.
 * In browser: creates a single EventSource to /events SSE endpoint.
 *
 * @param handlers - Map of event type → callback
 * @param options - Remote base URL and the missed-events callback
 * @returns Promise resolving to an unsubscribe function
 */
export async function subscribeEvents(
	handlers: Record<string, (payload: unknown) => void>,
	options: SubscribeEventsOptions = {},
): Promise<Unsubscribe> {
	const { baseUrl, onResync } = options;
	if (!baseUrl && isTauri()) {
		const { listen } = await import("@tauri-apps/api/event");
		const unsubscribers: Array<() => void> = [];
		for (const [eventType, handler] of Object.entries(handlers)) {
			const unlisten = await listen(eventType, (event) => handler(event.payload));
			unsubscribers.push(unlisten);
		}
		return () => unsubscribers.forEach((fn) => fn());
	}

	// Browser/remote mode: SSE via EventSource
	const types = Object.keys(handlers).join(",");
	const url = buildHttpUrl(`/events?types=${encodeURIComponent(types)}`, baseUrl);
	const es = new EventSource(url);

	for (const [eventType, handler] of Object.entries(handlers)) {
		es.addEventListener(eventType, ((event: MessageEvent) => {
			try {
				const payload = JSON.parse(event.data);
				handler(payload);
			} catch {
				transportLogger().warn("network", `Failed to parse SSE event "${eventType}"`, {
					eventData: previewLogPayload(event.data),
				});
			}
		}) as EventListener);
	}

	es.addEventListener("lagged", ((event: MessageEvent) => {
		transportLogger().warn("network", "SSE lagged", { eventData: previewLogPayload(event.data) });
		// The backend's broadcast channel dropped events for this subscriber. They
		// are gone; only the consumer knows how to re-derive what they carried.
		onResync?.("lagged");
	}) as EventListener);

	// `onopen` fires on every successful connection, so the FIRST one is the
	// initial connect and every later one is a reconnect. Only the later ones are
	// a gap: EventSource reconnects itself, silently, and whatever the backend
	// published while it was down was never queued for us.
	let everOpened = false;
	es.onopen = () => {
		if (everOpened) onResync?.("reconnect");
		everOpened = true;
	};

	es.onerror = () => {
		transportLogger().debug("network", "SSE connection error — will auto-reconnect");
	};

	return () => es.close();
}
