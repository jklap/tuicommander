import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal, testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
const mockListen = vi.fn().mockResolvedValue(vi.fn());

vi.mock("../../invoke", () => ({
	invoke: mockInvoke,
	listen: mockListen,
}));

const REPO = "/repo";
const WORKTREE = "/repo__wt/feature";
const BRANCH = "feature";

describe("switchToCreatedWorktree", () => {
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
	let terminalsStore: typeof import("../../stores/terminals").terminalsStore;
	let switchToCreatedWorktree: typeof import("../../hooks/useWorktreeSwitchPrompt").switchToCreatedWorktree;

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		mockListen.mockReset().mockResolvedValue(vi.fn());
		localStorage.clear();
		vi.doMock("../../invoke", () => ({ invoke: mockInvoke, listen: mockListen }));

		repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
		terminalsStore = (await import("../../stores/terminals")).terminalsStore;
		repositoriesStore._testSetHydrated(true);
		switchToCreatedWorktree = (await import("../../hooks/useWorktreeSwitchPrompt")).switchToCreatedWorktree;
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		terminalsStore._testCancelPendingTimers();
		vi.useRealTimers();
	});

	function seedActiveTerminal(agentType: "codex" | null): string {
		repositoriesStore.add({ path: REPO, displayName: "repo" });
		repositoriesStore.setWorkspace(REPO, "main", { worktreePath: REPO, isMain: true });
		repositoriesStore.setWorkspace(REPO, BRANCH, { worktreePath: WORKTREE });
		const terminalId = terminalsStore.add(makeTerminal({ sessionId: "session-main", cwd: REPO, agentType }));
		repositoriesStore.addTerminalToWorkspace(REPO, "main", terminalId);
		terminalsStore.setActive(terminalId);
		mockInvoke.mockClear();
		return terminalId;
	}

	it("opens the worktree without moving or interrupting a running agent", async () => {
		await testInScopeAsync(async () => {
			const terminalId = seedActiveTerminal("codex");
			const handleBranchSelect = vi.fn().mockResolvedValue(undefined);

			await switchToCreatedWorktree(
				{ handleBranchSelect, closeTerminalsForBranch: vi.fn() },
				REPO,
				BRANCH,
				BRANCH,
				WORKTREE,
			);

			expect(handleBranchSelect).toHaveBeenCalledWith(REPO, BRANCH);
			expect(repositoriesStore.get(REPO)!.workspaces.main.terminals).toContain(terminalId);
			expect(repositoriesStore.get(REPO)!.workspaces[BRANCH].terminals).not.toContain(terminalId);
			expect(mockInvoke).not.toHaveBeenCalled();
		});
	});

	it("keeps moving a plain shell into the worktree", async () => {
		await testInScopeAsync(async () => {
			const terminalId = seedActiveTerminal(null);
			const handleBranchSelect = vi.fn().mockResolvedValue(undefined);

			await switchToCreatedWorktree(
				{ handleBranchSelect, closeTerminalsForBranch: vi.fn() },
				REPO,
				BRANCH,
				BRANCH,
				WORKTREE,
			);

			expect(handleBranchSelect).toHaveBeenCalledWith(REPO, BRANCH);
			expect(repositoriesStore.get(REPO)!.workspaces.main.terminals).not.toContain(terminalId);
			expect(repositoriesStore.get(REPO)!.workspaces[BRANCH].terminals).toContain(terminalId);
			expect(mockInvoke).toHaveBeenCalledWith("write_pty", {
				sessionId: "session-main",
				data: `cd ${WORKTREE}\n`,
			});
		});
	});

	// The decision is made at click time, not when the worktree was created: the
	// toast outlives the event, so the tab that was active then may not be the tab
	// that is active now.
	it("re-reads the active terminal at call time", async () => {
		await testInScopeAsync(async () => {
			const shellId = seedActiveTerminal(null);
			const agentId = terminalsStore.add(makeTerminal({ sessionId: "session-agent", cwd: REPO, agentType: "codex" }));
			repositoriesStore.addTerminalToWorkspace(REPO, "main", agentId);
			terminalsStore.setActive(agentId);
			mockInvoke.mockClear();
			const handleBranchSelect = vi.fn().mockResolvedValue(undefined);

			await switchToCreatedWorktree(
				{ handleBranchSelect, closeTerminalsForBranch: vi.fn() },
				REPO,
				BRANCH,
				BRANCH,
				WORKTREE,
			);

			// The agent is the active tab now, so nothing moves and no cd is written —
			// even though a movable shell was active when the worktree was created.
			expect(repositoriesStore.get(REPO)!.workspaces.main.terminals).toContain(shellId);
			expect(repositoriesStore.get(REPO)!.workspaces[BRANCH].terminals).not.toContain(agentId);
			expect(mockInvoke).not.toHaveBeenCalled();
		});
	});
});

describe("useWorktreeSwitchPrompt — worktree-created", () => {
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
	let terminalsStore: typeof import("../../stores/terminals").terminalsStore;
	let toastsStore: typeof import("../../stores/toasts").toastsStore;
	let activityStore: typeof import("../../stores/activityStore").activityStore;
	let useWorktreeSwitchPrompt: typeof import("../../hooks/useWorktreeSwitchPrompt").useWorktreeSwitchPrompt;
	let handlers: Map<string, (event: { payload: unknown }) => void>;

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		handlers = new Map();
		mockListen.mockReset().mockImplementation((name: string, cb: (event: { payload: unknown }) => void) => {
			handlers.set(name, cb);
			return Promise.resolve(vi.fn());
		});
		localStorage.clear();
		vi.doMock("../../invoke", () => ({ invoke: mockInvoke, listen: mockListen }));

		repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
		terminalsStore = (await import("../../stores/terminals")).terminalsStore;
		toastsStore = (await import("../../stores/toasts")).toastsStore;
		activityStore = (await import("../../stores/activityStore")).activityStore;
		repositoriesStore._testSetHydrated(true);
		useWorktreeSwitchPrompt = (await import("../../hooks/useWorktreeSwitchPrompt")).useWorktreeSwitchPrompt;
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		terminalsStore._testCancelPendingTimers();
		vi.useRealTimers();
	});

	async function emitCreated(
		handleBranchSelect = vi.fn().mockResolvedValue(undefined),
		workspaceId = BRANCH,
		branch = BRANCH,
		kind: "worktree" = "worktree",
	) {
		repositoriesStore.add({ path: REPO, displayName: "repo" });
		repositoriesStore.setWorkspace(REPO, "main", { worktreePath: REPO, isMain: true });
		useWorktreeSwitchPrompt({ handleBranchSelect, closeTerminalsForBranch: vi.fn() });
		await Promise.resolve();
		handlers.get("worktree-created")?.({
			payload: {
				repo_path: REPO,
				workspace_id: workspaceId,
				branch,
				worktree_path: WORKTREE,
				kind,
			},
		});
		return handleBranchSelect;
	}

	// Catches: moving siblings, a foreign caller, or the creator of an explicit spawn.
	it.each([
		["eligible", REPO, false, "creator"],
		["active", REPO, false, "creator"],
		["spawned", REPO, true, "creator"],
		["foreign", "/other", false, "creator"],
		["missing", REPO, false, null],
	] as const)("worktree_creation_moves_only_eligible_creator (%s)", async (_case, ownerRepo, spawn, caller) => {
		await testInScopeAsync(async () => {
			repositoriesStore.add({ path: REPO, displayName: "repo" });
			repositoriesStore.setWorkspace(REPO, "main", { worktreePath: REPO, isMain: true });
			if (ownerRepo !== REPO) {
				repositoriesStore.add({ path: ownerRepo, displayName: "other" });
				repositoriesStore.setWorkspace(ownerRepo, "main", { worktreePath: ownerRepo, isMain: true });
			}
			const creator = terminalsStore.add(makeTerminal({ sessionId: "creator", cwd: ownerRepo }));
			const sibling = terminalsStore.add(makeTerminal({ sessionId: "sibling", cwd: REPO }));
			terminalsStore.setRepoPath(creator, ownerRepo);
			terminalsStore.setRepoPath(sibling, REPO);
			repositoriesStore.addTerminalToWorkspace(ownerRepo, "main", creator);
			repositoriesStore.addTerminalToWorkspace(REPO, "main", sibling);
			terminalsStore.setActive(_case === "active" ? creator : sibling);
			repositoriesStore.setActive(REPO);
			repositoriesStore.setActiveWorkspace(REPO, "main");
			useWorktreeSwitchPrompt({ handleBranchSelect: vi.fn(), closeTerminalsForBranch: vi.fn() });
			await Promise.resolve();
			handlers.get("worktree-created")?.({
				payload: {
					repo_path: REPO,
					workspace_id: BRANCH,
					branch: BRANCH,
					worktree_path: WORKTREE,
					kind: "worktree",
					creator_session: caller,
					spawn_session: spawn,
				},
			});
			const eligible = ownerRepo === REPO && !spawn && caller !== null;
			expect(repositoriesStore.findOwnerForTerminal(creator)?.workspaceId).toBe(eligible ? BRANCH : "main");
			expect(repositoriesStore.findOwnerForTerminal(creator)?.repoPath).toBe(ownerRepo);
			expect(repositoriesStore.findOwnerForTerminal(sibling)?.workspaceId).toBe("main");
			expect(terminalsStore.state.activeId).toBe(_case === "active" ? creator : sibling);
			expect(repositoriesStore.get(REPO)?.activeWorkspaceId).toBe(_case === "active" ? BRANCH : "main");
			expect(terminalsStore.get(creator)?.cwd).toBe(ownerRepo);
			expect(mockInvoke).not.toHaveBeenCalledWith("write_pty", expect.anything());
		});
	});

	// Catches: a declaration shown as a new-worktree offer, duplicated placement on
	// retries, a removed saved snapshot, or cwd reconciliation undoing the declaration.
	it("declared_worktree_moves_only_the_caller_without_a_creation_offer", async () => {
		await testInScopeAsync(async () => {
			repositoriesStore.add({ path: REPO, displayName: "repo" });
			repositoriesStore.setWorkspace(REPO, "main", { worktreePath: REPO, isMain: true });
			const creator = terminalsStore.add(makeTerminal({ sessionId: "declaring-caller", cwd: REPO }));
			const sibling = terminalsStore.add(makeTerminal({ sessionId: "sibling", cwd: REPO }));
			terminalsStore.setRepoPath(creator, REPO);
			terminalsStore.setRepoPath(sibling, REPO);
			repositoriesStore.addTerminalToWorkspace(REPO, "main", creator);
			repositoriesStore.addTerminalToWorkspace(REPO, "main", sibling);
			terminalsStore.setActive(sibling);
			repositoriesStore.setActiveWorkspace(REPO, "main");
			useWorktreeSwitchPrompt({ handleBranchSelect: vi.fn(), closeTerminalsForBranch: vi.fn() });
			await Promise.resolve();
			const event = {
				payload: {
					repo_path: REPO,
					workspace_id: BRANCH,
					branch: BRANCH,
					worktree_path: WORKTREE,
					kind: "worktree",
					creator_session: "declaring-caller",
					spawn_session: false,
				},
			};
			handlers.get("session-worktree-declared")?.(event);
			repositoriesStore.setWorkspace(REPO, BRANCH, {
				savedTerminals: [{ name: "Caller", cwd: REPO, fontSize: 14, agentType: "codex", tuicSession: "stable-caller" }],
			});
			handlers.get("session-worktree-declared")?.(event);
			const { reconcileTerminalOwnership } = await import("../../stores/terminalOwnership");
			const { createTerminalWorktreeCoordinator } = await import("../../hooks/git/createTerminalWorktreeCoordinator");
			reconcileTerminalOwnership();
			const coordinator = createTerminalWorktreeCoordinator({
				refreshBranches: vi.fn().mockResolvedValue(undefined),
				writePty: vi.fn().mockResolvedValue(undefined),
			});
			vi.useFakeTimers();
			coordinator.handleTerminalCwdChange(creator, REPO);
			await vi.advanceTimersByTimeAsync(300);
			coordinator.cancelCwdTracking(creator);
			vi.useRealTimers();
			expect(terminalsStore.get(creator)?.cwd).toBe(REPO);
			expect(repositoriesStore.get(REPO)?.workspaces[BRANCH].terminals).toEqual([creator]);
			expect(repositoriesStore.get(REPO)?.workspaces[BRANCH].savedTerminals).toHaveLength(1);
			expect(repositoriesStore.findOwnerForTerminal(sibling)?.workspaceId).toBe("main");
			expect(repositoriesStore.get(REPO)?.activeWorkspaceId).toBe("main");
			expect(activityStore.getForSection("worktrees")).toHaveLength(0);
			expect(mockInvoke).not.toHaveBeenCalledWith("write_pty", expect.anything());
		});
	});
	// Catches: an MCP/HTTP worktree event covering the input with an unsolicited toast.
	it("1397 keeps backend-created worktrees in the bell without a toast", async () => {
		await testInScopeAsync(async () => {
			const handleBranchSelect = await emitCreated();
			expect(toastsStore.toasts).toHaveLength(0);
			const item = activityStore.getForSection("worktrees").find((i) => i.title === `Worktree: ${BRANCH}`);
			expect(item?.subtitle).toBe("repo__wt/feature");
			expect(item?.onClick).toBeTypeOf("function");
			expect(handleBranchSelect).not.toHaveBeenCalled();
		});
	});

	it("registers the worktree in the sidebar whether or not the offer is taken", async () => {
		await testInScopeAsync(async () => {
			await emitCreated();

			expect(repositoriesStore.get(REPO)!.workspaces[BRANCH]?.worktreePath).toBe(WORKTREE);
			expect(activityStore.getForSection("worktrees").some((i) => i.title === `Worktree: ${BRANCH}`)).toBe(true);
		});
	});

	it("records a linked worktree with no parent repo", async () => {
		await testInScopeAsync(async () => {
			await emitCreated();

			const row = repositoriesStore.get(REPO)!.workspaces[BRANCH];
			expect(row?.kind).toBe("worktree");
			expect(row?.parentRepoPath).toBeNull();
		});
	});

	it("switches only once the bell offer is clicked", async () => {
		await testInScopeAsync(async () => {
			const handleBranchSelect = await emitCreated();

			const item = activityStore.getForSection("worktrees").find((i) => i.title === `Worktree: ${BRANCH}`);
			item!.onClick!();
			await Promise.resolve();

			expect(handleBranchSelect).toHaveBeenCalledWith(REPO, BRANCH);
		});
	});

	// The whole point of carrying both fields: the row is filed under the id the
	// backend minted, while every string the user reads is the branch. With
	// `workspace_id === branch` — which is what a linked worktree has — a hook
	// that quietly keyed by `branch` would pass every other test in this file.
	it("files the row under the workspace id and labels it with the branch", async () => {
		await testInScopeAsync(async () => {
			await emitCreated(vi.fn().mockResolvedValue(undefined), "feature~a1b2c3d4", "feature/x");

			const workspaces = repositoriesStore.get(REPO)!.workspaces;
			expect(workspaces["feature~a1b2c3d4"]?.worktreePath).toBe(WORKTREE);
			expect(workspaces["feature~a1b2c3d4"]?.branchName).toBe("feature/x");
			expect(workspaces["feature/x"]).toBeUndefined();
			expect(activityStore.getForSection("worktrees").some((i) => i.title === "Worktree: feature/x")).toBe(true);
		});
	});

	it("keeps the offer reachable from the bell during unattended creation", async () => {
		await testInScopeAsync(async () => {
			const handleBranchSelect = await emitCreated();

			const item = activityStore.getForSection("worktrees").find((i) => i.title === `Worktree: ${BRANCH}`);
			expect(item?.onClick).toBeTypeOf("function");
			item!.onClick!();
			await Promise.resolve();

			expect(handleBranchSelect).toHaveBeenCalledWith(REPO, BRANCH);
		});
	});
});
