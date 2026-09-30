/** Real-time gap between the payload write and the Enter write when an agent
 *  is attached. Mirrors `INJECT_ENTER_GAP` in `pty.rs`, which documented the
 *  same 50ms as "verified live against Codex: back-to-back hangs, CR after a
 *  gap submits".
 *
 *  That constant's comment used to claim the frontend "gets this gap for free —
 *  its two `writeFn` calls are separate IPC round-trips". It does not: a Tauri
 *  IPC round-trip completes far inside the child's read-scheduling latency, so
 *  both writes routinely land in one `read()` and the agent renders a newline
 *  instead of submitting. Separate flushes never guaranteed separate reads —
 *  only elapsed time does. */
export const AGENT_ENTER_GAP_MS = 50;

const delay = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

/** Send a command to a PTY session with split writes.
 *
 *  Splits into two writes:
 *  1. text (typed as-is, with no leading control prefix)
 *  2. \r (Enter — sent separately)
 *
 *  No longer sends a leading Ctrl-U before the text. It used to, to clear any
 *  stale/partial input already sitting in the prompt before typing the new
 *  command — desirable in principle for POSIX shells with readline
 *  (bash/zsh/fish), where a bundled Ctrl-U+text write is processed correctly
 *  and does clear the line. But Ink-based agents (Claude Code, Codex, etc.)
 *  don't reliably treat a bundled Ctrl-U as a discrete "clear line" keypress
 *  when it arrives in the same PTY write as the text — the byte lands as
 *  literal, invisible content instead, corrupting the injected text (Claude
 *  Code's own paste sanitizer then eats one Enter press removing it, so a
 *  second Enter is needed to actually submit; the cursor also doesn't end up
 *  where expected, since nothing actually executed a real clear). Rather than
 *  give Ctrl-U its own delayed write (mirroring the Enter split below) to make
 *  it work reliably for Ink agents too, the prefix was removed everywhere it
 *  was auto-sent, on both this transport and every Rust equivalent (MCP HTTP,
 *  the AI agent tool, tuic-cli).
 *
 *  Known behavior change: injecting a command into a POSIX shell prompt that
 *  already has stale/partial text typed into it no longer clears that text
 *  first — the injected text is now appended directly after whatever was
 *  already there, which can produce a garbled command line (and execute it,
 *  if `submit` is true) in the case where the prompt wasn't empty when the
 *  injection fired. Accepted tradeoff — see the fork report in this repo's
 *  history for the decision.
 *
 *  @param writeFn      Function that writes raw data to the PTY.
 *  @param text         Command text to inject (without trailing newline).
 *  @param agentType    Detected agent in the PTY (null = plain shell).
 *  @param submit       When false, the text is typed but the trailing Enter is
 *                      withheld so the user reviews and executes it manually.
 *                      Used by reviewable Smart Prompts and by suggestion chips
 *                      carrying shell metacharacters (spoofable via OSC 7770
 *                      from untrusted output). Default true.
 */
export async function sendCommand(
	writeFn: (data: string) => Promise<void>,
	text: string,
	agentType?: string | null,
	submit = true,
): Promise<void> {
	const payload = text.includes("\n") ? `\x1b[200~${text}\x1b[201~` : text;
	await writeFn(payload);
	if (!submit) return;
	// Two writes are not two reads. An Ink/raw-mode agent only treats the CR as
	// submit when it arrives in a SEPARATE read() from the text; back-to-back
	// writes — even flushed individually — are coalesced by the PTY into one
	// read and the CR is swallowed into the composer as a newline, leaving the
	// command typed but unsent. A plain shell is line-buffered and does not care.
	if (agentType) await delay(AGENT_ENTER_GAP_MS);
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
