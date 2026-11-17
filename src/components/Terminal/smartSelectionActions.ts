import {
	buildRunCommandScript,
	hasControlBytes,
	MAX_TYPED_RUN_LINE_BYTES,
	substituteActionParameter,
	typedRunLine,
} from "./smartSelection";
import { canonicalAction, type SmartSelectionAction } from "./smartSelectionTypes";

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
	/** `send_text` with Press Enter: when safe asks to auto-submit (subject to the caller's
	 *  `shouldAutoSubmitSuggestion`); with Press Enter: never it does not — the caller decides via
	 *  `autoSubmitAllowed`. */
	sendText: (text: string, autoSubmitAllowed: boolean) => Promise<void>;
	runInNewTerminal: (text: string) => Promise<void>;
	/** `run_background` ("Run Command"), in the background: run `command` — POSIX `sh` syntax in which
	 *  matched text appears only as `${TUIC_…}` references — from `cwd` ("" = the user's home), with
	 *  `env` holding each referenced variable's value, and no terminal involved. */
	runBackground: (command: string, cwd: string, env: Record<string, string>) => Promise<void>;
	/** `run_background` in a new terminal TYPES a line into that terminal's shell, which is only
	 *  sound for shells that parse POSIX quoting (bash, zsh, ksh, dash, sh). The backend knows the
	 *  shell a new terminal will run; this asks whether it is one of those. */
	newTerminalShell: () => Promise<{ shell: string; supported: boolean }>;
	/** Called instead of opening the terminal when `newTerminalShell` says the shell is not one of
	 *  them, so the user is told why nothing happened and what to do. */
	onUnsupportedNewTerminalShell: (shell: string) => void;
	/** Called instead of opening the terminal when the line to type can't be typed safely: a value
	 *  holds a control byte (`control_bytes`: the shell's line editor would act on it) or the line is
	 *  longer than a not-yet-ready shell reliably accepts (`too_long`). */
	onNewTerminalRefused: (reason: "control_bytes" | "too_long") => void;
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
	saved: SmartSelectionAction,
	ctx: SmartSelectionActionContext,
	deps: SmartSelectionActionDeps,
): Promise<void> {
	// The older `run_command` is `send_text` + "when safe": one code path for both.
	const action = canonicalAction(saved);
	const substitution = {
		match: ctx.matchText,
		groups: ctx.groups,
		cwd: ctx.cwd,
		user: ctx.user,
		host: ctx.host,
	};
	// Plain substitution is for kinds whose result is data or text for a person to read. Run
	// Command never uses it (its text goes through `buildRunCommandScript`), so don't touch the
	// matched text with it there.
	const parameter = action.kind === "run_background" ? "" : substituteActionParameter(action.parameter, substitution);

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
			// "Press Enter: when safe" asks the caller to apply its usual check; "never" (the
			// default) types the text and leaves it for the user.
			await deps.sendText(parameter, action.sendReturn === "when_safe");
			return;
		case "run_command_new_terminal":
			await deps.runInNewTerminal(parameter);
			return;
		case "run_background": {
			// The matched text is untrusted terminal output (a file name in `ls` output can contain
			// `$(...)`), and a shell is about to run this. So it never goes INTO the command line:
			// each placeholder becomes a variable reference and the values travel separately, as
			// environment variables, which a shell expands as data and never parses. What the
			// template author wrote around the placeholder (quotes, a heredoc) cannot change that.
			const run = buildRunCommandScript(action.parameter, substitution);
			if (action.runIn === "new_terminal") {
				// Typed into the terminal's own shell, so first make sure it is one whose quoting we
				// can rely on for the (quoted) assignments and script in the typed line.
				const { shell, supported } = await deps.newTerminalShell();
				if (!supported) {
					deps.onUnsupportedNewTerminalShell(shell);
					return;
				}
				// Checked after the shell, so an unsupported shell is reported as that first.
				if (hasControlBytes(run.values)) {
					deps.onNewTerminalRefused("control_bytes");
					return;
				}
				const line = typedRunLine(run);
				if (new TextEncoder().encode(line).length > MAX_TYPED_RUN_LINE_BYTES) {
					deps.onNewTerminalRefused("too_long");
					return;
				}
				await deps.runInNewTerminal(line);
			} else {
				await deps.runBackground(run.script, ctx.cwd ?? "", run.values);
			}
			return;
		}
		case "ask_ai":
			deps.askAi(parameter);
			return;
	}
}

/** The backend's answer to "Run Command" (`run_background_command`), as it crosses IPC/HTTP. */
export type BackgroundCommandOutcome =
	| { status: "succeeded" }
	| { status: "running" }
	| { status: "failed"; exit_code: number | null };

/**
 * What to tell the user about a finished background command, or `null` when there is nothing to
 * say: success, and a command still running, are both silent (a GUI tool left open is the normal
 * case, not news). A failure can only say how the command ended, because its output is discarded
 * on purpose (see `background_command.rs`), so it points at the way to see the output instead.
 */
export function backgroundFailureMessage(outcome: BackgroundCommandOutcome): string | null {
	if (outcome.status !== "failed") return null;
	switch (outcome.exit_code) {
		case 127:
			return "The command was not found. It has to be on the PATH the command runs with (your login shell's, for zsh, bash, fish and ksh) — try it in a terminal.";
		case 126:
			return "The command was found but could not be run (is it executable?).";
		case null:
			return "The command was stopped by a signal.";
		default:
			return `The command exited with code ${outcome.exit_code}. Its output is discarded; set “Run” to “In a new terminal” to see it.`;
	}
}
