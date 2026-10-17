import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { mockInvoke, mockWarn } = vi.hoisted(() => ({ mockInvoke: vi.fn(), mockWarn: vi.fn() }));
vi.mock("../../invoke", () => ({ invoke: mockInvoke }));
vi.mock("../../stores/appLogger", () => ({
	appLogger: { info: vi.fn(), warn: mockWarn, error: vi.fn(), debug: vi.fn() },
}));

import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { closeBranchTerminalPtys } from "../../utils/closeBranchTerminals";

// The post-merge cleanup used to send each terminal-store id ("term-3") as `sessionId`, so every
// `close_pty` failed with "Session not found" and was only logged. `branch.terminals` holds terminal
// ids, never PTY session ids.
describe("closeBranchTerminalPtys", () => {
	const clear = () => {
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	};

	// The stores share the mocked `invoke` for their own persistence, so look only at `close_pty`.
	const closeCalls = () => mockInvoke.mock.calls.filter(([command]) => command === "close_pty");

	function addTerminal(sessionId: string | null) {
		const id = terminalsStore.add({ name: "t", sessionId, fontSize: 14, cwd: null, awaitingInput: null });
		repositoriesStore.addTerminalToWorkspace("/repo", "feature", id);
		return id;
	}

	beforeEach(() => {
		clear();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		mockWarn.mockReset();
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setWorkspace("/repo", "feature", { branchName: "feature", worktreePath: "/repo-feature" });
	});

	afterEach(clear);

	it("closes each terminal's PTY by its session id, never by its terminal id", async () => {
		const a = addTerminal("pty-aaa");
		const b = addTerminal("pty-bbb");

		await closeBranchTerminalPtys("/repo", "feature");

		expect(closeCalls()).toHaveLength(2);
		expect(mockInvoke).toHaveBeenCalledWith("close_pty", { sessionId: "pty-aaa", cleanupWorktree: false });
		expect(mockInvoke).toHaveBeenCalledWith("close_pty", { sessionId: "pty-bbb", cleanupWorktree: false });
		const sent = closeCalls().map(([, args]) => args.sessionId);
		expect(sent).not.toContain(a);
		expect(sent).not.toContain(b);
	});

	it("skips a tab with no PTY yet instead of asking the backend to close a non-session", async () => {
		addTerminal(null);
		addTerminal("pty-live");

		await closeBranchTerminalPtys("/repo", "feature");

		expect(closeCalls()).toHaveLength(1);
		expect(mockInvoke).toHaveBeenCalledWith("close_pty", { sessionId: "pty-live", cleanupWorktree: false });
	});

	it("keeps closing the rest when one close fails, and logs the failure", async () => {
		const first = addTerminal("pty-bad");
		addTerminal("pty-good");
		mockInvoke.mockImplementation(async (command: string, args: { sessionId?: string }) => {
			if (command === "close_pty" && args.sessionId === "pty-bad") throw new Error("boom");
		});

		await closeBranchTerminalPtys("/repo", "feature");

		expect(closeCalls()).toHaveLength(2);
		expect(mockWarn).toHaveBeenCalledWith("git", expect.stringContaining(first), expect.any(Error));
	});

	it("does nothing for an unknown repo or workspace", async () => {
		await closeBranchTerminalPtys("/nope", "feature");
		await closeBranchTerminalPtys("/repo", "no-such-workspace");
		expect(closeCalls()).toHaveLength(0);
	});
});
