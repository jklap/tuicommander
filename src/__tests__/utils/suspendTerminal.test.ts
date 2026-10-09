import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { terminalsStore } from "../../stores/terminals";
import { resumeTerminal, suspendRefusal, suspendTerminal } from "../../utils/suspendTerminal";
import { makeTerminal, testInScope } from "../helpers/store";

const mockRpc = vi.fn();
const mockVerifyResume = vi.fn();

vi.mock("../../transport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../transport")>()),
	rpc: (...args: unknown[]) => mockRpc(...args),
}));
vi.mock("../../utils/agentSession", () => ({
	verifyAndBuildResumeCommand: (...args: unknown[]) => mockVerifyResume(...args),
}));

/** A live agent tab: the shape a running Claude tab has in the store. */
function addAgentTab(over: Partial<ReturnType<typeof terminalsStore.get>> = {}): string {
	const id = terminalsStore.add({
		...makeTerminal({ sessionId: "pty-1", cwd: "/Gits/alpha", tuicSession: "tab-uuid", name: "claude tab" }),
		alias: "al-3",
		agentSessionId: "agent-uuid",
	});
	terminalsStore.update(id, { agentType: "claude", agentState: "idle", shellState: "busy", ...over });
	return id;
}

describe("suspendRefusal", () => {
	// Each row is a turn or a question the suspend would have cut silently.
	it.each([
		["a working agent", { agentType: "claude", agentState: "working" }, "agent working"],
		["a starting agent", { agentType: "claude", agentState: "starting" }, "agent working"],
		[
			"an agent with background work",
			{ agentType: "claude", agentState: "idle", backgroundWork: true },
			"agent working",
		],
		["a question awaiting an answer", { agentType: "claude", awaitingInput: "question" }, "waiting for input"],
		["queued compose commands", { agentType: "claude", queuedCommands: 2 }, "queued commands pending"],
		["a plain shell running a command", { agentType: null, shellState: "busy" }, "command running"],
	] as const)("refuses %s", (_name, fields, reason) => {
		testInScope(() => {
			const id = terminalsStore.add(makeTerminal({ sessionId: "pty-x" }));
			terminalsStore.update(id, fields);
			expect(suspendRefusal(terminalsStore.get(id)!)).toBe(reason);
		});
	});

	// An agent TUI keeps its shell "busy" for the whole session, so the shell state
	// must not veto an idle agent — otherwise Suspend is never available for agents.
	it("allows an idle agent although its shell state is busy", () => {
		testInScope(() => {
			const id = addAgentTab();
			expect(suspendRefusal(terminalsStore.get(id)!)).toBeNull();
		});
	});

	it("allows an idle plain shell", () => {
		testInScope(() => {
			const id = terminalsStore.add(makeTerminal({ sessionId: "pty-y" }));
			terminalsStore.update(id, { shellState: "idle" });
			expect(suspendRefusal(terminalsStore.get(id)!)).toBeNull();
		});
	});
});

describe("suspendTerminal", () => {
	beforeEach(() => {
		mockRpc.mockReset().mockResolvedValue(undefined);
		mockVerifyResume.mockReset().mockResolvedValue("claude --resume agent-uuid");
	});

	// A suspend that dropped agentType/agentSessionId/tuicSession/alias would leave a
	// tab that cannot resume the conversation or reclaim its address.
	it("closes the PTY and keeps what the restart path needs to restore the tab", async () => {
		await testInScope(async () => {
			const id = addAgentTab();

			const outcome = await suspendTerminal(id);

			expect(outcome).toEqual({ ok: true });
			expect(mockRpc).toHaveBeenCalledWith("close_pty", { sessionId: "pty-1", cleanupWorktree: false });
			const term = terminalsStore.get(id)!;
			expect(term.suspended).toBe(true);
			expect(term.sessionId).toBeNull();
			expect(term).toMatchObject({
				agentType: "claude",
				agentSessionId: "agent-uuid",
				tuicSession: "tab-uuid",
				alias: "al-3",
				cwd: "/Gits/alpha",
			});
			// No resume banner: the suspended notice owns the resume action.
			expect(term.pendingResumeCommand).toBeNull();
		});
	});

	// Batch 43 review: Suspend cleared the banner fields but not its dedupe key, so
	// after resuming, an exit with the same agent session id was never re-offered.
	it("clears the resume banner's dedupe key so a resumed tab can be offered again", async () => {
		await testInScope(async () => {
			const id = addAgentTab({ resumeOfferedFor: "agent-uuid", pendingResumeTitle: "Fix the build" });

			expect(await suspendTerminal(id)).toEqual({ ok: true });

			const term = terminalsStore.get(id)!;
			expect(term.resumeOfferedFor).toBeNull();
			expect(term.pendingResumeTitle).toBeNull();
		});
	});

	// Cutting a working agent's turn silently is the case the story forbids.
	it("does not close the PTY of a busy agent", async () => {
		await testInScope(async () => {
			const id = addAgentTab({ agentState: "working" });

			const outcome = await suspendTerminal(id);

			expect(outcome).toEqual({ ok: false, reason: "agent working" });
			expect(mockRpc).not.toHaveBeenCalled();
			expect(terminalsStore.get(id)?.suspended).toBe(false);
		});
	});

	// The state can change while the resume command is being verified.
	it("re-checks the tab after verifying the resume command", async () => {
		await testInScope(async () => {
			const id = addAgentTab();
			mockVerifyResume.mockImplementation(async () => {
				terminalsStore.update(id, { agentState: "working" });
				return "claude --resume agent-uuid";
			});

			const outcome = await suspendTerminal(id);

			expect(outcome).toEqual({ ok: false, reason: "agent working" });
			expect(mockRpc).not.toHaveBeenCalled();
		});
	});

	// Ending the PTY of an agent that cannot be resumed loses the conversation for good.
	it("refuses an agent tab with no resumable session", async () => {
		await testInScope(async () => {
			const id = addAgentTab();
			mockVerifyResume.mockResolvedValue(null);

			const outcome = await suspendTerminal(id);

			expect(outcome).toEqual({ ok: false, reason: "no resumable agent session" });
			expect(mockRpc).not.toHaveBeenCalled();
			expect(terminalsStore.get(id)?.sessionId).toBe("pty-1");
		});
	});

	// A failed close must not leave a tab marked suspended while its process still runs.
	it("never marks the tab suspended when closing the session fails", async () => {
		await testInScope(async () => {
			const id = addAgentTab();
			mockRpc.mockRejectedValue(new Error("boom"));

			const outcome = await suspendTerminal(id);

			expect(outcome).toEqual({ ok: false, reason: "closing the session failed" });
			expect(terminalsStore.get(id)?.suspended).toBe(false);
			expect(terminalsStore.get(id)?.sessionId).toBe("pty-1");
		});
	});

	it("suspends a plain shell without asking for a resume command", async () => {
		await testInScope(async () => {
			const id = terminalsStore.add(makeTerminal({ sessionId: "pty-sh", cwd: "/Gits/alpha" }));
			terminalsStore.update(id, { shellState: "idle" });

			expect(await suspendTerminal(id)).toEqual({ ok: true });
			expect(mockVerifyResume).not.toHaveBeenCalled();
			expect(terminalsStore.get(id)).toMatchObject({ sessionId: null, suspended: true });
		});
	});
});

describe("resumeTerminal", () => {
	beforeEach(() => {
		mockVerifyResume.mockReset().mockResolvedValue("claude --resume agent-uuid");
	});

	// The command comes from verifyAndBuildResumeCommand, the function the restore path uses.
	// Resume queues it as pendingInitCommand (typed at the new shell's first idle, no banner)
	// where a restart shows a banner (pendingResumeCommand): clicking Resume is the confirmation.
	// That it is typed exactly once is asserted in Terminal-suspend-resume.test.tsx.
	it("clears the flag and queues the resume command for the new shell", async () => {
		await testInScope(async () => {
			const id = addAgentTab({ suspended: true, sessionId: null });

			expect(await resumeTerminal(id)).toEqual({ ok: true });

			expect(mockVerifyResume).toHaveBeenCalledWith("claude", "/Gits/alpha", "tab-uuid", "agent-uuid", null);
			expect(terminalsStore.get(id)).toMatchObject({
				suspended: false,
				pendingInitCommand: "claude --resume agent-uuid",
			});
		});
	});

	it("reopens a plain shell with no command", async () => {
		await testInScope(async () => {
			const id = terminalsStore.add(makeTerminal({ cwd: "/Gits/alpha" }));
			terminalsStore.update(id, { suspended: true });

			expect(await resumeTerminal(id)).toEqual({ ok: true });
			expect(terminalsStore.get(id)).toMatchObject({ suspended: false, pendingInitCommand: null });
		});
	});

	it("does nothing for a tab that is not suspended", async () => {
		await testInScope(async () => {
			const id = addAgentTab();
			expect(await resumeTerminal(id)).toEqual({ ok: false, reason: "not suspended" });
			expect(terminalsStore.get(id)?.pendingInitCommand).toBeNull();
		});
	});
});

afterEach(() => {
	terminalsStore._testCancelPendingTimers();
});
