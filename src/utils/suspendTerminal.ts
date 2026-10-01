import { appLogger } from "../stores/appLogger";
import { type TerminalData, terminalsStore } from "../stores/terminals";
import { rpc } from "../transport";
import { verifyAndBuildResumeCommand } from "./agentSession";
import { clearShellFamilyCache } from "./sendCommand";

export type SuspendOutcome = { ok: true } | { ok: false; reason: string };

/** Why this tab cannot be suspended right now, or null. Ending a PTY mid-turn
 *  would cut the agent's work or an unanswered question, so busy tabs are refused. */
export function suspendRefusal(term: TerminalData): string | null {
	if (term.suspended) return "already suspended";
	if (!term.sessionId) return "no live session";
	if (term.awaitingInput) return "waiting for input";
	if (term.queuedCommands > 0) return "queued commands pending";
	if (term.agentType) {
		if (term.backgroundWork || term.agentState === "working" || term.agentState === "starting") {
			return "agent working";
		}
	} else if (term.shellState === "busy") {
		return "command running";
	}
	return null;
}

function resumeCommandFor(term: TerminalData): Promise<string | null> {
	if (!term.agentType) return Promise.resolve(null);
	return verifyAndBuildResumeCommand(
		term.agentType,
		term.cwd,
		term.tuicSession,
		term.agentSessionId,
		term.agentLaunchCommand,
	);
}

/** End the tab's PTY (and its agent) but keep the tab, in the state a restart restores it in.
 *  The tab keeps agentType, agentSessionId, tuicSession, alias and cwd; `suspended` makes
 *  it persist across a restart and stops its Terminal from spawning a new PTY. */
export async function suspendTerminal(id: string): Promise<SuspendOutcome> {
	const initial = terminalsStore.get(id);
	if (!initial) return { ok: false, reason: "unknown tab" };
	const refusal = suspendRefusal(initial);
	if (refusal) return { ok: false, reason: refusal };

	// An agent tab whose session cannot be resumed would lose the agent for good.
	if (initial.agentType && !(await resumeCommandFor(initial))) {
		return { ok: false, reason: "no resumable agent session" };
	}

	// The verification awaited: the tab may have started a turn or been closed meanwhile.
	const term = terminalsStore.get(id);
	if (!term) return { ok: false, reason: "unknown tab" };
	const recheck = suspendRefusal(term);
	if (recheck) return { ok: false, reason: recheck };
	const sessionId = term.sessionId as string;

	// Before the close, so the tab's exit handler sees a suspend and not an agent exit.
	terminalsStore.update(id, { suspended: true });
	try {
		clearShellFamilyCache(sessionId);
		await rpc("close_pty", { sessionId, cleanupWorktree: false });
	} catch (e) {
		terminalsStore.update(id, { suspended: false });
		appLogger.warn("terminal", "Suspend: closing the PTY failed", { id, error: String(e) });
		return { ok: false, reason: "closing the session failed" };
	}
	terminalsStore.setSessionId(id, null);
	terminalsStore.update(id, {
		shellState: null,
		agentState: null,
		backgroundWork: false,
		standby: false,
		pendingInitCommand: null,
		pendingResumeCommand: null,
	});
	return { ok: true };
}

/** Bring a suspended tab back: a new PTY in the same cwd, and for an agent tab the same
 *  resume command a restart uses, typed when the shell first goes idle. */
export async function resumeTerminal(id: string): Promise<SuspendOutcome> {
	const term = terminalsStore.get(id);
	if (!term?.suspended) return { ok: false, reason: "not suspended" };
	const command = await resumeCommandFor(term);
	if (term.agentType && !command) {
		appLogger.warn("terminal", "Resume: no resumable agent session, opening a plain shell", { id });
	}
	terminalsStore.update(id, { suspended: false, pendingInitCommand: command });
	return { ok: true };
}
