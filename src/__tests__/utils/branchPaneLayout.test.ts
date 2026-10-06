import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

vi.mock("../../invoke", () => ({
	invoke: vi.fn(() => Promise.resolve(null)),
}));

vi.mock("../../stores/terminals", () => ({
	terminalsStore: { onRemove: vi.fn(), getIds: vi.fn(() => []), isDetached: vi.fn(() => false) },
}));

import { invoke } from "../../invoke";
import { type PaneLayoutState, paneLayoutStore } from "../../stores/paneLayout";
import { paneLayoutKey, savedPaneLayouts } from "../../stores/savedPaneLayouts";
import { resolvePaneLayoutForBranch, savePaneLayoutForBranch } from "../../utils/branchPaneLayout";

const REPO = "/repo/commerce-journal";
const BRANCH = "main";
const KEY = paneLayoutKey(REPO, BRANCH);

function liveTerminalIds(): string[] {
	return Object.values(paneLayoutStore.serialize().groups)
		.flatMap((g) => g.tabs.map((t) => t.id))
		.sort();
}

/** Split `ids` into a saved layout for the branch, then clear the live store like a branch switch does. */
function saveSplitAndLeave(ids: string[]): void {
	paneLayoutStore.arrangeSessionsAsLayout(ids, "tiled");
	savePaneLayoutForBranch(REPO, BRANCH);
	paneLayoutStore.reset();
}

describe("resolvePaneLayoutForBranch — saved layout restore", () => {
	beforeEach(() => {
		paneLayoutStore.reset();
		savedPaneLayouts.clear();
	});

	afterEach(() => {
		paneLayoutStore.flushSave();
	});

	it("restores the whole split when every terminal is still valid", () => {
		saveSplitAndLeave(["t1", "t2", "t3", "t4"]);

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1", "t2", "t3", "t4", "t5"]);

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(liveTerminalIds()).toEqual(["t1", "t2", "t3", "t4"]);
	});

	it("keeps the rest of the split when one terminal was closed while away", () => {
		saveSplitAndLeave(["t1", "t2", "t3", "t4"]);

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1", "t2", "t3"]);

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(liveTerminalIds()).toEqual(["t1", "t2", "t3"]);
		expect(paneLayoutStore.getAllGroupIds()).toHaveLength(3);
	});

	it("refreshes the cached copy so a later restore doesn't see the closed terminal again", () => {
		saveSplitAndLeave(["t1", "t2", "t3", "t4"]);

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1", "t2", "t3"]);

		const cached = savedPaneLayouts.get(KEY);
		expect(cached).toBeDefined();
		expect(Object.values(cached?.groups ?? {}).flatMap((g) => g.tabs.map((t) => t.id))).not.toContain("t4");
	});

	it("keeps a valid active pane/tab after pruning the one that was active", () => {
		paneLayoutStore.arrangeSessionsAsLayout(["t1", "t2", "t3"], "tiled");
		paneLayoutStore.setActiveGroup(paneLayoutStore.getGroupForTab("t3") as string);
		savePaneLayoutForBranch(REPO, BRANCH);
		paneLayoutStore.reset();

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1", "t2"]);

		const live = paneLayoutStore.serialize();
		expect(live.activeGroupId).not.toBeNull();
		expect(live.groups[live.activeGroupId as string]).toBeDefined();
	});

	it("resets to a flat view when pruning leaves only one pane", () => {
		saveSplitAndLeave(["t1", "t2", "t3", "t4"]);

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t2"]);

		expect(paneLayoutStore.isSplit()).toBe(false);
		expect(savedPaneLayouts.has(KEY)).toBe(false);
	});

	it("resets when none of the split's terminals survive", () => {
		saveSplitAndLeave(["t1", "t2"]);

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t9"]);

		expect(paneLayoutStore.isSplit()).toBe(false);
		expect(savedPaneLayouts.has(KEY)).toBe(false);
	});

	it("keeps a pane the user split open but hasn't filled yet", () => {
		const g1 = paneLayoutStore.createGroup();
		paneLayoutStore.addTab(g1, { id: "t1", type: "terminal" });
		paneLayoutStore.setRoot({ type: "leaf", id: g1 });
		paneLayoutStore.setActiveGroup(g1);
		paneLayoutStore.split(g1, "vertical");
		savePaneLayoutForBranch(REPO, BRANCH);
		paneLayoutStore.reset();

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1"]);

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(paneLayoutStore.getAllGroupIds()).toHaveLength(2);
	});

	it("still drops a pane whose only terminal died, while keeping an empty pane beside it", () => {
		const g1 = paneLayoutStore.createGroup();
		paneLayoutStore.addTab(g1, { id: "t1", type: "terminal" });
		paneLayoutStore.setRoot({ type: "leaf", id: g1 });
		paneLayoutStore.setActiveGroup(g1);
		const g2 = paneLayoutStore.split(g1, "vertical") as string;
		paneLayoutStore.addTab(g2, { id: "t2", type: "terminal" });
		paneLayoutStore.split(g2, "horizontal"); // third pane, left empty
		savePaneLayoutForBranch(REPO, BRANCH);
		paneLayoutStore.reset();

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1"]); // t2 died

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(liveTerminalIds()).toEqual(["t1"]);
		expect(paneLayoutStore.getAllGroupIds()).toHaveLength(2); // t1's pane + the still-empty one
	});

	it("keeps non-terminal tabs in a surviving pane", () => {
		paneLayoutStore.arrangeSessionsAsLayout(["t1", "t2", "t3"], "tiled");
		const g1 = paneLayoutStore.getGroupForTab("t1") as string;
		paneLayoutStore.addTab(g1, { id: "md-1", type: "markdown" });
		savePaneLayoutForBranch(REPO, BRANCH);
		paneLayoutStore.reset();

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1", "t2"]);

		expect(liveTerminalIds()).toEqual(["md-1", "t1", "t2"]);
	});
});

describe("savePaneLayoutForBranch", () => {
	beforeEach(() => {
		paneLayoutStore.reset();
		savedPaneLayouts.clear();
	});

	afterEach(() => {
		paneLayoutStore.flushSave();
	});

	it("stores the live split under the branch's key", () => {
		paneLayoutStore.arrangeSessionsAsLayout(["t1", "t2"], "tiled");

		savePaneLayoutForBranch(REPO, BRANCH);

		expect(
			Object.values(savedPaneLayouts.get(KEY)?.groups ?? {})
				.flatMap((g) => g.tabs.map((t) => t.id))
				.sort(),
		).toEqual(["t1", "t2"]);
	});

	it("keys the saved split by repo AND branch", () => {
		paneLayoutStore.arrangeSessionsAsLayout(["t1", "t2"], "tiled");

		savePaneLayoutForBranch(REPO, "other-branch");

		expect(savedPaneLayouts.has(paneLayoutKey(REPO, "other-branch"))).toBe(true);
		expect(savedPaneLayouts.has(KEY)).toBe(false);
	});

	it("deletes a stale saved split when the branch is no longer split", () => {
		paneLayoutStore.arrangeSessionsAsLayout(["t1", "t2"], "tiled");
		savePaneLayoutForBranch(REPO, BRANCH);
		paneLayoutStore.reset();

		savePaneLayoutForBranch(REPO, BRANCH);

		expect(savedPaneLayouts.has(KEY)).toBe(false);
	});

	it("snapshots the layout, so later live edits don't alter what was saved", () => {
		paneLayoutStore.arrangeSessionsAsLayout(["t1", "t2"], "tiled");
		savePaneLayoutForBranch(REPO, BRANCH);

		paneLayoutStore.reset();

		expect(savedPaneLayouts.get(KEY)?.root?.type).toBe("branch");
	});
});

describe("resolvePaneLayoutForBranch — nothing saved, nothing from disk", () => {
	beforeEach(() => {
		paneLayoutStore.reset();
		savedPaneLayouts.clear();
	});

	afterEach(() => {
		paneLayoutStore.flushSave();
	});

	it("clears another branch's live split rather than leaving it under this branch", () => {
		paneLayoutStore.arrangeSessionsAsLayout(["x1", "x2"], "tiled");

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1", "t2"]);

		expect(paneLayoutStore.isSplit()).toBe(false);
	});

	it("does not restore another branch's saved split", () => {
		paneLayoutStore.arrangeSessionsAsLayout(["x1", "x2"], "tiled");
		savePaneLayoutForBranch(REPO, "elsewhere");
		paneLayoutStore.reset();

		resolvePaneLayoutForBranch(REPO, BRANCH, ["x1", "x2"]);

		expect(paneLayoutStore.isSplit()).toBe(false);
	});
});

describe("resolvePaneLayoutForBranch — layout restored from disk at startup", () => {
	const onDisk = (groups: Record<string, string[]>): PaneLayoutState => {
		const ids = Object.keys(groups);
		return {
			root: {
				type: "branch",
				direction: "horizontal",
				children: ids.map((id) => ({ type: "leaf", id }) as const),
				ratios: ids.map(() => 1 / ids.length),
			},
			groups: Object.fromEntries(
				ids.map((id) => [
					id,
					{ id, tabs: groups[id].map((t) => ({ id: t, type: "terminal" as const })), activeTabId: groups[id][0] },
				]),
			),
			activeGroupId: ids[0],
		};
	};

	async function loadFromDisk(layout: PaneLayoutState): Promise<void> {
		vi.mocked(invoke).mockResolvedValueOnce(layout);
		await paneLayoutStore.loadFromDisk();
	}

	beforeEach(() => {
		paneLayoutStore.reset();
		savedPaneLayouts.clear();
	});

	afterEach(() => {
		paneLayoutStore.flushSave();
	});

	it("keeps the disk layout untouched when every terminal in it is valid", async () => {
		await loadFromDisk(onDisk({ g1: ["t1"], g2: ["t2"] }));

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1", "t2", "t3"]);

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(liveTerminalIds()).toEqual(["t1", "t2"]);
	});

	it("prunes a terminal that no longer exists instead of dropping the whole disk layout", async () => {
		await loadFromDisk(onDisk({ g1: ["t1"], g2: ["t2"], g3: ["t3"] }));

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1", "t3"]);

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(liveTerminalIds()).toEqual(["t1", "t3"]);
	});

	it("prunes terminals that belong to a different branch out of the disk layout", async () => {
		await loadFromDisk(onDisk({ g1: ["t1"], g2: ["t2"], g3: ["other-branch-term"] }));

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1", "t2"]);

		expect(liveTerminalIds()).toEqual(["t1", "t2"]);
	});

	it("resets when fewer than two panes would survive", async () => {
		await loadFromDisk(onDisk({ g1: ["t1"], g2: ["t2"] }));

		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1"]);

		expect(paneLayoutStore.isSplit()).toBe(false);
	});

	it("applies the disk layout to only the first branch resolved after startup", async () => {
		await loadFromDisk(onDisk({ g1: ["t1"], g2: ["t2"] }));
		resolvePaneLayoutForBranch(REPO, BRANCH, ["t1", "t2"]);

		resolvePaneLayoutForBranch(REPO, "second-branch", ["t1", "t2"]);

		expect(paneLayoutStore.isSplit()).toBe(false);
	});

	it("prefers an in-memory saved layout over the disk layout", async () => {
		await loadFromDisk(onDisk({ g1: ["t1"], g2: ["t2"] }));
		savedPaneLayouts.set(KEY, onDisk({ g7: ["s1"], g8: ["s2"] }));

		resolvePaneLayoutForBranch(REPO, BRANCH, ["s1", "s2", "t1", "t2"]);

		expect(liveTerminalIds()).toEqual(["s1", "s2"]);
	});
});
