import { beforeEach, describe, expect, it, vi } from "vitest";
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

function addTab(over: Partial<NonNullable<ReturnType<typeof terminalsStore.get>>> = {}): string {
	const id = terminalsStore.add({
		...makeTerminal({ sessionId: "pty-c1", cwd: "/Gits/wt/feature", tuicSession: "tab-uuid" }),
		alias: "al-9",
		agentSessionId: "agent-uuid",
	});
	terminalsStore.update(id, {
		agentType: "claude",
		agentState: "idle",
		shellState: "busy",
		agentLaunchCommand: "claude --dangerously-skip-permissions",
		...over,
	});
	return id;
}

describe("suspendTerminal (critic 1358)", () => {
	beforeEach(() => {
		mockRpc.mockReset().mockResolvedValue(undefined);
		mockVerifyResume.mockReset().mockResolvedValue("claude --resume agent-uuid");
	});

	// Catches: two Suspend clicks (menu + MCP request) both pass the first refusal check while the
	// resume verification is awaited, and both close the session.
	it("closes the PTY once when two suspends race", () =>
		testInScope(async () => {
			const id = addTab();
			const [a, b] = await Promise.all([suspendTerminal(id), suspendTerminal(id)]);
			expect([a.ok, b.ok].sort()).toEqual([false, true]);
			expect(mockRpc.mock.calls.filter(([cmd]) => cmd === "close_pty")).toHaveLength(1);
		}));

	// Catches: suspend closes with cleanupWorktree=true, deleting the worktree the suspended tab
	// has to resume in.
	it("keeps the worktree when it closes the PTY", () =>
		testInScope(async () => {
			const id = addTab();
			await suspendTerminal(id);
			expect(mockRpc).toHaveBeenCalledWith("close_pty", { sessionId: "pty-c1", cleanupWorktree: false });
		}));

	// Catches: a failed close wipes the pending commands or the live session id, leaving a tab that
	// is neither suspended nor attached to the PTY that is still running.
	it("leaves the tab untouched when close_pty fails", () =>
		testInScope(async () => {
			const id = addTab({ pendingInitCommand: "make dev", pendingResumeCommand: "claude --resume x" });
			mockRpc.mockRejectedValueOnce(new Error("backend gone"));
			const outcome = await suspendTerminal(id);
			expect(outcome).toEqual({ ok: false, reason: "closing the session failed" });
			expect(terminalsStore.get(id)).toMatchObject({
				suspended: false,
				sessionId: "pty-c1",
				pendingInitCommand: "make dev",
				pendingResumeCommand: "claude --resume x",
				agentState: "idle",
			});
		}));

	// Catches: the agent starts a turn while the resume command is being verified and the stale
	// pre-await check lets the suspend cut it.
	it("refuses when the agent starts working during the resume verification", () =>
		testInScope(async () => {
			const id = addTab();
			let release: (v: string) => void = () => {};
			mockVerifyResume.mockReturnValueOnce(new Promise<string>((r) => (release = r)));
			const pending = suspendTerminal(id);
			terminalsStore.update(id, { agentState: "working" });
			release("claude --resume agent-uuid");
			expect(await pending).toEqual({ ok: false, reason: "agent working" });
			expect(mockRpc).not.toHaveBeenCalled();
			expect(terminalsStore.get(id)?.suspended).toBe(false);
		}));

	// Catches: the restorable record (agent id, tab identity, alias, cwd) is cleared with the session.
	it("keeps every field the restart restore reads", () =>
		testInScope(async () => {
			const id = addTab();
			await suspendTerminal(id);
			expect(terminalsStore.get(id)).toMatchObject({
				suspended: true,
				sessionId: null,
				agentType: "claude",
				agentSessionId: "agent-uuid",
				tuicSession: "tab-uuid",
				alias: "al-9",
				cwd: "/Gits/wt/feature",
				agentLaunchCommand: "claude --dangerously-skip-permissions",
			});
		}));

	// Catches: a standby (SIGSTOP'd) tab keeps its standby flag after the PTY is gone, so the
	// resumed tab shows as frozen.
	it("clears standby with the session", () =>
		testInScope(async () => {
			const id = addTab({ standby: true });
			await suspendTerminal(id);
			expect(terminalsStore.get(id)?.standby).toBe(false);
		}));
});

describe("suspendRefusal boundaries (critic 1358)", () => {
	// Catches: a tab whose PTY already ended offers Suspend and sends close_pty for a null session.
	it("refuses a tab without a live session", () =>
		testInScope(() => {
			const id = terminalsStore.add(makeTerminal({ sessionId: null }));
			expect(suspendRefusal(terminalsStore.get(id)!)).toBe("no live session");
		}));

	// Catches: a finished turn ("completed") is treated as work in progress and can never be suspended.
	it("allows an agent whose turn completed", () =>
		testInScope(() => {
			const id = terminalsStore.add(makeTerminal({ sessionId: "pty-z" }));
			terminalsStore.update(id, { agentType: "claude", agentState: "completed", shellState: "busy" });
			expect(suspendRefusal(terminalsStore.get(id)!)).toBeNull();
		}));

	// Catches: an error-awaiting tab (awaitingInput "error") is not counted as awaiting.
	it("refuses a tab awaiting after an error", () =>
		testInScope(() => {
			const id = terminalsStore.add(makeTerminal({ sessionId: "pty-z", awaitingInput: "error" }));
			expect(suspendRefusal(terminalsStore.get(id)!)).toBe("waiting for input");
		}));
});

describe("resumeTerminal (critic 1358)", () => {
	beforeEach(() => {
		mockRpc.mockReset().mockResolvedValue(undefined);
		mockVerifyResume.mockReset().mockResolvedValue("claude --resume agent-uuid");
	});

	// Catches: resume builds the command from different inputs than the restart restore (no cwd,
	// no launch command), so a CLAUDE_CONFIG_DIR alias resumes in the wrong store.
	it("builds the resume command from the saved record the restart uses", () =>
		testInScope(async () => {
			const id = addTab();
			await suspendTerminal(id);
			mockVerifyResume.mockClear();
			await resumeTerminal(id);
			expect(mockVerifyResume).toHaveBeenCalledWith(
				"claude",
				"/Gits/wt/feature",
				"tab-uuid",
				"agent-uuid",
				"claude --dangerously-skip-permissions",
			);
		}));

	// Catches: a second Resume (button + menu) on a tab that is already live re-arms the init command.
	it("refuses to resume a tab that is not suspended", () =>
		testInScope(async () => {
			const id = addTab();
			expect(await resumeTerminal(id)).toEqual({ ok: false, reason: "not suspended" });
			expect(terminalsStore.get(id)?.pendingInitCommand).toBeNull();
		}));

	// Catches: a plain shell tab gets an init command and runs something on resume.
	it("reopens a plain shell without any command", () =>
		testInScope(async () => {
			const id = terminalsStore.add(makeTerminal({ sessionId: null, cwd: "/Gits/alpha" }));
			terminalsStore.update(id, { suspended: true });
			expect((await resumeTerminal(id)).ok).toBe(true);
			expect(terminalsStore.get(id)).toMatchObject({ suspended: false, pendingInitCommand: null });
			expect(mockVerifyResume).not.toHaveBeenCalled();
		}));
});
