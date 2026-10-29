import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { testInScope } from "../helpers/store";

const mockInvoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));

/**
 * "+" on a branch row that has no checkout of its own used to spawn a shell in the repo
 * root — another branch's directory — and the cwd tracker then re-homed the tab under the
 * row that really owns it. It is refused up front instead.
 */
describe("adding a terminal to a branch that is not checked out anywhere", () => {
	let createBranchSelectionCoordinator: typeof import("../../hooks/git/createBranchSelectionCoordinator").createBranchSelectionCoordinator;
	let repositoriesStore: typeof import("../../stores/repositories").repositoriesStore;
	let terminalsStore: typeof import("../../stores/terminals").terminalsStore;
	let paneLayoutStore: typeof import("../../stores/paneLayout").paneLayoutStore;
	let settingsStore: typeof import("../../stores/settings").settingsStore;
	const setStatusInfo = vi.fn();

	beforeEach(async () => {
		vi.resetModules();
		setStatusInfo.mockReset();
		mockInvoke.mockReset().mockResolvedValue(undefined);
		vi.doMock("@tauri-apps/api/core", () => ({ invoke: mockInvoke }));
		createBranchSelectionCoordinator = (await import("../../hooks/git/createBranchSelectionCoordinator"))
			.createBranchSelectionCoordinator;
		repositoriesStore = (await import("../../stores/repositories")).repositoriesStore;
		terminalsStore = (await import("../../stores/terminals")).terminalsStore;
		paneLayoutStore = (await import("../../stores/paneLayout")).paneLayoutStore;
		settingsStore = (await import("../../stores/settings")).settingsStore;
		repositoriesStore._testSetHydrated(true);
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		paneLayoutStore._testCancelPendingSave();
		settingsStore._testCancelPendingSave();
	});

	const coordinator = () =>
		createBranchSelectionCoordinator({
			repo: { getDiffStats: async () => ({ additions: 0, deletions: 0 }) },
			pty: { canSpawn: async () => true },
			setStatusInfo,
			getDefaultFontSize: () => 14,
		});

	const addRepo = () => {
		repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
		repositoriesStore.setWorkspace("/Gits/alpha", "wip-bkup", { worktreePath: "/Gits/alpha", kind: "main" });
		repositoriesStore.setActive("/Gits/alpha");
		repositoriesStore.setActiveWorkspace("/Gits/alpha", "wip-bkup");
	};

	it("refuses, says why, and creates no terminal", async () => {
		await testInScope(async () => {
			addRepo();
			repositoriesStore.setWorkspace("/Gits/alpha", "wip"); // a branch row, no directory
			const before = terminalsStore.getIds().length;

			const id = await coordinator().handleAddTerminalToWorkspace("/Gits/alpha", "wip");

			expect(id).toBeUndefined();
			expect(terminalsStore.getIds()).toHaveLength(before);
			expect(setStatusInfo).toHaveBeenCalledWith(expect.stringContaining('"wip" has no checkout of its own'));
			expect(repositoriesStore.get("/Gits/alpha")?.workspaces.wip.terminals).toEqual([]);
		});
	});

	it("still opens one on the row that owns the repo root", async () => {
		await testInScope(async () => {
			addRepo();

			const id = await coordinator().handleAddTerminalToWorkspace("/Gits/alpha", "wip-bkup");

			expect(id).toBeTruthy();
			expect(terminalsStore.get(id!)?.cwd).toBe("/Gits/alpha");
			expect(setStatusInfo).not.toHaveBeenCalled();
		});
	});

	it("still opens one on a linked worktree", async () => {
		await testInScope(async () => {
			addRepo();
			repositoriesStore.setWorkspace("/Gits/alpha", "feature", { worktreePath: "/Gits/alpha__wt/feature" });

			const id = await coordinator().handleAddTerminalToWorkspace("/Gits/alpha", "feature");

			expect(terminalsStore.get(id!)?.cwd).toBe("/Gits/alpha__wt/feature");
		});
	});

	it("does not refuse when the caller names the directory itself", async () => {
		await testInScope(async () => {
			addRepo();
			repositoriesStore.setWorkspace("/Gits/alpha", "wip");

			const id = await coordinator().handleAddTerminalToWorkspace("/Gits/alpha", "wip", "/Gits/alpha/sub");

			expect(id).toBeTruthy();
			expect(terminalsStore.get(id!)?.cwd).toBe("/Gits/alpha/sub");
		});
	});

	it("opens one on a lone row that has no directory recorded yet", async () => {
		await testInScope(async () => {
			repositoriesStore.add({ path: "/Gits/alpha", displayName: "alpha" });
			repositoriesStore.setWorkspace("/Gits/alpha", "main");
			repositoriesStore.setActive("/Gits/alpha");
			repositoriesStore.setActiveWorkspace("/Gits/alpha", "main");

			const id = await coordinator().handleAddTerminalToWorkspace("/Gits/alpha", "main");

			expect(terminalsStore.get(id!)?.cwd).toBe("/Gits/alpha");
		});
	});
});
