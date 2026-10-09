import { appLogger } from "../stores/appLogger";
import { rpc } from "../transport";

/** Shell family classification from the Rust PTY layer.
 *  Serialized as kebab-case to match `serde(rename_all = "kebab-case")`. */
export type ShellFamily = "posix" | "windows-native" | "unknown";

/** Per-session cache of the resolved shell family. Queried once on first
 *  use and reused afterwards — the shell doesn't change mid-session. */
const shellFamilyCache = new Map<string, ShellFamily>();

/** Real-time gap between payload and Enter for every verified agent. Mirrors `INJECT_ENTER_GAP` in `pty.rs`.
 *
 *  That constant's comment used to claim the frontend "gets this gap for free —
 *  its two `writeFn` calls are separate IPC round-trips". It does not: a Tauri
 *  IPC round-trip completes far inside the child's read-scheduling latency, so
 *  both writes routinely land in one `read()` and the agent renders a newline
 *  instead of submitting. Separate flushes never guaranteed separate reads —
 *  only elapsed time does. */
export const AGENT_ENTER_GAP_MS = 50;
/** Gap for an agent type whose input semantics are not verified. Mirrors
 *  `UNVERIFIED_ENTER_GAP` in `pty.rs`. */
export const UNVERIFIED_ENTER_GAP_MS = 200;

/** Codex ingests a long plain write as a paste burst for hundreds of
 *  milliseconds, and an Enter inside that burst is swallowed (measured live on
 *  0.159.0: 1000 chars at a 200ms gap stay in the composer). No fixed gap
 *  covers a payload whose ingestion time grows with its length, so a long
 *  Codex payload is a bracketed paste. Shorter text stays plain keystrokes:
 *  Codex approval and choice overlays ignore a paste event. Mirrors
 *  `CODEX_PASTE_FRAME_MIN_CHARS` in `pty.rs`. */
export const CODEX_PASTE_FRAME_MIN_CHARS = 500;

// Keep in step with Rust's agent_submit_profile: an unrecognized type must use
// the longer gap until its input semantics are known.
const SHORT_ENTER_GAP_AGENTS = new Set([
	"claude",
	"codex",
	"gemini",
	"opencode",
	"aider",
	"amp",
	"cursor",
	"goose",
	"grok",
	"droid",
	"pi",
]);

const delay = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

/** Keep Enter in its own read after the last input write. */
export function waitForAgentEnterGap(agentType?: string | null): Promise<void> {
	return delay(agentType && SHORT_ENTER_GAP_AGENTS.has(agentType) ? AGENT_ENTER_GAP_MS : UNVERIFIED_ENTER_GAP_MS);
}

/** Fetch (and cache) the shell family for a PTY session. Returns "unknown"
 *  if the backend can't tell us. Informational since `sendCommand` stopped
 *  sending a Ctrl-U (it only decided whether to skip that prefix). */
export async function getShellFamily(sessionId: string): Promise<ShellFamily> {
	const cached = shellFamilyCache.get(sessionId);
	if (cached) return cached;
	try {
		const family = await rpc<ShellFamily | null>("get_session_shell_family", { sessionId });
		const resolved: ShellFamily = family ?? "unknown";
		shellFamilyCache.set(sessionId, resolved);
		return resolved;
	} catch (err) {
		appLogger.warn("terminal", "Failed to query shell family; falling back to platform heuristic", err);
		return "unknown";
	}
}

/** Drop a session's cached entry. Call on session close so a reused
 *  session id (unlikely but possible) doesn't keep a stale classification. */
export function clearShellFamilyCache(sessionId: string): void {
	shellFamilyCache.delete(sessionId);
}

/** Send a command to a PTY session with split writes.
 *
 *  Splits into separate writes:
 *  1. the text (multiline, or a long Codex payload, as a bracketed paste)
 *  2. \r (Enter — sent separately, a real gap later for raw-mode agents)
 *
 *  No Ctrl-U is sent: the text is APPENDED to whatever the input line already
 *  holds, never replaces it (user decision, dropped-items #16). That also
 *  removes the old native-Windows special case (cmd/PowerShell echo a Ctrl-U
 *  literally), so `shellFamily` no longer changes the bytes written.
 *
 *  @param writeFn      Function that writes raw data to the PTY.
 *  @param text         Command text to inject (without trailing newline).
 *  @param agentType    Detected agent in the PTY (null = plain shell).
 *  @param _shellFamily Kept for call-site compatibility; no longer used.
 *  @param submit       When false, the text is typed but the trailing Enter is
 *                      withheld so the user reviews and executes it manually.
 *                      Used by reviewable Smart Prompts and by suggestion chips
 *                      carrying shell metacharacters (spoofable via OSC 7770
 *                      from untrusted output). Default true.
 *  @param sessionId    PTY to probe when the foreground agent type is unknown.
 */
export async function sendCommand(
	writeFn: (data: string) => Promise<void>,
	text: string,
	agentType?: string | null,
	_shellFamily?: ShellFamily,
	submit = true,
	sessionId?: string,
): Promise<void> {
	let unknownForeground = false;
	let foregroundProbeFailed = false;
	if (!agentType && sessionId) {
		try {
			unknownForeground = Boolean(await rpc<string | null>("has_foreground_process", { sessionId }));
		} catch (err) {
			appLogger.warn("terminal", "Failed to identify foreground process; keeping a safe Enter gap", err);
			foregroundProbeFailed = true;
		}
	}
	const agentInput = Boolean(agentType) || unknownForeground;
	const bracketed = text.includes("\n") || (agentType === "codex" && text.length > CODEX_PASTE_FRAME_MIN_CHARS);
	const payload = bracketed ? `\x1b[200~${text}\x1b[201~` : text;
	await writeFn(payload);
	if (!submit) return;
	// Two writes are not two reads. Keep a scheduling gap for raw-mode agents.
	if (agentInput || foregroundProbeFailed) await waitForAgentEnterGap(agentType);
	await writeFn("\r");
}

/** Shell metacharacters that enable command chaining, substitution, redirection,
 *  or embedded newlines (Enter). A suggestion chip (OSC 7770 `suggest=`) can be
 *  emitted by ANY terminal output, including an untrusted file or log the user
 *  merely displayed. When a chip's text contains one of these, the caller should
 *  withhold auto-Enter (`sendCommand(..., submit=false)`) so a click can never
 *  silently execute a chained/redirected command — the user reviews it first.
 *  Plain-prose suggestions ("Run tests", "Fix the bug") contain none of these
 *  and keep the one-click behaviour. This is content-based, NOT a provenance
 *  gate, so legitimate shell-integration suggestions still work. */
const SHELL_METACHARACTERS = /[;&|`$<>\n\r]/;

export function containsShellMetacharacters(text: string): boolean {
	return SHELL_METACHARACTERS.test(text);
}

/** Decide whether a clicked suggestion chip (OSC 7770 `suggest=`) should
 *  auto-submit (send the trailing Enter) or just type the text for review.
 *
 *  The shell-injection guard only applies to SHELLS: in a shell, newlines and
 *  metacharacters (`;&|`, `<>`, `$\``) chain/redirect commands, so an untrusted
 *  chip must not silently execute. For an AGENT (codex, claude, …) a chip is
 *  plain prompt text — nothing is dangerous, and a multi-line prompt MUST submit
 *  (withholding Enter leaves it dangling; Codex then treats a manual Enter as a
 *  newline, not submit). So: agent → always submit; shell → gate on metachars. */
export function shouldAutoSubmitSuggestion(agentType: string | null | undefined, text: string): boolean {
	return agentType ? true : !containsShellMetacharacters(text);
}

/** Send a single raw character/escape sequence to a PTY running a TUI dialog
 *  in raw stdin mode (Claude Code edit-confirm, bash-confirm, apply-patch, ...).
 *
 *  Unlike `sendCommand`, this writes EXACTLY the bytes provided — no trailing
 *  `\r`. Adding one breaks raw-mode dialog parsers:
 *  Claude Code reads one key and interprets trailing bytes as the next prompt,
 *  Codex aborts on unexpected input. This is the intended counterpart to
 *  `sendCommand` for the ChoicePrompt / numbered-option path.
 *
 *  Intentionally a one-liner over `writeFn` so the call site is centralized
 *  (grep-able, uniform logging) and the src/AGENTS.md "never raw text+\r" rule
 *  still routes through a named helper even for single-key writes.
 */
export async function sendPtyKey(writeFn: (data: string) => Promise<void>, key: string): Promise<void> {
	await writeFn(key);
}
