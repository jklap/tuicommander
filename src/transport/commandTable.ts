import type { HttpMapping } from "./types";

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

/** Builds the `&ignoreLeadingWs=true&...` query-string suffix `mcp_http/types.rs`'s
 *  `PathQuery::diff_options`/`FileQuery::diff_options` expect, from a `DiffOptions`
 *  object (or nothing, when every option is absent/false — the default diff). Shared
 *  by `get_git_diff` and `get_file_diff` (both in this file). */
export function diffOptionsQueryString(options: unknown): string {
	if (!isRecord(options)) return "";
	const flags = ["ignoreLeadingWs", "ignoreTrailingWs", "ignoreWsAmount", "ignoreCase"];
	let suffix = "";
	for (const flag of flags) {
		if (options[flag]) suffix += `&${flag}=true`;
	}
	return suffix;
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
export const COMMAND_TABLE: Record<string, CommandTableEntry> = {
	resume_session: {
		map: (args) => ({ method: "POST", path: "/sessions/wake", body: { tuicSession: args.tuicSession } }),
	},
	suspended_session_action: { map: (args) => ({ method: "POST", path: "/sessions/suspended", body: args.request }) },
	work_item_provider_bridge: {
		map: (args) => ({ method: "POST", path: "/work-items/provider-bridge", body: args.input }),
	},
	account_secret_form: { map: (args) => ({ method: "POST", path: "/accounts/secret-form", body: args.request }) },
	account_action: { map: (args) => ({ method: "POST", path: "/accounts/action", body: args.request }) },
	telegram_settings: { map: () => ({ method: "GET", path: "/config/telegram" }) },
	telegram_setup: { map: (args) => ({ method: "PUT", path: "/config/telegram", body: { change: args.change } }) },
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
			// `choice` likewise mirrors the desktop command; the server ignores a
			// `custom` file over HTTP and plays the default tone instead.
			body: { sound: args.sound, volume: args.volume, device: args.device ?? null, choice: args.choice ?? null },
		}),
	},
	// --- Relay ---
	get_relay_status: { map: () => ({ method: "GET", path: "/system/relay-status" }) },
	// --- Update channel ---
	check_update_channel: {
		map: (_args, p) => ({ method: "GET", path: `/system/check-update?channel=${p("channel")}` }),
	},
	// --- MCP upstream config (proxied through server for keyring access) ---
	load_mcp_upstreams: { map: () => ({ method: "GET", path: "/mcp/upstreams" }) },
	get_mcp_upstream_status: { map: () => ({ method: "GET", path: "/mcp/upstream-status" }) },
	save_mcp_upstreams: {
		map: (args) => ({
			method: "PUT",
			path: "/mcp/upstreams",
			body: { base: args.base, config: args.config, ...(args.credentials ? { credentials: args.credentials } : {}) },
		}),
	},

	// --- ACP (ego) ---
	// One entry per acp_* command, session-scoped like the routes they map to.
	// The bodies carry exactly the arguments the Tauri command takes, because
	// the client's refusals are computed in Rust and must be identical on both
	// transports — a body that dropped a field would move a decision here.
	acp_workspace_root: {
		map: () => ({ method: "GET", path: "/acp/workspace" }),
	},
	acp_chat_open: {
		map: (args) => ({ method: "POST", path: "/acp/chat/open", body: args.request }),
	},
	acp_connect: {
		map: (args) => ({ method: "POST", path: "/acp/connections", body: { root: args.root } }),
	},
	acp_connection_snapshot: {
		map: (_args, p) => ({ method: "GET", path: `/acp/connections/${p("connectionId")}` }),
	},
	acp_disconnect: {
		map: (_args, p) => ({ method: "DELETE", path: `/acp/connections/${p("connectionId")}` }),
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
		map: (args, p) => {
			const query = new URLSearchParams();
			if (args.cwd !== undefined && args.cwd !== null) query.set("cwd", String(args.cwd));
			if (args.cursor !== undefined && args.cursor !== null) query.set("cursor", String(args.cursor));
			const suffix = query.toString() ? `?${query.toString()}` : "";
			return { method: "GET", path: `/acp/connections/${p("connectionId")}/sessions${suffix}` };
		},
	},
	acp_session_load: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/load`,
			body: { authority: args.authority },
		}),
	},
	acp_session_fork: {
		map: (args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/fork`,
			body: { authority: args.authority, ...(args.atMessageId !== undefined ? { atMessageId: args.atMessageId } : {}) },
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
			body: { prompt: args.prompt, viewedRepo: args.viewedRepo ?? null },
		}),
	},
	acp_session_cancel: {
		map: (_args, p) => ({
			method: "POST",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/cancel`,
		}),
	},
	acp_queued_prompt_cancel: {
		map: (_args, p) => ({
			method: "DELETE",
			path: `/acp/connections/${p("connectionId")}/sessions/${p("sessionId")}/queue/${p("turnId")}`,
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
	ego_perimeter: {
		map: () => ({ method: "GET", path: "/ego/perimeter" }),
	},
	ego_set_perimeter_roots: {
		map: (args) => ({ method: "POST", path: "/ego/perimeter/roots", body: { roots: args.roots } }),
	},
	ego_set_perimeter_network: {
		map: (args) => ({ method: "POST", path: "/ego/perimeter/network", body: { enabled: args.enabled } }),
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
	force_retry_provider_recovery_command: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${encodeURIComponent(String(args.sessionId))}/force-retry`,
		}),
	},
	submit_agent_reply: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${encodeURIComponent(String(args.sessionId))}/submit`,
			body: { input: args.input },
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
			body: {
				text: args.text,
				...(args.expectedDraft != null ? { expectedDraft: args.expectedDraft } : {}),
				...(args.idempotencyKey != null ? { idempotencyKey: args.idempotencyKey } : {}),
			},
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
	get_prompt_receipt: {
		map: (args) => ({ method: "GET", path: `/sessions/${args.sessionId}/prompt-receipt` }),
	},
	get_last_prompt: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/last-prompt`,
			transform: (data) => (data as { prompt: string | null }).prompt ?? null,
		}),
	},
	send_agent_input_if_empty: {
		map: (args) => ({ method: "POST", path: `/sessions/${args.sessionId}/input-if-empty`, body: { text: args.text } }),
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
			body: { visible: args.visible },
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
	terminal_scroll_to_anchor: {
		map: (args) => ({
			method: "POST",
			path: `/sessions/${args.sessionId}/terminal/scroll-to-anchor`,
			body: { anchor: args.anchor },
		}),
	},
	terminal_get_lines: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/terminal/lines?start=${args.start}&end=${args.end}`,
			transform: (data) => (data as { lines: string[] }).lines,
		}),
	},
	chat_view_snapshot: {
		map: (args) => ({
			method: "GET",
			path: `/sessions/${args.sessionId}/chat-view?from_seq=${args.fromSeq ?? 0}${args.epoch != null ? `&epoch=${args.epoch}` : ""}`,
		}),
	},

	// --- Orchestrator ---
	get_orchestrator_stats: { map: () => ({ method: "GET", path: "/stats" }) },
	get_session_metrics: { map: () => ({ method: "GET", path: "/metrics" }) },

	// --- Claude Usage dashboard ---
	get_claude_usage_api: {
		map: (args, p) => ({
			method: "GET",
			path: args.sessionId == null ? "/claude/usage" : `/claude/usage?sessionId=${p("sessionId")}`,
		}),
	},
	list_active_sessions: { map: () => ({ method: "GET", path: "/sessions" }) },
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

	// --- Config: app ---
	load_config: { map: () => ({ method: "GET", path: "/config" }) },
	save_config: { map: (args) => ({ method: "PUT", path: "/config", body: { base: args.base, config: args.config } }) },
	load_app_config: { map: () => ({ method: "GET", path: "/config" }) },
	save_app_config: {
		map: (args) => ({ method: "PUT", path: "/config", body: { base: args.base, config: args.config } }),
	},
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
		map: (args) => ({ method: "PUT", path: "/config/notifications", body: { base: args.base, config: args.config } }),
	},

	// --- Config: UI prefs ---
	load_ui_prefs: { map: () => ({ method: "GET", path: "/config/ui-prefs" }) },
	save_ui_prefs: {
		map: (args) => ({ method: "PUT", path: "/config/ui-prefs", body: { base: args.base, config: args.config } }),
	},

	// --- Config: repo settings ---
	load_repo_settings: { map: () => ({ method: "GET", path: "/config/repo-settings" }) },
	save_repo_settings: {
		map: (args) => ({ method: "PUT", path: "/config/repo-settings", body: { base: args.base, config: args.config } }),
	},
	check_has_custom_settings: {
		map: (_args, p) => ({ method: "GET", path: `/config/repo-settings/has-custom?path=${p("path")}` }),
	},
	load_repo_defaults: { map: () => ({ method: "GET", path: "/config/repo-defaults" }) },
	save_repo_defaults: {
		map: (args) => ({ method: "PUT", path: "/config/repo-defaults", body: { base: args.base, config: args.config } }),
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

	// --- Desktop Markdown metadata (mobile reads; publication stays loopback-only) ---
	list_open_markdown_tabs: {
		map: (args) => ({ method: "GET", path: `/repo/open-markdown-tabs?cwd=${encodeURIComponent(String(args.cwd))}` }),
	},
	sync_open_markdown_tabs: {
		map: (args) => ({ method: "PUT", path: "/config/open-markdown-tabs", body: { tabs: args.tabs } }),
	},
	// --- Config: pane layout ---
	load_pane_layout: { map: () => ({ method: "GET", path: "/config/pane-layout" }) },
	save_pane_layout: {
		map: (args) => ({ method: "PUT", path: "/config/pane-layout", body: { base: args.base, config: args.layout } }),
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
	progress_list: {
		map: (args, p) => ({ method: "POST", path: `/progress/list?path=${p("project")}`, body: args.input }),
	},
	progress_projects: { map: () => ({ method: "GET", path: "/progress/projects" }) },
	automation_action: {
		map: (args) => ({ method: "POST", path: "/automations/action", body: { input: args.input } }),
	},
	progress_delete: {
		map: (args, p) => ({ method: "POST", path: `/progress/delete?path=${p("project")}`, body: args.input }),
	},
	progress_mark_viewed: {
		map: (args, p) => ({
			method: "POST",
			path: `/progress/viewed?path=${p("project")}${args.ptyId ? `&ptyId=${p("ptyId")}` : ""}`,
		}),
	},
	progress_flow: {
		map: (args, p) => ({ method: "POST", path: `/progress/flow?path=${p("project")}`, body: args.input }),
	},
	progress_flow_detail: {
		map: (args) => ({ method: "POST", path: "/progress/flow/detail", body: args.input }),
	},

	// --- Config: prompt library ---
	load_prompt_library: { map: () => ({ method: "GET", path: "/config/prompt-library" }) },
	save_prompt_library: {
		map: (args) => ({ method: "PUT", path: "/config/prompt-library", body: { base: args.base, config: args.config } }),
	},

	// --- Config: activity ---
	load_activity: { map: () => ({ method: "GET", path: "/config/activity" }) },
	save_activity: {
		map: (args) => ({ method: "PUT", path: "/config/activity", body: { base: args.base, config: args.items } }),
	},

	// --- Config: keybindings ---
	load_keybindings: { map: () => ({ method: "GET", path: "/config/keybindings" }) },
	save_keybindings: {
		map: (args) => ({ method: "PUT", path: "/config/keybindings", body: { base: args.base, config: args.config } }),
	},

	// --- Config: agents ---
	load_agents_config: { map: () => ({ method: "GET", path: "/config/agents" }) },
	save_agents_config: {
		map: (args) => ({ method: "PUT", path: "/config/agents", body: { base: args.base, config: args.config } }),
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

	// --- Plugin data ---
	// Tauri contract is Option<String>: missing key → null. The route 404s on miss,
	// which notFoundAsNull bridges back to null. Found content is returned as a string
	// to match the command's String payload (the route may sniff JSON and parse it).
	read_plugin_data: {
		map: (_args, p) => ({
			method: "GET",
			path: `/api/plugins/${p("pluginId")}/data/${p("path")}`,
			notFoundAsNull: true,
			allowText: true,
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

	// --- Git/GitHub ---
	get_repo_info: {
		map: (_args, p) => ({ method: "GET", path: `/repo/info?path=${p("path")}` }),
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
	create_branch: {
		map: (args) => ({
			method: "POST",
			path: "/repo/create-branch",
			body: { path: args.path, name: args.name, startPoint: args.startPoint, checkout: args.checkout },
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
	get_git_diff: {
		map: (args, p) => {
			let diffUrl = `/repo/diff?path=${p("path")}`;
			if (args?.scope) diffUrl += `&scope=${encodeURIComponent(String(args.scope))}`;
			diffUrl += diffOptionsQueryString(args?.options);
			return {
				method: "GET",
				path: diffUrl,
				transform: (data) => (data as { diff: string }).diff,
			};
		},
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
	get_pr_action_state: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/pr-action-state?repoPath=${p("repoPath")}&prNumber=${p("prNumber")}`,
		}),
	},
	set_pr_auto_merge: {
		map: (args) => ({
			method: "POST",
			path: "/repo/pr-auto-merge",
			body: {
				repoPath: args.repoPath,
				prNumber: args.prNumber,
				enabled: args.enabled,
				mergeMethod: args.mergeMethod,
				expectedHeadSha: args.expectedHeadSha,
			},
		}),
	},
	set_pr_merge_queue: {
		map: (args) => ({
			method: "POST",
			path: "/repo/pr-merge-queue",
			body: {
				repoPath: args.repoPath,
				prNumber: args.prNumber,
				enqueued: args.enqueued,
				expectedHeadSha: args.expectedHeadSha,
			},
		}),
	},
	merge_or_enqueue_pr: {
		map: (args) => ({
			method: "POST",
			path: "/repo/pr-merge-or-enqueue",
			body: {
				repoPath: args.repoPath,
				prNumber: args.prNumber,
				mergeMethod: args.mergeMethod,
				expectedHeadSha: args.expectedHeadSha,
			},
		}),
	},
	rerun_failed_pr_jobs: {
		map: (args) => ({
			method: "POST",
			path: "/repo/pr-rerun-failed",
			body: { repoPath: args.repoPath, runId: args.runId },
		}),
	},
	get_pr_line_threads: {
		map: (args, p) => ({
			method: "GET",
			path: `/repo/pr-line-threads?repoPath=${p("repoPath")}&prNumber=${p("prNumber")}${args.after ? `&after=${p("after")}` : ""}`,
		}),
	},
	get_pr_open_threads: {
		map: (_args, p) => ({
			method: "GET",
			path: `/repo/pr-open-threads?repoPath=${p("repoPath")}&prNumber=${p("prNumber")}`,
		}),
	},
	get_pr_thread_comments: {
		map: (args, p) => ({
			method: "GET",
			path: `/repo/pr-thread-comments?repoPath=${p("repoPath")}&threadId=${p("threadId")}${args.after ? `&after=${p("after")}` : ""}`,
		}),
	},
	set_pr_thread_resolved: {
		map: (args) => ({
			method: "POST",
			path: "/repo/pr-thread-resolved",
			body: { repoPath: args.repoPath, threadId: args.threadId, resolved: args.resolved },
		}),
	},
	reply_pr_thread: {
		map: (args) => ({
			method: "POST",
			path: "/repo/pr-thread-reply",
			body: { repoPath: args.repoPath, threadId: args.threadId, body: args.body },
		}),
	},
	submit_pr_draft_review: {
		map: (args) => ({
			method: "POST",
			path: "/repo/pr-draft-review",
			body: { repoPath: args.repoPath, prNumber: args.prNumber, draft: args.draft },
		}),
	},
	get_github_viewer_login: {
		map: () => ({ method: "GET", path: "/github/viewer-login" }),
	},
	github_set_pr_hide_drafts: {
		map: (args) => ({ method: "POST", path: "/github/pr-hide-drafts", body: { hide: args.hide } }),
	},
	set_branch_label: {
		map: (args) => ({
			method: "POST",
			path: "/config/branch-label",
			body: { repoPath: args.repoPath, branchName: args.branchName, label: args.label },
		}),
	},
	save_note_image: {
		map: (args) => ({
			method: "POST",
			path: "/config/note-image",
			body: { noteId: args.noteId, dataBase64: args.dataBase64, extension: args.extension },
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
	claude_project_dir: {
		map: (args) => ({
			method: "POST",
			path: "/agent/claude-project-dir",
			body: { cwd: args.cwd, claudeConfigDir: args.claudeConfigDir },
		}),
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
	github_set_issue_filter: {
		map: (args) => ({
			method: "POST",
			path: "/repo/github-poller/set-issue-filter",
			body: { filter: args.filter },
		}),
	},
	get_git_branches: {
		map: (_args, p) => ({ method: "GET", path: `/repo/branches?path=${p("path")}` }),
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
	get_remote_url: {
		map: (_args, p) => ({ method: "GET", path: `/repo/remote-url?path=${p("path")}` }),
	},
	get_git_remotes: {
		map: (_args, p) => ({ method: "GET", path: `/repo/remotes?path=${p("path")}` }),
	},
	list_workitem_links: {
		map: () => ({ method: "GET", path: "/workitem-links" }),
	},
	link_workitem: {
		map: (args) => ({
			method: "PUT",
			path: "/workitem-links",
			body: { repoPath: args.repoPath, branch: args.branch, link: args.link },
		}),
	},
	unlink_workitem: {
		map: (args) => ({
			method: "DELETE",
			path: "/workitem-links",
			body: { repoPath: args.repoPath, branch: args.branch },
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
	list_local_branches: {
		map: (_args, p) => ({ method: "GET", path: `/repo/local-branches?path=${p("repoPath")}` }),
	},

	// --- File operations ---
	list_markdown_files: {
		map: (_args, p) => ({ method: "GET", path: `/repo/markdown-files?path=${p("path")}` }),
	},
	read_file: {
		map: (_args, p) => ({ method: "GET", path: `/repo/file?path=${p("path")}&file=${p("file")}` }),
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
	detect_all_agent_binaries: {
		map: (args) => ({ method: "POST", path: "/agents/detect-all", body: { binaries: args.binaries } }),
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
	managed_agent_launch: {
		map: (args) => ({ method: "POST", path: "/agents/launch", body: args.request }),
	},
	spawn_agent: {
		map: (args) => {
			const ptyConfig = isRecord(args.pty_config) ? args.pty_config : {};
			const agentConfig = isRecord(args.agent_config) ? args.agent_config : {};
			return {
				method: "POST",
				path: "/sessions/agent",
				body: {
					rows: ptyConfig.rows,
					cols: ptyConfig.cols,
					cwd: agentConfig.cwd ?? ptyConfig.cwd,
					env: ptyConfig.env,
					prompt: agentConfig.prompt,
					model: agentConfig.model,
					print_mode: agentConfig.print_mode,
					output_format: agentConfig.output_format,
					agent_type: agentConfig.agent_type,
					binary_path: agentConfig.binary_path,
					args: agentConfig.args,
				},
				transform: (data) => {
					if (isRecord(data) && typeof data.session_id === "string") return data.session_id;
					throw new Error("spawn_agent HTTP response missing session_id");
				},
			};
		},
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
	compare_work_item_comments: {
		map: (args) => ({ method: "POST", path: "/tasks/compare-comments", body: args.input }),
	},
	project_work_items: {
		map: (args) => ({ method: "POST", path: "/tasks/project", body: args.input }),
	},
	build_start_work_prompt: {
		map: (args) => ({
			method: "POST",
			path: "/tasks/start-work-prompt",
			body: { repoPath: args.repoPath, input: args.input },
		}),
	},
	fs_read_file: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read?repoPath=${p("repoPath")}&file=${p("file")}` }),
	},
	read_external_file: {
		map: (_args, p) => ({ method: "GET", path: `/fs/read-external?path=${p("path")}` }),
	},
	write_file: {
		map: (args) => ({
			method: "POST",
			path: "/fs/write",
			body: { repoPath: args.repoPath, file: args.file, content: args.content },
		}),
	},
	write_file_if_unchanged: {
		map: (args) => ({
			method: "POST",
			path: "/fs/write-if-unchanged",
			body: { repoPath: args.repoPath, file: args.file, expected: args.expected, content: args.content },
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
	resolve_markdown_link: {
		map: (args) => ({
			method: "POST",
			path: "/fs/resolve-markdown-link",
			body: { root: args.root, currentFile: args.currentFile, href: args.href },
		}),
	},
	stat_path: {
		map: (_args, p) => ({ method: "GET", path: `/fs/stat?path=${p("path")}` }),
	},
	warm_content_index: {
		map: (args) => ({ method: "POST", path: "/fs/warm-index", body: { repoPath: args.repoPath } }),
	},
	write_external_file: {
		map: (args) => ({
			method: "POST",
			path: "/fs/write-external",
			body: { path: args.path, content: args.content },
		}),
	},

	// --- Notes ---
	load_notes: { map: () => ({ method: "GET", path: "/config/notes" }) },
	save_notes: {
		map: (args) => ({ method: "PUT", path: "/config/notes", body: { base: args.base, config: args.config } }),
	},

	// --- Recent commits ---
	get_recent_commits: {
		map: (args, p) => ({
			method: "GET",
			path: `/repo/recent-commits?path=${p("path")}&count=${args.count ?? 5}`,
		}),
	},

	// --- Plugins ---
	list_user_plugins: {
		map: (args) => ({
			method: "GET",
			path:
				args.pluginIds == null
					? "/plugins/list"
					: `/plugins/list?plugin_ids=${encodeURIComponent(JSON.stringify(args.pluginIds))}`,
		}),
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
	configure_ssh_daemon_password: {
		map: (args, p) => ({
			method: "POST",
			path: `/config/remote-connections/${p("id")}/configure-ssh-password`,
			body: { plan_digest: args.planDigest },
		}),
	},
	reload_design_mode: {
		map: (args) => ({ method: "POST", path: "/design-mode/reload", body: { repoPath: args.repoPath } }),
	},
	design_mode_browser_command: {
		map: (a) => ({
			method: "POST",
			path: "/design-mode/command",
			body: { repoPath: a.repoPath, action: a.action, args: a.args },
		}),
	},
	navigate_design_mode: {
		map: (args) => ({
			method: "POST",
			path: "/design-mode/navigate",
			body: { repoPath: args.repoPath, url: args.url },
		}),
	},
	list_tunnel_profiles: { map: () => ({ method: "GET", path: "/tunnels/profiles" }) },
	save_tunnel_profile: { map: (args) => ({ method: "POST", path: "/tunnels/profiles", body: args.profile }) },
	delete_tunnel_profile: { map: (args) => ({ method: "DELETE", path: `/tunnels/profiles/${args.id}` }) },
	start_tunnel: { map: (args) => ({ method: "POST", path: `/tunnels/start/${args.id}` }) },
	stop_tunnel: { map: (args) => ({ method: "POST", path: `/tunnels/stop/${args.id}` }) },
	list_active_tunnels: { map: () => ({ method: "GET", path: "/tunnels/active" }) },
	get_tunnel_status: { map: (args) => ({ method: "GET", path: `/tunnels/status/${args.id}` }) },
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

	// --- Opt-in terminal-content evidence ---
	save_frame_workload: {
		map: (args) => ({ method: "POST", path: "/debug/frame-workload", body: { recording: args.recording } }),
	},
	// --- App Logger ---
	push_log: {
		map: (args) => ({
			method: "POST",
			path: "/logs",
			allowNoContent: true,
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
	clear_logs: { map: () => ({ method: "DELETE", path: "/logs", allowNoContent: true }) },

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
		map: (args, p) => {
			let path = `/api/plugins/${p("pluginId")}/fs/read-base64?path=${p("path")}`;
			if (args.maxBytes != null) path += `&maxBytes=${p("maxBytes")}`;
			return { method: "GET", path };
		},
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
	plugin_github_action: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/github`,
			body: { account: args.account, reference: args.reference, request: args.request },
		}),
	},
	plugin_http_cancel: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/http/cancel`,
			body: { requestId: args.requestId },
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
				account: args.account,
				requestId: args.requestId,
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
	settings_action: {
		map: (args) => ({ method: "POST", path: "/settings/action", body: args.request }),
	},
	plugin_settings_action: {
		map: (args, p) => ({
			method: "POST",
			path: `/api/plugins/${p("pluginId")}/settings`,
			body: args.action,
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
 * imported only by the non-mobile entries (src/appEntry.tsx and the dev
 * fixture pages) — see that file's doc comment for why the split exists and
 * why mobile must never import it.
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
	// Native desktop lifecycle; remote clients must not hide or quit the host.
	"request_quit",
	"resolve_close",
	"ack_background_snapshot",
	// Browser clients use their own navigator.clipboard, never the host pasteboard.
	"write_clipboard_text",
	"read_clipboard_text",
	// A native window identity grants nonce bootstrap. HTTP clients use the
	// capability link shown only in that window, never a discoverable bootstrap.
	"secret_form_bootstrap",
	// Data leaves the machine; source paths come from Finder and cannot be gated
	// to registered roots. HTTP token holders must not trigger exfiltration.
	"fs_transfer_remote_paths",
	// Multi-window management — secondary/panel windows are a desktop-only concept.
	"open_secondary_window",
	// Open on the client machine; remote HTTP must not launch host applications.
	"open_default_app",
	"open_panel_window",
	"close_panel_window",
	"focus_panel_window",
	"focus_main_window",
	"show_native_notification",
	// Native drag-and-drop (WKWebView/OS drag) — no browser equivalent.
	"start_native_drag",
	// Native file pickers (NSOpenPanel/NSSavePanel and their peers) — the host's
	// own filesystem browser. A remote client picks from ITS machine through the
	// in-app file browser, so there is nothing to map.
	"pick_path",
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
	// Terminal grid push — browser uses its session WebSocket instead of Tauri channels.
	"subscribe_terminal_grid",
	"unsubscribe_terminal_grid",
	"ack_terminal_frame",
	"terminal_exit_alt_screen",
	// WebView liveness beat. Deliberately host-only: it watches the embedded
	// WebView's main thread, and a browser client beating on the same channel
	// would mask a dead desktop one.
	"frontend_heartbeat",
	// Desktop-only terminal/session diagnostics or local visual state with no
	// faithful HTTP contract yet.
	"set_ansi_colors",
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
