import { invoke } from "../invoke";
import { agentConfigsStore } from "../stores/agentConfigs";
import { appLogger } from "../stores/appLogger";
import { githubStore } from "../stores/github";
import { promptLibraryStore, type SavedPrompt } from "../stores/promptLibrary";
import { repositoriesStore } from "../stores/repositories";
import { settingsStore } from "../stores/settings";
import { terminalsStore } from "../stores/terminals";
import type { EgoTurn } from "../types/acp";
import { writeClipboard } from "../utils/clipboard";
import { prContextVariables } from "../utils/promptContext";
import { resolvePromptTreeIn } from "../utils/repoOwnership";
import { usePty } from "./usePty";

export interface SmartPromptResult {
	ok: boolean;
	reason?: string;
	/** For headless mode: command output. For unresolved_variables: JSON array of variable names. */
	output?: string;
}

export interface CanExecuteResult {
	ok: boolean;
	reason?: string;
}

/** Translate a failed SmartPromptResult into a user-friendly message — shared by
 *  every surface that shows execution failures to the user (SmartButtonStrip's
 *  onError, PromptDrawer's toast) so the wording never drifts between them. */
export function friendlyError(result: { reason?: string; output?: string }, promptName: string): string {
	if (result.reason === "unresolved_variables" && result.output) {
		try {
			const vars = JSON.parse(result.output) as string[];
			if (vars.includes("staged_diff")) return "Stage some files first — no staged changes to analyze";
			return `Missing context: ${vars.join(", ")}`;
		} catch {
			/* fall through */
		}
	}
	return result.reason ?? `"${promptName}" failed`;
}

/**
 * Minimal shell-word splitter for headless templates.
 * Respects single and double quotes; backslash escapes the next char (outside single quotes).
 * Does NOT perform variable expansion, command substitution, or globbing — those would
 * re-introduce the injection vector we are removing. Metacharacters like `;` and backticks
 * are treated as literal characters inside the resulting argv tokens.
 */
export function shellSplit(input: string): string[] {
	const tokens: string[] = [];
	let cur = "";
	let quote: '"' | "'" | null = null;
	let hasToken = false;
	let escaped = false;
	for (const ch of input) {
		if (escaped) {
			cur += ch;
			escaped = false;
			hasToken = true;
			continue;
		}
		if (quote === "'") {
			if (ch === "'") {
				quote = null;
			} else {
				cur += ch;
			}
			hasToken = true;
			continue;
		}
		if (quote === '"') {
			if (ch === '"') {
				quote = null;
			} else if (ch === "\\") {
				escaped = true;
			} else {
				cur += ch;
			}
			hasToken = true;
			continue;
		}
		if (ch === "'" || ch === '"') {
			quote = ch;
			hasToken = true;
			continue;
		}
		if (ch === "\\") {
			escaped = true;
			continue;
		}
		if (/\s/.test(ch)) {
			if (hasToken) {
				tokens.push(cur);
				cur = "";
				hasToken = false;
			}
			continue;
		}
		cur += ch;
		hasToken = true;
	}
	if (hasToken) tokens.push(cur);
	return tokens;
}

interface ResolvedAgent {
	agent: string | null;
}

/** Whether api mode has a model to run on.
 *
 * TUICommander holds no provider registry and makes no provider call any more
 * (#784-0aec): ego owns the model, and api mode is one unattended ego turn. So
 * the only thing that can be checked without launching anything is whether ego
 * itself is named. Which model it will use is ego's own state, and asking would
 * mean running ego to find out whether ego can be run.
 *
 * The refusal names both steps in order. The binary is a TUICommander setting
 * on General; the model is ego's and is set on the AI Chat page, which replaced
 * the deleted provider registry. */
function canExecuteApi(): CanExecuteResult {
	if (!settingsStore.isAcpConfigured()) {
		return {
			ok: false,
			reason: "ego is not configured — name the binary in Settings → General, then pick a model in Settings → AI Chat",
		};
	}
	// ACP gives a session one working directory and it must be a real one. An
	// empty path would reach ego as a session it refuses to open, which reads as
	// an ego fault rather than as "open a repository first".
	if (!apiRoot()) {
		return { ok: false, reason: "No repository open — ego needs a working directory to run in" };
	}
	return { ok: true };
}

/** The directory an unattended turn runs in: the active terminal's, else the
 *  active repository's. The same fallback `executeHeadless` uses, so the two
 *  modes cannot disagree about where a prompt ran. */
function apiRoot(): string {
	return terminalsStore.getActive()?.cwd ?? repositoriesStore.getActive()?.path ?? "";
}

/** Resolve where an inject-mode prompt's text goes: the Compose box for review,
 *  or straight to the terminal input. An explicit `injectTarget` always wins;
 *  "auto" (and unset, its equivalent) adapts to whether Compose happens to be
 *  open right now — reviewing in an already-open Compose panel makes sense,
 *  but popping one open just to hold text the user didn't ask to review does
 *  not, so a closed Compose panel routes to the terminal instead. */
export function resolveInjectTarget(prompt: SavedPrompt, composeIsOpen: boolean): "compose" | "terminal" {
	const target = prompt.injectTarget ?? "auto";
	if (target === "auto") return composeIsOpen ? "compose" : "terminal";
	return target;
}

/** Resolve whether an inject-mode prompt submits after insertion.
 *
 * Explicit UI actions win: Insert always withholds Enter, while Insert & Run
 * and double-click always submit. Otherwise the persisted autoExecute flag is
 * authoritative. Absent that, submission follows the resolved target: terminal
 * submits, compose remains editable. */
export function shouldSubmitInjectPrompt(
	prompt: SavedPrompt,
	composeIsOpen: boolean,
	submitOverride?: boolean,
): boolean {
	return submitOverride ?? prompt.autoExecute ?? resolveInjectTarget(prompt, composeIsOpen) === "terminal";
}

function resolveHeadlessAgent(prompt: SavedPrompt): ResolvedAgent {
	const preferred = prompt.preferredAgent;
	const global = agentConfigsStore.getHeadlessAgent();

	if (preferred) {
		if (preferred === "api") return { agent: "api" };
		const template = agentConfigsStore.getHeadlessTemplate(preferred);
		if (template) return { agent: preferred };
		appLogger.warn("prompts", `Preferred agent "${preferred}" has no template, falling back to global`);
	}

	return { agent: global ?? null };
}

/** Route headless output to the appropriate destination */
export function routeHeadlessOutput(prompt: SavedPrompt, output: string): void {
	if (!output) return;
	switch (prompt.outputTarget) {
		case "clipboard":
			writeClipboard(output).then(
				() => appLogger.info("prompts", `"${prompt.name}" output copied to clipboard`),
				(err) => appLogger.error("prompts", `Failed to copy to clipboard`, err),
			);
			break;
		case "toast":
			appLogger.info("prompts", `${prompt.name}: ${output.slice(0, 500)}`);
			break;
		case "commit-message":
			// Emit a custom event that the Git Panel commit textarea can listen to
			window.dispatchEvent(new CustomEvent("smart-prompt:commit-message", { detail: output }));
			appLogger.info("prompts", `"${prompt.name}" output sent to commit message`);
			break;
		case "panel":
			// For now, log the output — a dedicated panel can be added later
			appLogger.info("prompts", `${prompt.name} result:\n${output.slice(0, 2000)}`);
			break;
		default:
			// No routing — output is available in the SmartPromptResult
			break;
	}
}

export function useSmartPrompts() {
	const pty = usePty();

	/** Check if a smart prompt can be executed right now */
	/** `submitOverride`: the caller's explicit submit choice (a double-click, the
	 *  variable dialog's Execute) — it must reach the busy gate, or an
	 *  `autoExecute: false` prompt submitted that way skips it. */
	function canExecute(prompt: SavedPrompt, submitOverride?: boolean): CanExecuteResult {
		if (prompt.enabled === false) return { ok: false, reason: "Prompt is disabled" };

		if (prompt.executionMode === "shell") {
			return { ok: true };
		}

		if (prompt.executionMode === "api") {
			return canExecuteApi();
		}

		if (prompt.executionMode === "headless") {
			const resolved = resolveHeadlessAgent(prompt);
			// "api" is a headless agent the same way it is a mode: the work is one
			// ego turn either way, so it answers to the same check.
			if (resolved.agent === "api") return canExecuteApi();
			if (!resolved.agent)
				return { ok: false, reason: "No headless agent configured — set one in Settings → Smart Prompts" };
			return { ok: true };
		}

		return canExecuteInject(prompt, submitOverride);
	}

	function canExecuteInject(prompt: SavedPrompt, submitOverride?: boolean): CanExecuteResult {
		const active = terminalsStore.getActive();
		if (!active?.sessionId) return { ok: false, reason: "No active terminal" };
		if (!active.agentType) return { ok: false, reason: "No agent detected in terminal" };
		const composeIsOpen = active.ref?.isComposeOpen?.() ?? false;
		// Idle only matters when this action will submit. Review-only insertions do
		// not steer the active turn, regardless of their preferred UI target.
		if (shouldSubmitInjectPrompt(prompt, composeIsOpen, submitOverride) && prompt.requiresIdle !== false) {
			// isBusy, not isWorking: declared background work (teammates, bg tasks) must not
			// block a prompt into the main agent, which is free to take it.
			const busy = terminalsStore.isBusy(active.id);
			if (busy) return { ok: false, reason: "Agent is busy" };
		}
		return { ok: true };
	}

	/** Frontend-only variables (GitHub PR, agent/terminal) — no IPC needed.
	 *  `branch`, when given, is the branch the tree the variables are being
	 *  resolved against actually has checked out (`PromptTree.branchName`,
	 *  derived fresh from the terminal's cwd) — preferred over
	 *  `repo.activeBranch`, which is a separately-maintained pointer that
	 *  several focus-switch paths (cross-pane Alt+Arrow, closing a pane) are
	 *  known to leave stale for a non-root worktree. `undefined`/`null` falls
	 *  back to `activeBranch`, which is still the right signal for the repo
	 *  ROOT itself — there's no per-worktree branch to derive there; whatever
	 *  is checked out at the root can change at any time. */
	function resolveFrontendVars(repoPath: string, branch?: string | null): Record<string, string> {
		const vars: Record<string, string> = {};
		const repo = repositoriesStore.get(repoPath);
		const resolvedBranch = branch ?? repo?.activeWorkspaceId ?? "";
		if (resolvedBranch) {
			const pr = githubStore.getBranchPrData(repoPath, resolvedBranch);
			if (pr) Object.assign(vars, prContextVariables(pr));
		}
		const activeTerminal = terminalsStore.getActive();
		if (activeTerminal?.agentType) {
			vars["agent_type"] = activeTerminal.agentType;
		}
		if (activeTerminal?.cwd) {
			vars["cwd"] = activeTerminal.cwd;
		}
		return vars;
	}

	/** Resolve all context variables (git + frontend). */
	async function resolveAllVariables(repoPath: string): Promise<Record<string, string>> {
		const vars = await promptLibraryStore.resolveVariables(repoPath);
		return { ...vars, ...resolveFrontendVars(repoPath) };
	}

	/** Execute a smart prompt via inject or headless mode.
	 *
	 *  `targetPath`, when given, overrides which tree (worktree or repo root)
	 *  variables resolve against AND which directory a shell/headless prompt
	 *  actually runs in — for a caller that is itself bound to a specific
	 *  repo/worktree independent of terminal focus (e.g. a Git panel showing
	 *  `props.repoPath`, which need not be the currently active terminal's
	 *  repo). Omitted, this falls back to the active terminal's cwd, as before. */
	async function executeSmartPrompt(
		prompt: SavedPrompt,
		manualVariables?: Record<string, string>,
		targetPath?: string,
	): Promise<SmartPromptResult> {
		const check = canExecute(prompt);
		if (!check.ok) {
			appLogger.warn("prompts", `Cannot execute "${prompt.name}": ${check.reason}`);
			return check;
		}

		const effectiveMode = prompt.executionMode ?? "inject";

		// Resolve variables against the tree (worktree or repo root) that owns
		// `targetPath` when the caller supplied one, else the ACTIVE
		// TERMINAL's cwd — not just "the active repo" (the
		// active-repo-vs-worktree-cwd bug: {branch}/{diff} used to describe
		// the main checkout while the command actually ran in the worktree).
		// Falls back to today's active-repo behavior when neither resolves
		// against a registered repo, so a plain shell in an unregistered
		// directory keeps working exactly as before.
		const activeTerminal = terminalsStore.getActive();
		const tree = resolvePromptTreeIn(targetPath ?? activeTerminal?.cwd, repositoriesStore.state.repositories);
		const activeRepo = repositoriesStore.getActive();
		const repoRoot = tree?.repoPath ?? activeRepo?.path ?? "";
		const varsPath = tree?.treePath ?? repoRoot;

		// Single IPC: extract needed variable names + resolve only those from git.
		const { vars: gitVars, needed: varNames } = await invoke<{ vars: Record<string, string>; needed: string[] }>(
			"resolve_prompt_variables",
			{ content: prompt.content, repoPath: varsPath || null },
		);
		// resolveFrontendVars takes the REPO ROOT specifically, never a
		// worktree path — repositoriesStore is keyed by repo root, so
		// passing varsPath here would make its `repositoriesStore.get(...)`
		// lookup silently miss and drop every pr_* variable. tree?.branchName
		// (derived fresh from the terminal's cwd) is passed through too —
		// without it, resolveFrontendVars falls back to repo.activeBranch,
		// a separately-maintained pointer that cross-pane focus switches
		// (Alt+Arrow, closing a pane) can leave stale for a worktree that
		// isn't the repo root, reintroducing a narrower version of the same
		// active-repo-vs-worktree-cwd bug for pr_* variables specifically.
		const allVars = {
			...gitVars,
			...resolveFrontendVars(repoRoot, tree?.branchName),
			...manualVariables,
		};
		const unresolved = varNames.filter((v) => !(v in allVars));
		if (unresolved.length > 0) {
			return { ok: false, reason: "unresolved_variables", output: JSON.stringify(unresolved) };
		}

		// Substitute variables into content. In shell mode, values go through
		// shell-quoting so repo-controlled variables (branch/pr_*/commit_log) can't
		// escape their argument inside `sh -c` / `cmd /C`.
		const processed = await promptLibraryStore.processContent(prompt, allVars, {
			shellSafe: effectiveMode === "shell",
		});

		if (effectiveMode === "shell") {
			return executeShell(prompt, processed, targetPath ? varsPath : undefined);
		}
		if (effectiveMode === "api") {
			return executeApi(prompt, processed, targetPath ? varsPath : undefined);
		}
		if (effectiveMode === "headless") {
			return executeHeadless(prompt, processed, targetPath ? varsPath : undefined);
		}
		return executeInject(prompt, processed);
	}

	async function executeInject(
		prompt: SavedPrompt,
		content: string,
		submitOverride?: boolean,
	): Promise<SmartPromptResult> {
		const active = terminalsStore.getActive();
		if (!active?.sessionId) return { ok: false, reason: "No active terminal" };

		try {
			const composeIsOpen = active.ref?.isComposeOpen?.() ?? false;
			const target = resolveInjectTarget(prompt, composeIsOpen);
			const submit = shouldSubmitInjectPrompt(prompt, composeIsOpen, submitOverride);
			if (!submit && target === "compose" && active.ref?.openComposeWithText) {
				// Fill the compose box; the user reviews and sends.
				active.ref.openComposeWithText(content);
			} else {
				// Submission and the PTY fallback for review both use the central helper.
				// submit=false keeps the text editable while retaining platform-specific
				// line clearing and bracketed-paste behavior.
				await pty.sendCommand(active.sessionId, content, active.agentType, submit);
			}
			promptLibraryStore.markAsUsed(prompt.id);
			return { ok: true };
		} catch (err) {
			appLogger.error("prompts", `Failed to inject prompt "${prompt.name}"`, err);
			return { ok: false, reason: String(err) };
		}
	}

	/** One unattended ego turn, the api-mode counterpart of `executeHeadless`.
	 *
	 * The same shape on purpose: one call that returns the whole answer, then
	 * `routeHeadlessOutput`. Nothing streams, because there is nowhere to stream
	 * to — a Smart Prompt runs with no panel open. */
	async function executeApi(prompt: SavedPrompt, content: string, rootOverride?: string): Promise<SmartPromptResult> {
		try {
			// Same override `executeHeadless` honours, so a caller-supplied
			// `targetPath` moves both unattended modes to the same directory.
			const root = rootOverride ?? apiRoot();
			const turn = await invoke<EgoTurn>("acp_one_shot_prompt", { root, prompt: content });
			promptLibraryStore.markAsUsed(prompt.id);

			// An empty answer after a refused question is not an empty answer. The
			// model reached for a tool an unattended turn cannot grant, and
			// reporting "it returned nothing" would send the reader to the prompt.
			if (!turn.text && turn.declined > 0) {
				const reason = `ego asked for ${turn.declined} permission${turn.declined === 1 ? "" : "s"} this mode cannot grant, so the turn produced nothing`;
				appLogger.warn("prompts", `"${prompt.name}": ${reason}`);
				return { ok: false, reason };
			}

			routeHeadlessOutput(prompt, turn.text);
			return { ok: true, output: turn.text };
		} catch (err) {
			appLogger.error("prompts", `API execution failed for "${prompt.name}"`, err);
			return { ok: false, reason: String(err) };
		}
	}

	async function executeHeadless(
		prompt: SavedPrompt,
		content: string,
		repoPathOverride?: string,
	): Promise<SmartPromptResult> {
		const resolved = resolveHeadlessAgent(prompt);
		const headlessVal = resolved.agent;
		// The "api" headless agent is the same unattended ego turn as api mode.
		// Two paths to one behaviour would be two places to fix it.
		if (headlessVal === "api") return executeApi(prompt, content, repoPathOverride);
		if (!headlessVal) {
			return { ok: false, reason: "No headless agent configured — set one in Settings → Smart Prompts" };
		}

		// Resolve headless_agent: "type:configName" format from grouped dropdown, or plain agent type.
		// Prompt content is sent via stdin — {prompt} tokens in args/templates are dropped, never
		// interpolated. Args are passed as a structured argv array (no shell) to eliminate injection.
		let command: string | undefined;
		let args: string[] = [];
		let envVars: Record<string, string> | undefined;
		let fallbackTemplate: string | undefined;
		if (headlessVal.includes(":")) {
			// Run config selected — parse "agentType:configName". Split on the FIRST
			// colon only: `str.split(":", 2)` is NOT "split into at most 2 parts" in
			// JS — it splits on every colon and then truncates the result array to 2
			// elements, silently dropping anything after a second colon in the config
			// name itself (nothing prevents a colon in a run config's name).
			const colonIdx = headlessVal.indexOf(":");
			const agentType = headlessVal.slice(0, colonIdx);
			const configName = headlessVal.slice(colonIdx + 1);
			const configs = agentConfigsStore.getRunConfigs(agentType as import("../agents").AgentType);
			const cfg = configs.find((c) => c.name === configName);
			if (cfg) {
				command = cfg.command;
				args = cfg.args.filter((a) => a !== "{prompt}");
				envVars = Object.keys(cfg.env).length > 0 ? cfg.env : undefined;
			} else {
				// Config not found, fall back to agent type template
				fallbackTemplate = agentConfigsStore.getHeadlessTemplate(agentType as import("../agents").AgentType);
			}
		} else {
			fallbackTemplate = agentConfigsStore.getHeadlessTemplate(headlessVal as import("../agents").AgentType);
		}

		if (!command && fallbackTemplate) {
			const tokens = shellSplit(fallbackTemplate).filter((t) => t !== "{prompt}");
			command = tokens[0];
			args = tokens.slice(1);
		}

		if (!command) {
			return { ok: false, reason: "No headless template found for the configured agent" };
		}

		const active = terminalsStore.getActive();
		const repoPath = repoPathOverride ?? active?.cwd ?? repositoriesStore.getActive()?.path ?? "";
		try {
			const output = await invoke<string>("execute_headless_prompt", {
				command,
				args,
				stdinContent: content,
				timeoutMs: 300000,
				repoPath,
				env: envVars,
			});
			promptLibraryStore.markAsUsed(prompt.id);

			// Route output based on prompt's outputTarget
			routeHeadlessOutput(prompt, output);

			return { ok: true, output };
		} catch (err) {
			appLogger.error("prompts", `Headless execution failed for "${prompt.name}"`, err);
			return { ok: false, reason: String(err) };
		}
	}

	async function executeShell(
		prompt: SavedPrompt,
		content: string,
		repoPathOverride?: string,
	): Promise<SmartPromptResult> {
		const active = terminalsStore.getActive();
		const repoPath = repoPathOverride ?? active?.cwd ?? repositoriesStore.getActive()?.path ?? "";
		try {
			const output = await invoke<string>("execute_shell_script", {
				scriptContent: content,
				timeoutMs: 60000,
				repoPath,
			});
			promptLibraryStore.markAsUsed(prompt.id);
			routeHeadlessOutput(prompt, output);
			return { ok: true, output };
		} catch (err) {
			appLogger.error("prompts", `Shell execution failed for "${prompt.name}"`, err);
			return { ok: false, reason: String(err) };
		}
	}

	return {
		canExecute,
		executeSmartPrompt,
		resolveAllVariables,
	};
}
