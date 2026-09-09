import { AGENTS, type AgentType } from "../agents";
import { agentConfigsStore } from "../stores/agentConfigs";
import { rpc } from "../transport";
import { pathBasename } from "./pathUtils";

/**
 * Apply the agent's default run config to a resume command.
 *
 * The AGENTS table hardcodes the binary (e.g. "claude --resume <id>"), but the
 * user may have configured a custom command/args in their run config (e.g. a
 * "c2" wrapper with "--model claude-opus-4-6"). This helper strips the original
 * binary, swaps in the run config's command, and appends the run config's args
 * AFTER the resume flag so model/profile settings apply to the resumed session.
 *
 * Example: "claude --resume abc" + run config (c2, --model opus) →
 *          "c2 --resume abc --model opus"
 *
 * Returns the original command unchanged when there's no run config (keeps
 * fallback behaviour; used by tests that don't set up the store).
 */
/**
 * Apply a run config to a resume command, preferring the original launch command
 * over the current default. This ensures that e.g. resuming a session started
 * with `c` (claude with custom flags/config-dir) doesn't switch to `c2`.
 *
 * The launch command may carry a leading `KEY=VALUE` env prefix — discovery
 * rebuilds it from the live agent process, because a shell alias hides the env
 * that decides where the session is stored. The prefix stays in front of the
 * binary; the resume flags still go between the binary and its args.
 */
function applyDefaultRunConfig(agentType: AgentType, command: string, launchCommand?: string | null): string {
	const launch = launchCommand ? splitEnvPrefix(launchCommand) : null;
	const runConfig = launch ? null : agentConfigsStore.getDefaultConfig(agentType);
	if (!launch && !runConfig) return command;

	const resumeFlags = tokenize(command).slice(1); // drop the hardcoded binary

	if (launch) {
		// Use the original launch binary + its args, with resume flags in between
		const [launchBinary, ...launchArgs] = launch.argv;
		return [...launch.env, launchBinary, ...resumeFlags, ...launchArgs].join(" ");
	}
	// eslint-disable-next-line @typescript-eslint/no-non-null-assertion
	return [runConfig!.command, ...resumeFlags, ...runConfig!.args].join(" ");
}

/**
 * Resolve the environment a persisted launch command runs under.
 *
 * A rebuilt launch command states it outright (`CLAUDE_CONFIG_DIR=… claude …`) —
 * that is ground truth read from the agent process, and the only source that
 * survives the process. Otherwise fall back to the run config, which knows the
 * env only when the user typed it into TUIC rather than into a shell alias.
 */
function resolveLaunchEnv(agentType: AgentType, launchCommand?: string | null): Record<string, string> {
	const prefix = launchCommand ? splitEnvPrefix(launchCommand).env : [];
	if (prefix.length > 0) {
		return Object.fromEntries(
			prefix.map((assignment) => {
				const eq = assignment.indexOf("=");
				return [assignment.slice(0, eq), unquoteShellValue(assignment.slice(eq + 1))];
			}),
		);
	}
	const config = launchCommand
		? agentConfigsStore
				.getRunConfigs(agentType)
				.find((candidate) => [candidate.command, ...candidate.args].join(" ") === launchCommand)
		: agentConfigsStore.getDefaultConfig(agentType);
	return config?.env ?? {};
}

/** Split a command into tokens, keeping single-quoted spans (paths with spaces) whole. */
function tokenize(command: string): string[] {
	const tokens: string[] = [];
	let current = "";
	let quoted = false;
	let started = false;
	for (const ch of command) {
		if (ch === "'") {
			quoted = !quoted;
			current += ch;
			started = true;
		} else if (ch === " " && !quoted) {
			if (started) tokens.push(current);
			current = "";
			started = false;
		} else {
			current += ch;
			started = true;
		}
	}
	if (started) tokens.push(current);
	return tokens;
}

/** Separate a command's leading `KEY=VALUE` assignments from the command itself. */
function splitEnvPrefix(command: string): { env: string[]; argv: string[] } {
	const tokens = tokenize(command);
	const cut = tokens.findIndex((token) => !/^[A-Za-z_][A-Za-z0-9_]*=/.test(token));
	const at = cut === -1 ? tokens.length : cut;
	return { env: tokens.slice(0, at), argv: tokens.slice(at) };
}

/** Undo POSIX single-quoting (`'a'\''b'` → `a'b`), which is how the backend quotes. */
function unquoteShellValue(value: string): string {
	if (!value.includes("'")) return value;
	let out = "";
	let quoted = false;
	for (let i = 0; i < value.length; i++) {
		const ch = value[i];
		if (ch === "'") {
			quoted = !quoted;
		} else if (ch === "\\" && !quoted && value[i + 1] === "'") {
			out += "'";
			i++;
		} else {
			out += ch;
		}
	}
	return out;
}

/**
 * Build the launch command for an agent, injecting --session-id when applicable.
 *
 * Only Claude Code supports --session-id. For other agents the command is returned unchanged.
 * The command string may include a binary path and extra args (e.g. "claude --model opus").
 *
 * When `agentType` is provided, it takes precedence over the binary-name heuristic.
 * This is important for custom commands (aliases, wrappers) like "C2" that don't
 * contain "claude" in the name but still need --session-id injection.
 */
export function buildAgentLaunchCommand(
	command: string,
	agentSessionId?: string | null,
	agentType?: AgentType | null,
): string {
	if (!agentSessionId) return command;

	const parts = command.split(" ");
	const binary = parts[0];
	const binaryName = pathBasename(binary) ?? "";

	const isClaude = agentType === "claude" || binaryName.startsWith("claude");
	if (!isClaude) return command;

	// Insert --session-id right after the binary
	const rest = parts.slice(1);
	return [binary, "--session-id", agentSessionId, ...rest].join(" ");
}

/**
 * Build the resume command for restoring an agent session.
 *
 * For Claude Code with a persisted session UUID, returns "claude --resume <uuid>".
 * For all other cases, falls back to the static resumeCommand from AGENTS config.
 */
export function buildResumeCommand(
	agentType: AgentType,
	agentSessionId?: string | null,
	launchCommand?: string | null,
): string | null {
	let base: string | null = null;
	if (agentSessionId) {
		const disc = AGENTS[agentType].sessionDiscovery;
		if (disc) base = disc.resumeWithId(agentSessionId);
	}
	if (base === null) base = AGENTS[agentType].resumeCommand;
	if (base === null) return null;
	return applyDefaultRunConfig(agentType, base, launchCommand);
}

/**
 * Verify a discovered session ID against the agent's local session storage, then
 * build the appropriate resume command.
 *
 * Discovery-backed agents use agentSessionId, which is the ID the agent wrote
 * to disk. TUIC's tab UUID is not a valid substitute unless an agent explicitly
 * supports forced binding (those agents do not expose sessionDiscovery).
 * Falls back gracefully when verify_agent_session is unavailable (browser mode).
 */
export async function verifyAndBuildResumeCommand(
	agentType: AgentType,
	cwd: string | null,
	tuicSession?: string | null,
	agentSessionId?: string | null,
	launchCommand?: string | null,
): Promise<string | null> {
	const disc = AGENTS[agentType].sessionDiscovery;

	const sessionId = disc ? agentSessionId : (tuicSession ?? agentSessionId);

	if (sessionId && cwd && disc) {
		try {
			// At restore time the agent process has exited, so agentPid is null.
			// Preserve profile-root overrides such as CODEX_HOME.
			const exists = await rpc<boolean>("verify_agent_session", {
				agentType,
				sessionId,
				cwd,
				agentPid: null,
				envOverrides: resolveLaunchEnv(agentType, launchCommand),
			});
			if (exists) {
				const cmd = disc.resumeWithId(sessionId);
				return applyDefaultRunConfig(agentType, cmd, launchCommand);
			}
			return null;
		} catch {
			// verify_agent_session unavailable (browser mode) — fall through
		}
	}

	// No verified session — fall back to static resumeCommand
	return buildResumeCommand(agentType, agentSessionId, launchCommand);
}
