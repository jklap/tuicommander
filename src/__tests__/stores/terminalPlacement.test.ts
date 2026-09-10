import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
	invoke: mockInvoke,
}));

describe("resolvePlacementForCwd()", () => {
	let store: typeof import("../../stores/repositories").repositoriesStore;
	let resolvePlacementForCwd: typeof import("../../stores/terminalPlacement").resolvePlacementForCwd;
	let resolvePlacementForOwner: typeof import("../../stores/terminalPlacement").resolvePlacementForOwner;
	let resolveRepoOwner: typeof import("../../stores/repositories").resolveRepoOwner;

	beforeEach(async () => {
		vi.resetModules();
		vi.useFakeTimers();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		localStorage.clear();

		vi.doMock("@tauri-apps/api/core", () => ({
			invoke: mockInvoke,
		}));

		const repoMod = await import("../../stores/repositories");
		store = repoMod.repositoriesStore;
		resolveRepoOwner = repoMod.resolveRepoOwner;
		store._testSetHydrated(true);
		const placementMod = await import("../../stores/terminalPlacement");
		resolvePlacementForCwd = placementMod.resolvePlacementForCwd;
		resolvePlacementForOwner = placementMod.resolvePlacementForOwner;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("returns null for a null/undefined/empty cwd with no active repo", () => {
		testInScope(() => {
			expect(resolvePlacementForCwd(null)).toBeNull();
			expect(resolvePlacementForCwd(undefined)).toBeNull();
			expect(resolvePlacementForCwd("")).toBeNull();
		});
	});

	it("resolves a cwd inside a linked worktree to its repo+branch, not a guess", () => {
		testInScope(() => {
			store.add({ path: "/repo", displayName: "Repo" });
			store.setBranch("/repo", "feature", { worktreePath: "/repo__wt/feature" });

			expect(resolvePlacementForCwd("/repo__wt/feature/src")).toEqual({
				repoPath: "/repo",
				branchName: "feature",
				isGuess: false,
			});
		});
	});

	it("resolves a cwd at the repo root to the repo's activeBranch, not a guess", () => {
		testInScope(() => {
			store.add({ path: "/repo", displayName: "Repo" });
			store.setBranch("/repo", "main", { worktreePath: "/repo" });
			store.setActiveBranch("/repo", "main");

			expect(resolvePlacementForCwd("/repo/src/deep")).toEqual({
				repoPath: "/repo",
				branchName: "main",
				isGuess: false,
			});
		});
	});

	it("falls back to the active repo (as a guess) when no registered repo owns the cwd", () => {
		testInScope(() => {
			store.add({ path: "/repo", displayName: "Repo" });
			store.setBranch("/repo", "main", { worktreePath: "/repo" });
			store.setActiveBranch("/repo", "main");
			store.setActive("/repo");

			expect(resolvePlacementForCwd("/somewhere/unrelated")).toEqual({
				repoPath: "/repo",
				branchName: "main",
				isGuess: true,
			});
		});
	});

	it("returns null when the active repo has no active branch", () => {
		testInScope(() => {
			store.add({ path: "/repo", displayName: "Repo" });
			store.setActive("/repo");
			// No branches registered at all — activeBranch stays null.

			expect(resolvePlacementForCwd("/somewhere/unrelated")).toBeNull();
		});
	});

	it("returns null when nothing owns the cwd and there is no active repo", () => {
		testInScope(() => {
			expect(resolvePlacementForCwd("/somewhere/unrelated")).toBeNull();
		});
	});

	it("does not use the active repo as a fallback owner when the cwd IS owned by a different registered repo", () => {
		testInScope(() => {
			store.add({ path: "/repo-a", displayName: "A" });
			store.setBranch("/repo-a", "main", { worktreePath: "/repo-a" });
			store.setActiveBranch("/repo-a", "main");
			store.setActive("/repo-a");

			store.add({ path: "/repo-b", displayName: "B" });
			store.setBranch("/repo-b", "dev", { worktreePath: "/repo-b__wt/dev" });

			expect(resolvePlacementForCwd("/repo-b__wt/dev/src")).toEqual({
				repoPath: "/repo-b",
				branchName: "dev",
				isGuess: false,
			});
		});
	});

	describe("resolvePlacementForOwner()", () => {
		// This is the split-out core `resolvePlacementForCwd` calls after doing
		// its own `resolveRepoOwner`. Callers that already have an owner (e.g.
		// `assignSessionToRepoBranch` in `hooks/useAppInit.ts`) use this directly
		// so the repo-owner scan runs once, not twice, per session.

		it("matches resolvePlacementForCwd's result when given the same cwd's pre-resolved owner", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "Repo" });
				store.setBranch("/repo", "feature", { worktreePath: "/repo__wt/feature" });

				const cwd = "/repo__wt/feature/src";
				const owner = resolveRepoOwner(cwd);

				expect(resolvePlacementForOwner(owner)).toEqual(resolvePlacementForCwd(cwd));
				expect(resolvePlacementForOwner(owner)).toEqual({
					repoPath: "/repo",
					branchName: "feature",
					isGuess: false,
				});
			});
		});

		it("falls back to the active repo (as a guess) when owner is null", () => {
			testInScope(() => {
				store.add({ path: "/repo", displayName: "Repo" });
				store.setBranch("/repo", "main", { worktreePath: "/repo" });
				store.setActiveBranch("/repo", "main");
				store.setActive("/repo");

				expect(resolvePlacementForOwner(null)).toEqual({
					repoPath: "/repo",
					branchName: "main",
					isGuess: true,
				});
			});
		});

		it("returns null when owner is null and there is no active repo", () => {
			testInScope(() => {
				expect(resolvePlacementForOwner(null)).toBeNull();
			});
		});
	});
});
