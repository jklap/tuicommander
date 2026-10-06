import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

/**
 * The tmux shim's `select-layout` must land in the layout of the branch that
 * OWNS the swarm's terminals — not whatever branch happens to be on screen.
 * Real `paneLayoutStore` + `savedPaneLayouts`; repo/terminal stores mocked.
 */
const { mockRepositoriesStore, mockTerminalsStore, mockGlobalWorkspaceStore } = vi.hoisted(() => ({
	mockRepositoriesStore: {
		getRepoPathForTerminal: vi.fn<(id: string) => string | null>(),
		state: {
			activeRepoPath: null as string | null,
			repositories: {} as Record<
				string,
				{ activeWorkspaceId: string | null; workspaces: Record<string, { terminals: string[] }> }
			>,
		},
	},
	mockTerminalsStore: {
		onRemove: vi.fn(),
		getIds: vi.fn<() => string[]>(() => []),
		isDetached: vi.fn<(id: string) => boolean>(() => false),
	},
	mockGlobalWorkspaceStore: { isManualWorkspaceActive: vi.fn(() => false) },
}));

vi.mock("../../stores/repositories", () => ({ repositoriesStore: mockRepositoriesStore }));
vi.mock("../../stores/terminals", () => ({ terminalsStore: mockTerminalsStore }));
vi.mock("../../stores/globalWorkspace", () => ({ globalWorkspaceStore: mockGlobalWorkspaceStore }));

import { appLogger } from "../../stores/appLogger";
import { paneLayoutStore } from "../../stores/paneLayout";
import { paneLayoutKey, savedPaneLayouts } from "../../stores/savedPaneLayouts";
import { arrangeSwarmLayout } from "../../utils/arrangeTmuxLayout";
import { resolvePaneLayoutForBranch } from "../../utils/branchPaneLayout";

const REPO_A = "/repo/commerce-journal"; // the swarm's repo
const REPO_B = "/repo/tuicommander"; // what's on screen
const SWARM = ["s1", "s2", "s3", "s4"];
const KEY_A = paneLayoutKey(REPO_A, "main");

function tabIds(layout: { groups: Record<string, { tabs: { id: string }[] }> } | undefined): string[] {
	return Object.values(layout?.groups ?? {})
		.flatMap((g) => g.tabs.map((t) => t.id))
		.sort();
}

describe("arrangeSwarmLayout", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		paneLayoutStore.reset();
		savedPaneLayouts.clear();
		mockGlobalWorkspaceStore.isManualWorkspaceActive.mockReturnValue(false);

		// Repo B is on screen; the swarm lives in repo A.
		mockRepositoriesStore.state.activeRepoPath = REPO_B;
		mockRepositoriesStore.state.repositories = {
			[REPO_A]: { activeWorkspaceId: "main", workspaces: { main: { terminals: [...SWARM, "a-extra"] } } },
			[REPO_B]: { activeWorkspaceId: "wip", workspaces: { wip: { terminals: ["b1", "b2"] } } },
		};
		mockRepositoriesStore.getRepoPathForTerminal.mockImplementation((id) =>
			SWARM.includes(id) || id === "a-extra" ? REPO_A : id.startsWith("b") ? REPO_B : null,
		);
		mockTerminalsStore.getIds.mockReturnValue([...SWARM, "a-extra", "b1", "b2"]);
		mockTerminalsStore.isDetached.mockReturnValue(false);
	});

	afterEach(() => {
		paneLayoutStore.flushSave();
		vi.restoreAllMocks();
	});

	it("arranges the live layout when the swarm's branch is the one on screen", () => {
		mockRepositoriesStore.state.activeRepoPath = REPO_A;

		arrangeSwarmLayout(SWARM, "tiled");

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(tabIds(paneLayoutStore.serialize())).toEqual(SWARM);
		expect(savedPaneLayouts.has(KEY_A)).toBe(false);
	});

	it("stores the split under the swarm's own branch when another branch is on screen", () => {
		arrangeSwarmLayout(SWARM, "tiled");

		expect(tabIds(savedPaneLayouts.get(KEY_A))).toEqual(SWARM);
		expect(savedPaneLayouts.get(KEY_A)?.root?.type).toBe("branch");
	});

	it("leaves the layout on screen alone when the swarm belongs to another branch", () => {
		// The user's own split of repo B's terminals is showing.
		paneLayoutStore.arrangeSessionsAsLayout(["b1", "b2"], "tiled");
		const before = JSON.stringify(paneLayoutStore.serialize());

		arrangeSwarmLayout(SWARM, "tiled");

		expect(JSON.stringify(paneLayoutStore.serialize())).toBe(before);
	});

	it("shows the arranged split when the user later switches to the swarm's branch", () => {
		arrangeSwarmLayout(SWARM, "tiled");

		resolvePaneLayoutForBranch(REPO_A, "main", [...SWARM, "a-extra"]);

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(tabIds(paneLayoutStore.serialize())).toEqual(SWARM);
	});

	it("re-arranges a previously saved swarm split whose member has since been closed", () => {
		arrangeSwarmLayout(SWARM, "tiled");
		// s4 closes while away; the next teammate event carries only the survivors.
		mockTerminalsStore.getIds.mockReturnValue(["s1", "s2", "s3", "a-extra", "b1", "b2"]);

		arrangeSwarmLayout(["s1", "s2", "s3"], "tiled");

		expect(tabIds(savedPaneLayouts.get(KEY_A))).toEqual(["s1", "s2", "s3"]);
	});

	it("does not clobber an unrelated saved split on the swarm's branch", () => {
		mockRepositoriesStore.state.repositories[REPO_A].workspaces.main.terminals.push("u1", "u2");
		mockTerminalsStore.getIds.mockReturnValue([...SWARM, "a-extra", "u1", "u2", "b1", "b2"]);
		paneLayoutStore.arrangeSessionsAsLayout(["u1", "u2"], "tiled");
		savedPaneLayouts.set(KEY_A, paneLayoutStore.serialize());
		paneLayoutStore.reset();
		const before = JSON.stringify(savedPaneLayouts.get(KEY_A));

		arrangeSwarmLayout(SWARM, "tiled");

		expect(JSON.stringify(savedPaneLayouts.get(KEY_A))).toBe(before);
	});

	it("falls back to the live layout when a terminal's owner can't be resolved", () => {
		mockRepositoriesStore.getRepoPathForTerminal.mockReturnValue(null);

		arrangeSwarmLayout(SWARM, "tiled");

		expect(tabIds(paneLayoutStore.serialize())).toEqual(SWARM);
		expect(savedPaneLayouts.size).toBe(0);
	});

	it("uses the live layout while the manual Global Workspace is showing", () => {
		mockGlobalWorkspaceStore.isManualWorkspaceActive.mockReturnValue(true);

		arrangeSwarmLayout(SWARM, "tiled");

		expect(tabIds(paneLayoutStore.serialize())).toEqual(SWARM);
		expect(savedPaneLayouts.size).toBe(0);
	});

	it("splits a mixed set between the live layout and the other branch's saved one", () => {
		mockRepositoriesStore.state.activeRepoPath = REPO_A;

		arrangeSwarmLayout(["s1", "s2", "b1", "b2"], "tiled");

		expect(tabIds(paneLayoutStore.serialize())).toEqual(["s1", "s2"]);
		expect(tabIds(savedPaneLayouts.get(paneLayoutKey(REPO_B, "wip")))).toEqual(["b1", "b2"]);
	});

	it("treats a branch other than the one on screen in the SAME repo as off-screen", () => {
		mockRepositoriesStore.state.activeRepoPath = REPO_A;
		mockRepositoriesStore.state.repositories[REPO_A].workspaces.feature = { terminals: ["w1", "w2", "w3"] };
		mockRepositoriesStore.getRepoPathForTerminal.mockImplementation((id) => (id.startsWith("w") ? REPO_A : null));
		mockTerminalsStore.getIds.mockReturnValue(["w1", "w2", "w3"]);

		arrangeSwarmLayout(["w1", "w2", "w3"], "tiled");

		expect(paneLayoutStore.isSplit()).toBe(false);
		expect(tabIds(savedPaneLayouts.get(paneLayoutKey(REPO_A, "feature")))).toEqual(["w1", "w2", "w3"]);
	});

	it("treats a swarm as off-screen when the active repo has no active branch", () => {
		mockRepositoriesStore.state.repositories[REPO_B].activeWorkspaceId = null;

		arrangeSwarmLayout(SWARM, "tiled");

		expect(tabIds(savedPaneLayouts.get(KEY_A))).toEqual(SWARM);
	});

	it("uses the live layout when the terminal's repo resolves but isn't in the store", () => {
		mockRepositoriesStore.getRepoPathForTerminal.mockReturnValue("/repo/unregistered");

		arrangeSwarmLayout(SWARM, "tiled");

		expect(tabIds(paneLayoutStore.serialize())).toEqual(SWARM);
		expect(savedPaneLayouts.size).toBe(0);
	});

	it("uses the live layout when no branch of the repo lists the terminal yet", () => {
		mockRepositoriesStore.state.repositories[REPO_A].workspaces.main.terminals = [];

		arrangeSwarmLayout(SWARM, "tiled");

		expect(tabIds(paneLayoutStore.serialize())).toEqual(SWARM);
		expect(savedPaneLayouts.size).toBe(0);
	});

	it("saves nothing for a lone off-screen terminal (a single pane isn't a split)", () => {
		arrangeSwarmLayout(["s1"], "tiled");

		expect(savedPaneLayouts.size).toBe(0);
		expect(paneLayoutStore.isSplit()).toBe(false);
	});

	it("honours main-vertical for an off-screen branch, leader first", () => {
		arrangeSwarmLayout(["s1", "s2", "s3"], "main-vertical");

		const saved = savedPaneLayouts.get(KEY_A);
		const root = saved?.root;
		expect(root?.type).toBe("branch");
		if (root?.type !== "branch") return;
		const first = root.children[0];
		expect(first.type === "leaf" && saved?.groups[first.id].tabs[0].id).toBe("s1");
	});

	it("keeps separate saved splits for terminals owned by different off-screen branches", () => {
		mockRepositoriesStore.state.repositories["/repo/third"] = {
			activeWorkspaceId: "main",
			workspaces: { main: { terminals: ["c1", "c2"] } },
		};
		mockRepositoriesStore.getRepoPathForTerminal.mockImplementation((id) =>
			id.startsWith("s") ? REPO_A : id.startsWith("c") ? "/repo/third" : null,
		);
		mockTerminalsStore.getIds.mockReturnValue([...SWARM, "c1", "c2"]);

		arrangeSwarmLayout(["s1", "s2", "c1", "c2"], "tiled");

		expect(tabIds(savedPaneLayouts.get(KEY_A))).toEqual(["s1", "s2"]);
		expect(tabIds(savedPaneLayouts.get(paneLayoutKey("/repo/third", "main")))).toEqual(["c1", "c2"]);
	});

	it("rebuilds from scratch when the earlier saved split has fewer than two live panes left", () => {
		arrangeSwarmLayout(["s1", "s2"], "tiled");
		mockTerminalsStore.getIds.mockReturnValue(["s1", "s3", "s4", "a-extra", "b1", "b2"]); // s2 died

		arrangeSwarmLayout(["s1", "s3", "s4"], "tiled");

		expect(tabIds(savedPaneLayouts.get(KEY_A))).toEqual(["s1", "s3", "s4"]);
	});

	it("treats a member detached since the last arrangement as gone", () => {
		arrangeSwarmLayout(SWARM, "tiled");
		mockTerminalsStore.isDetached.mockImplementation((id: string) => id === "s4");

		arrangeSwarmLayout(["s1", "s2", "s3"], "tiled");

		expect(tabIds(savedPaneLayouts.get(KEY_A))).toEqual(["s1", "s2", "s3"]);
	});

	it("re-arranging the same sessions again leaves a valid split", () => {
		arrangeSwarmLayout(SWARM, "tiled");

		arrangeSwarmLayout(SWARM, "tiled");

		expect(tabIds(savedPaneLayouts.get(KEY_A))).toEqual(SWARM);
		expect(savedPaneLayouts.get(KEY_A)?.root?.type).toBe("branch");
	});

	it("logs, and keeps the saved split, when it holds panes the swarm doesn't own", () => {
		const warn = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		mockRepositoriesStore.state.repositories[REPO_A].workspaces.main.terminals.push("u1", "u2");
		mockTerminalsStore.getIds.mockReturnValue([...SWARM, "a-extra", "u1", "u2", "b1", "b2"]);
		paneLayoutStore.arrangeSessionsAsLayout(["u1", "u2"], "tiled");
		savedPaneLayouts.set(KEY_A, paneLayoutStore.serialize());
		paneLayoutStore.reset();

		arrangeSwarmLayout(SWARM, "tiled");

		expect(warn).toHaveBeenCalledWith("app", expect.stringContaining(REPO_A));
		expect(tabIds(savedPaneLayouts.get(KEY_A))).toEqual(["u1", "u2"]);
	});

	it("does nothing for an empty request", () => {
		arrangeSwarmLayout([], "tiled");

		expect(savedPaneLayouts.size).toBe(0);
		expect(paneLayoutStore.isSplit()).toBe(false);
	});
});
