import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

/**
 * Regression: switching repo/branch by clicking a terminal (`navigateToTerminal`)
 * must save the outgoing branch's split and resolve the incoming branch's own,
 * exactly like a branch-row select does. Uses the REAL `paneLayoutStore` and
 * `savedPaneLayouts` — a fully mocked pane store can't observe the bug.
 */
const { mockRepositoriesStore, mockTerminalsStore, mockGlobalWorkspaceStore } = vi.hoisted(() => {
	const mockRepositoriesStore = {
		getRepoPathForTerminal: vi.fn<(id: string) => string | null>(),
		state: {
			activeRepoPath: null as string | null,
			repositories: {} as Record<
				string,
				{ activeWorkspaceId: string | null; workspaces: Record<string, { terminals: string[] }> }
			>,
		},
		setActive: vi.fn((path: string) => {
			mockRepositoriesStore.state.activeRepoPath = path;
		}),
		setActiveWorkspace: vi.fn((path: string, id: string) => {
			mockRepositoriesStore.state.repositories[path].activeWorkspaceId = id;
		}),
	};
	const mockTerminalsStore = {
		setActive: vi.fn(),
		get: vi.fn(() => undefined),
		getIds: vi.fn<() => string[]>(() => []),
		isDetached: vi.fn<(id: string) => boolean>(() => false),
		onRemove: vi.fn(),
	};
	const mockGlobalWorkspaceStore = {
		isManualWorkspaceActive: vi.fn(() => false),
		getPromotedIds: vi.fn<() => string[]>(() => []),
		deactivate: vi.fn(),
	};
	return { mockRepositoriesStore, mockTerminalsStore, mockGlobalWorkspaceStore };
});

vi.mock("../../stores/repositories", () => ({ repositoriesStore: mockRepositoriesStore }));
vi.mock("../../stores/terminals", () => ({ terminalsStore: mockTerminalsStore }));
vi.mock("../../stores/globalWorkspace", () => ({ globalWorkspaceStore: mockGlobalWorkspaceStore }));
vi.mock("../../hooks/useWorktreeConsolidation", () => ({ syncScopeForActiveRepo: vi.fn() }));

import { paneLayoutStore } from "../../stores/paneLayout";
import { paneLayoutKey, savedPaneLayouts } from "../../stores/savedPaneLayouts";
import { navigateToTerminal } from "../../utils/navigateToTerminal";

const REPO_A = "/repo/commerce-journal";
const REPO_B = "/repo/other";
const SPLIT = ["t1", "t2", "t3", "t4"];
const B_SPLIT = ["b1", "b2"];
const KEY_A_MAIN = paneLayoutKey(REPO_A, "main");
const KEY_B_MAIN = paneLayoutKey(REPO_B, "main");

function terminalIdsInLayout(): string[] {
	return Object.values(paneLayoutStore.serialize().groups).flatMap((g) => g.tabs.map((t) => t.id));
}

function savedIds(key: string): string[] {
	return Object.values(savedPaneLayouts.get(key)?.groups ?? {})
		.flatMap((g) => g.tabs.map((t) => t.id))
		.sort();
}

describe("navigateToTerminal pane-layout round trip", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		paneLayoutStore.reset();
		savedPaneLayouts.clear();
		vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback): number => {
			cb(0);
			return 0;
		});

		// Repo A: main has 6 terminals (4 split), feature has 2. Repo B: main has 3.
		mockRepositoriesStore.state.activeRepoPath = REPO_A;
		mockRepositoriesStore.state.repositories = {
			[REPO_A]: {
				activeWorkspaceId: "main",
				workspaces: { main: { terminals: [...SPLIT, "t5", "t6"] }, feature: { terminals: ["f1", "f2"] } },
			},
			[REPO_B]: { activeWorkspaceId: "main", workspaces: { main: { terminals: ["b1", "b2", "b3"] } } },
		};
		mockRepositoriesStore.getRepoPathForTerminal.mockImplementation((id) =>
			id.startsWith("b") ? REPO_B : /^(t|f)\d/.test(id) ? REPO_A : null,
		);
		mockTerminalsStore.getIds.mockReturnValue([...SPLIT, "t5", "t6", "f1", "f2", "b1", "b2", "b3"]);
		mockTerminalsStore.isDetached.mockReturnValue(false);
		mockGlobalWorkspaceStore.isManualWorkspaceActive.mockReturnValue(false);

		paneLayoutStore.arrangeSessionsAsLayout(SPLIT, "tiled");
	});

	afterEach(() => {
		paneLayoutStore.flushSave();
	});

	it("starts with a 4-pane split (fixture sanity)", () => {
		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(terminalIdsInLayout().sort()).toEqual(SPLIT);
	});

	it("saves the outgoing branch's split when clicking a terminal in another repo", () => {
		navigateToTerminal("b1");

		expect(savedIds(KEY_A_MAIN)).toEqual(SPLIT);
	});

	it("does not leave the outgoing branch's split live under the incoming branch", () => {
		navigateToTerminal("b1");

		expect(paneLayoutStore.isSplit()).toBe(false);
	});

	it("restores the split when clicking a terminal back in the original repo", () => {
		navigateToTerminal("b1");
		navigateToTerminal("t5");

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(terminalIdsInLayout().sort()).toEqual(SPLIT);
	});

	it("restores the split after a reset happened while away (e.g. a branch select elsewhere)", () => {
		navigateToTerminal("b1");
		// What any branch-row select on the other repo does to the live layout.
		paneLayoutStore.reset();
		navigateToTerminal("t5");

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(terminalIdsInLayout().sort()).toEqual(SPLIT);
	});

	it("keeps both branches' splits when hopping between two split branches", () => {
		// Give repo B a split of its own, saved as if the user had been there.
		paneLayoutStore.reset();
		paneLayoutStore.arrangeSessionsAsLayout(B_SPLIT, "tiled");
		savedPaneLayouts.set(KEY_B_MAIN, paneLayoutStore.serialize());
		paneLayoutStore.reset();
		paneLayoutStore.arrangeSessionsAsLayout(SPLIT, "tiled");

		navigateToTerminal("b1");
		expect(terminalIdsInLayout().sort()).toEqual(B_SPLIT);
		expect(savedIds(KEY_A_MAIN)).toEqual(SPLIT);

		navigateToTerminal("t5");
		expect(terminalIdsInLayout().sort()).toEqual(SPLIT);
		expect(savedIds(KEY_B_MAIN)).toEqual(B_SPLIT);
	});

	it("saves and restores across a branch change inside the same repo", () => {
		navigateToTerminal("f1");

		expect(mockRepositoriesStore.setActive).not.toHaveBeenCalled();
		expect(mockRepositoriesStore.setActiveWorkspace).toHaveBeenCalledWith(REPO_A, "feature");
		expect(savedIds(KEY_A_MAIN)).toEqual(SPLIT);
		expect(paneLayoutStore.isSplit()).toBe(false);

		navigateToTerminal("t5");
		expect(terminalIdsInLayout().sort()).toEqual(SPLIT);
	});

	it("leaves the live split alone when the terminal is on the branch already showing", () => {
		const before = JSON.stringify(paneLayoutStore.serialize());

		navigateToTerminal("t5");

		expect(JSON.stringify(paneLayoutStore.serialize())).toBe(before);
		expect(savedPaneLayouts.size).toBe(0);
	});

	it("keeps the rest of the split when one of its terminals closes while away", () => {
		navigateToTerminal("b1");
		mockTerminalsStore.getIds.mockReturnValue(["t1", "t2", "t3", "t5", "t6", "f1", "f2", "b1", "b2", "b3"]);

		navigateToTerminal("t5");

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(terminalIdsInLayout().sort()).toEqual(["t1", "t2", "t3"]);
	});

	it("drops a terminal that was detached while away rather than reviving its pane", () => {
		navigateToTerminal("b1");
		mockTerminalsStore.isDetached.mockImplementation((id) => id === "t4");

		navigateToTerminal("t5");

		expect(terminalIdsInLayout().sort()).toEqual(["t1", "t2", "t3"]);
	});

	it("restores a flat view when too few of the split's terminals are left", () => {
		navigateToTerminal("b1");
		mockTerminalsStore.getIds.mockReturnValue(["t1", "t5", "t6", "f1", "f2", "b1", "b2", "b3"]);

		navigateToTerminal("t5");

		expect(paneLayoutStore.isSplit()).toBe(false);
		expect(savedPaneLayouts.has(KEY_A_MAIN)).toBe(false);
	});

	it("exits the manual Global Workspace first, then saves the repo layout it hands back", () => {
		mockGlobalWorkspaceStore.isManualWorkspaceActive.mockReturnValue(true);
		const order: string[] = [];
		mockGlobalWorkspaceStore.deactivate.mockImplementation(() => {
			order.push("deactivate");
		});
		mockRepositoriesStore.setActive.mockImplementation((path: string) => {
			order.push("setActive");
			mockRepositoriesStore.state.activeRepoPath = path;
		});

		navigateToTerminal("b1");

		expect(order).toEqual(["deactivate", "setActive"]);
		expect(savedIds(KEY_A_MAIN)).toEqual(SPLIT);
	});

	it("does nothing to the layout or repo state for a terminal no repo claims", () => {
		const before = JSON.stringify(paneLayoutStore.serialize());

		navigateToTerminal("orphan");

		expect(JSON.stringify(paneLayoutStore.serialize())).toBe(before);
		expect(mockRepositoriesStore.setActive).not.toHaveBeenCalled();
		expect(savedPaneLayouts.size).toBe(0);
	});

	it("does nothing to the layout when the owning repo isn't in the store", () => {
		mockRepositoriesStore.getRepoPathForTerminal.mockReturnValue("/repo/unregistered");
		const before = JSON.stringify(paneLayoutStore.serialize());

		navigateToTerminal("t1");

		expect(JSON.stringify(paneLayoutStore.serialize())).toBe(before);
		expect(savedPaneLayouts.size).toBe(0);
	});

	it("does nothing to the layout when no workspace of the repo lists the terminal", () => {
		const before = JSON.stringify(paneLayoutStore.serialize());

		navigateToTerminal("t99"); // resolves to REPO_A but is in none of its workspaces

		expect(JSON.stringify(paneLayoutStore.serialize())).toBe(before);
		expect(savedPaneLayouts.size).toBe(0);
	});

	it("switches without saving anything when there was no active repo to leave", () => {
		mockRepositoriesStore.state.activeRepoPath = null;

		navigateToTerminal("b1");

		expect(mockRepositoriesStore.setActive).toHaveBeenCalledWith(REPO_B);
		expect(savedPaneLayouts.size).toBe(0);
	});

	it("switches without saving when the repo being left has no active workspace", () => {
		mockRepositoriesStore.state.repositories[REPO_A].activeWorkspaceId = null;

		navigateToTerminal("b1");

		expect(savedPaneLayouts.size).toBe(0);
		expect(mockRepositoriesStore.setActive).toHaveBeenCalledWith(REPO_B);
	});

	it("clears a stale saved split for the branch being left when it is no longer split", () => {
		savedPaneLayouts.set(KEY_A_MAIN, paneLayoutStore.serialize());
		paneLayoutStore.reset();

		navigateToTerminal("b1");

		expect(savedPaneLayouts.has(KEY_A_MAIN)).toBe(false);
	});
});
