import { substituteActionParameter } from "./smartSelection";
import type { SmartSelectionAction } from "./smartSelectionTypes";

/** Context substituted into an action's `parameter` template before dispatch —
 *  see `substituteActionParameter`'s `\0`-`\9`/`\d`/`\u`/`\h` codes. */
export interface SmartSelectionActionContext {
	matchText: string;
	groups: (string | undefined)[];
	cwd?: string;
	user?: string;
	host?: string;
}

/**
 * Everything a smart-selection action needs to actually run, injected so this
 * module is testable without Tauri/DOM — mirrors the `WatcherFireDeps`
 * pattern in `src/stores/watcherFire.ts`. The real implementations live in
 * `CanvasTerminal.tsx`, each a thin wrapper around an existing utility
 * (`writeClipboard`, `handleOpenUrl`, `sendCommand`, …) bound to the current
 * session — this module only decides WHICH one to call and with what text.
 */
export interface SmartSelectionActionDeps {
	copyToClipboard: (text: string) => Promise<void>;
	/** Must apply the same http/https/mailto allowlist as `utils/openUrl.ts`
	 *  — this module does not re-validate the scheme. */
	openUrl: (url: string) => void;
	openFile: (path: string, line?: number, col?: number) => void;
	/** `run_command` may auto-submit (subject to `shouldAutoSubmitSuggestion`);
	 *  `send_text` never does — the caller decides via `autoSubmitAllowed`. */
	sendText: (text: string, autoSubmitAllowed: boolean) => Promise<void>;
	runInNewTerminal: (text: string) => Promise<void>;
	askAi: (text: string) => void;
	/** Called instead of `openUrl` when the parameter's scheme isn't
	 *  allowlisted, so the user gets visible feedback (a toast) rather than
	 *  the silent console warning `handleOpenUrl` itself logs. */
	onBlockedUrl: (url: string) => void;
}

const ALLOWED_URL_SCHEMES = new Set(["http:", "https:", "mailto:"]);

function isAllowedUrl(url: string): boolean {
	try {
		return ALLOWED_URL_SCHEMES.has(new URL(url).protocol);
	} catch {
		return false;
	}
}

/**
 * Run one smart-selection action: substitute its parameter template, then
 * dispatch to the matching dep. `\0`-etc. substitution happens here (not in
 * the caller) so every action kind gets it uniformly, including the ones
 * that pass the parameter straight through as literal text (`copy`).
 */
export async function runSmartSelectionAction(
	action: SmartSelectionAction,
	ctx: SmartSelectionActionContext,
	deps: SmartSelectionActionDeps,
): Promise<void> {
	const parameter = substituteActionParameter(action.parameter, {
		match: ctx.matchText,
		groups: ctx.groups,
		cwd: ctx.cwd,
		user: ctx.user,
		host: ctx.host,
	});

	switch (action.kind) {
		case "copy":
			await deps.copyToClipboard(parameter);
			return;
		case "open_url":
			if (isAllowedUrl(parameter)) {
				deps.openUrl(parameter);
			} else {
				deps.onBlockedUrl(parameter);
			}
			return;
		case "open_file":
			deps.openFile(parameter);
			return;
		case "send_text":
			await deps.sendText(parameter, false);
			return;
		case "run_command":
			await deps.sendText(parameter, true);
			return;
		case "run_command_new_terminal":
			await deps.runInNewTerminal(parameter);
			return;
		case "ask_ai":
			deps.askAi(parameter);
			return;
	}
}
