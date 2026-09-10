import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScopeAsync } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

const REPO = "/repo";
const WT = "/repo__wt/feat-a";

/**
 * `pruneRemovedWorktree` is the frontend half of the `worktree-removed` event:
 * without it, a worktree removed by MCP/HTTP/merge&archive keeps its sidebar row
 * forever (the refresh prune deliberately keeps rows that still have terminals,
 * and selecting such a row spawns a fresh terminal in the missing directory).
 */
describe("pruneRemovedWorktree", () => {
	let store: typeof import("../../stores/repositories").repositoriesStore;
	let prune: typeof import("../../hooks/useWorktreeSwitchPrompt").pruneRemovedWorktree;

	beforeEach(async () => {
		vi.resetModules();
		// Fake timers: the store's debounced save would otherwise leak a pending
		// timeout past the test.
		vi.useFakeTimers();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		localStorage.clear();
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

		store = (await import("../../stores/repositories")).repositoriesStore;
		store._testSetHydrated(true);
		prune = (await import("../../hooks/useWorktreeSwitchPrompt")).pruneRemovedWorktree;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	const seedRepo = (): void => {
		store.add({ path: REPO, displayName: "repo" });
		store.setWorkspace(REPO, "main", { worktreePath: REPO, isMain: true });
		store.setWorkspace(REPO, "feat-a", { worktreePath: WT });
	};

	it("removes the branch row for a removed linked worktree", async () => {
		await testInScopeAsync(async () => {
			seedRepo();
			const closeTerminals = vi.fn().mockResolvedValue(undefined);

			await prune(REPO, "feat-a", closeTerminals);

			expect(store.get(REPO)!.workspaces["feat-a"]).toBeUndefined();
			expect(store.get(REPO)!.workspaces.main).toBeDefined();
			// No terminals attached → nothing to close.
			expect(closeTerminals).not.toHaveBeenCalled();
		});
	});

	it("closes the terminals still living in the removed worktree first", async () => {
		await testInScopeAsync(async () => {
			seedRepo();
			store.addTerminalToWorkspace(REPO, "feat-a", "term-1");
			store.addTerminalToWorkspace(REPO, "feat-a", "term-2");
			const order: string[] = [];
			const closeTerminals = vi.fn().mockImplementation(async () => {
				// The row must still exist while its terminals are being closed,
				// otherwise the close path can't resolve the branch.
				order.push(store.get(REPO)!.workspaces["feat-a"] ? "row-present" : "row-gone");
			});

			await prune(REPO, "feat-a", closeTerminals);

			expect(closeTerminals).toHaveBeenCalledWith(REPO, "feat-a");
			expect(order).toEqual(["row-present"]);
			expect(store.get(REPO)!.workspaces["feat-a"]).toBeUndefined();
		});
	});

	it("never removes the main checkout, whose worktreePath is the repo root", async () => {
		await testInScopeAsync(async () => {
			seedRepo();
			const closeTerminals = vi.fn().mockResolvedValue(undefined);

			await prune(REPO, "main", closeTerminals);

			expect(store.get(REPO)!.workspaces.main).toBeDefined();
			expect(closeTerminals).not.toHaveBeenCalled();
		});
	});

	it("is idempotent — a second event for an already-pruned branch is a no-op", async () => {
		await testInScopeAsync(async () => {
			seedRepo();
			const closeTerminals = vi.fn().mockResolvedValue(undefined);

			await prune(REPO, "feat-a", closeTerminals);
			await prune(REPO, "feat-a", closeTerminals);

			expect(store.get(REPO)!.workspaces["feat-a"]).toBeUndefined();
			expect(closeTerminals).toHaveBeenCalledTimes(0);
		});
	});

	it("ignores events for repos and branches that are not open", async () => {
		await testInScopeAsync(async () => {
			seedRepo();
			const closeTerminals = vi.fn().mockResolvedValue(undefined);

			await prune("/not/open", "feat-a", closeTerminals);
			await prune(REPO, "never-existed", closeTerminals);

			expect(Object.keys(store.get(REPO)!.workspaces).sort()).toEqual(["feat-a", "main"]);
			expect(closeTerminals).not.toHaveBeenCalled();
		});
	});

	// The removal event names ONE workspace. Two workspaces can sit on one branch
	// (that is the point of a COW clone), so a prune that resolved anything by
	// branch would take the sibling's terminals down with it — and the sibling is
	// a live agent in a different directory.
	it("leaves a same-branch sibling and its terminals untouched", async () => {
		await testInScopeAsync(async () => {
			store.add({ path: REPO, displayName: "repo" });
			store.setWorkspace(REPO, "main", { worktreePath: REPO, isMain: true });
			store.setWorkspace(REPO, "feat~aaaa1111", {
				branchName: "feat",
				worktreePath: "/repo__cow/feat-1",
				kind: "cow",
			});
			store.setWorkspace(REPO, "feat~bbbb2222", {
				branchName: "feat",
				worktreePath: "/repo__cow/feat-2",
				kind: "cow",
			});
			store.addTerminalToWorkspace(REPO, "feat~aaaa1111", "term-doomed");
			store.addTerminalToWorkspace(REPO, "feat~bbbb2222", "term-survivor");
			const closeTerminals = vi.fn().mockResolvedValue(undefined);

			await prune(REPO, "feat~aaaa1111", closeTerminals);

			expect(closeTerminals).toHaveBeenCalledExactlyOnceWith(REPO, "feat~aaaa1111");
			expect(store.get(REPO)!.workspaces["feat~aaaa1111"]).toBeUndefined();
			const survivor = store.get(REPO)!.workspaces["feat~bbbb2222"];
			expect(survivor?.terminals).toEqual(["term-survivor"]);
			expect(survivor?.branchName).toBe("feat");
		});
	});

	it("clears activeBranch when the removed worktree was the active one", async () => {
		await testInScopeAsync(async () => {
			seedRepo();
			store.setActiveWorkspace(REPO, "feat-a");
			const closeTerminals = vi.fn().mockResolvedValue(undefined);

			await prune(REPO, "feat-a", closeTerminals);

			expect(store.get(REPO)!.activeWorkspaceId).toBe("main");
		});
	});
});
