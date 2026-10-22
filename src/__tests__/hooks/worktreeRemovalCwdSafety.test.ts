import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { createWorktreeRemovalCoordinator } from "../../hooks/git/createWorktreeRemovalCoordinator";
import { createWorktreeWorkflowCoordinator } from "../../hooks/git/createWorktreeWorkflowCoordinator";
import { pruneRemovedWorktree } from "../../hooks/useWorktreeSwitchPrompt";
import { globalWorkspaceStore, MANUAL_SCOPE } from "../../stores/globalWorkspace";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { makeTerminal } from "../helpers/store";

const REPO = "/repo";
const WT = "/repo__wt/feature";

describe("worktree removal cwd authority", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
		repositoriesStore._testSetHydrated(true);
		repositoriesStore.add({ path: REPO, displayName: "repo" });
		repositoriesStore.setWorkspace(REPO, "main", { worktreePath: REPO });
		repositoriesStore.setWorkspace(REPO, "feature", { worktreePath: WT });
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		terminalsStore._testCancelPendingTimers();
		vi.useRealTimers();
	});

	function add(cwd: string | null, workspaceId = "feature") {
		const id = terminalsStore.add({ ...makeTerminal({ cwd }), placementPath: WT });
		terminalsStore.setRepoPath(id, REPO);
		repositoriesStore.addTerminalToWorkspace(REPO, workspaceId, id);
		return id;
	}

	function workflow(closeTerminal: (id: string, skipConfirm?: boolean) => Promise<void>) {
		return createWorktreeWorkflowCoordinator({ closeTerminal } as Parameters<
			typeof createWorktreeWorkflowCoordinator
		>[0]);
	}

	function removal(closeTerminal: (id: string, skipConfirm?: boolean) => Promise<void>) {
		const [removingBranches, setRemovingBranches] = createSignal(new Set<string>());
		return createWorktreeRemovalCoordinator({
			repo: {
				removeWorktree: vi.fn(async () => undefined),
				getWorkspaceLifecycle: async () => ({ dirtyFiles: 0, commitStatus: "merged", removalSafety: "safe" }),
			},
			dialogs: { confirmRemoveWorktree: async () => true },
			closeTerminal,
			setStatusInfo: vi.fn(),
			removingBranches,
			setRemovingBranches,
		});
	}

	// Catches: explicit removal closes sidebar members before the safe backend event can preserve them.
	it("explicit removal preserves misfiled outside-cwd tabs and closes only checkout residents", async () => {
		const root = add(REPO);
		const sibling = add(`${WT}-other`);
		const unknown = add(null);
		const resident = add(`${WT}/src`, "main");
		const close = vi.fn(async (id: string) => terminalsStore.remove(id));
		await removal(close).handleRemoveWorkspace(REPO, "feature");
		expect(close).toHaveBeenCalledExactlyOnceWith(resident, true);
		expect(repositoriesStore.findOwnerForTerminal(root)?.workspaceId).toBe("main");
		for (const id of [root, sibling, unknown]) expect(terminalsStore.get(id)).toBeDefined();
		for (const id of [sibling, unknown]) expect(globalWorkspaceStore.getScopeMembers(MANUAL_SCOPE)).toContain(id);
		expect(repositoriesStore.get(REPO)?.workspaces.feature).toBeUndefined();
	});

	// Catches: a tab leaving during confirmation or a sibling close is still closed from stale membership.
	it("explicit removal rechecks cwd after each awaited resident close", async () => {
		const first = add(WT);
		const leaves = add(`${WT}/src`);
		const close = vi.fn(async (id: string) => {
			if (id === first) terminalsStore.update(leaves, { cwd: REPO });
			terminalsStore.remove(id);
		});
		await removal(close).handleRemoveWorkspace(REPO, "feature");
		expect(close).toHaveBeenCalledExactlyOnceWith(first, true);
		expect(terminalsStore.get(leaves)?.cwd).toBe(REPO);
		expect(repositoriesStore.findOwnerForTerminal(leaves)?.workspaceId).toBe("main");
	});

	// Catches: the backend removal event closes a root coordinator by sidebar membership.
	it("keeps a misfiled root coordinator alive and visible after a removal event", async () => {
		const root = add(REPO);
		terminalsStore.update(root, { tuicSession: "declared-root" });
		repositoriesStore.setDeclaredWorktree(REPO, "declared-root", {
			workspaceId: "feature",
			branch: "feature",
			worktreePath: WT,
		});
		const nested = add(`${WT}/src`);
		const close = vi.fn(async (id: string) => terminalsStore.remove(id));
		await pruneRemovedWorktree(REPO, "feature", workflow(close).closeTerminalsForBranch);
		expect(close).toHaveBeenCalledExactlyOnceWith(nested, true);
		expect(terminalsStore.get(root)?.cwd).toBe(REPO);
		expect(repositoriesStore.findOwnerForTerminal(root)?.workspaceId).toBe("main");
		expect(repositoriesStore.get(REPO)?.workspaces.feature).toBeUndefined();
		expect(repositoriesStore.get(REPO)?.declaredWorktrees?.["declared-root"]).toBeUndefined();
	});

	// Catches: an unrelated or unknown-cwd tab is closed, or disappears with its stale row.
	it("preserves sibling-prefix and unknown-cwd tabs when their stale row is removed", async () => {
		const sibling = add(`${WT}-other`);
		const unknown = add(null);
		const close = vi.fn(async (id: string) => terminalsStore.remove(id));
		await pruneRemovedWorktree(REPO, "feature", workflow(close).closeTerminalsForBranch);
		expect(close).not.toHaveBeenCalled();
		for (const id of [sibling, unknown]) {
			expect(terminalsStore.get(id)).toBeDefined();
			expect(globalWorkspaceStore.getScopeMembers(MANUAL_SCOPE)).toContain(id);
		}
	});

	// Catches: taking a cwd snapshot before awaiting a sibling close kills a tab that left.
	it("rechecks cwd after each awaited close rather than closing a captured membership list", async () => {
		const first = add(WT);
		const leaves = add(`${WT}/src`);
		const close = vi.fn(async (id: string) => {
			if (id === first) terminalsStore.update(leaves, { cwd: REPO });
			terminalsStore.remove(id);
		});
		await workflow(close).closeTerminalsForBranch(REPO, "feature");
		expect(close).toHaveBeenCalledExactlyOnceWith(first, true);
		expect(terminalsStore.get(leaves)?.cwd).toBe(REPO);
	});

	// Catches: Windows cwd separators fail containment, or display membership hides an in-cwd tab.
	it("closes by separator-independent cwd even when the tab is filed in a different row", async () => {
		const id = add("C:\\repo__wt\\feature\\src", "main");
		const close = vi.fn(async (id: string) => terminalsStore.remove(id));
		await workflow(close).closeTerminalsInWorktree("C:/repo__wt/feature/");
		expect(close).toHaveBeenCalledExactlyOnceWith(id, true);
	});
});
