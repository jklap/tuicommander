import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { appLogger } from "../../stores/appLogger";
import {
	allLeafIds,
	arrangeLayoutState,
	type PaneLayoutState,
	type PaneNode,
	paneLayoutStore,
	pruneLayoutToLiveTerminals,
} from "../../stores/paneLayout";

/** A flat `{ id: [tab ids] }` description -> layout whose tree is a horizontal split of those groups. */
function layoutOf(groups: Record<string, string[]>, activeGroupId: string | null = null): PaneLayoutState {
	const ids = Object.keys(groups);
	const root: PaneNode =
		ids.length === 1
			? { type: "leaf", id: ids[0] }
			: {
					type: "branch",
					direction: "horizontal",
					children: ids.map((id) => ({ type: "leaf", id }) as PaneNode),
					ratios: ids.map(() => 1 / ids.length),
				};
	return {
		root,
		groups: Object.fromEntries(
			ids.map((id) => [
				id,
				{
					id,
					tabs: groups[id].map((t) => ({
						id: t,
						type: t.startsWith("md") ? ("markdown" as const) : ("terminal" as const),
					})),
					activeTabId: groups[id].at(-1) ?? null,
				},
			]),
		),
		activeGroupId,
	};
}

const tabsOf = (l: PaneLayoutState | null): string[] =>
	Object.values(l?.groups ?? {})
		.flatMap((g) => g.tabs.map((t) => t.id))
		.sort();

describe("pruneLayoutToLiveTerminals", () => {
	const live = (...ids: string[]) => new Set(ids);

	it("returns the same panes and tabs when everything is alive", () => {
		const input = layoutOf({ g1: ["t1"], g2: ["t2"], g3: ["t3"] }, "g2");
		const out = pruneLayoutToLiveTerminals(input, live("t1", "t2", "t3"));
		expect(out).toEqual(input);
	});

	it("returns null for a layout with no tree", () => {
		expect(pruneLayoutToLiveTerminals({ root: null, groups: {}, activeGroupId: null }, live("t1"))).toBeNull();
	});

	it("returns null when the tree is a single pane (not a split)", () => {
		expect(pruneLayoutToLiveTerminals(layoutOf({ g1: ["t1"] }), live("t1"))).toBeNull();
	});

	it("drops a pane whose only terminal died and collapses the tree", () => {
		const out = pruneLayoutToLiveTerminals(layoutOf({ g1: ["t1"], g2: ["t2"], g3: ["t3"] }), live("t1", "t3"));
		expect(out && allLeafIds(out.root as PaneNode)).toEqual(["g1", "g3"]);
		expect(Object.keys(out?.groups ?? {}).sort()).toEqual(["g1", "g3"]);
	});

	it("renormalises sibling ratios after removing a pane", () => {
		const out = pruneLayoutToLiveTerminals(layoutOf({ g1: ["t1"], g2: ["t2"], g3: ["t3"] }), live("t1", "t3"));
		const root = out?.root;
		expect(root?.type).toBe("branch");
		if (root?.type === "branch") expect(root.ratios.reduce((a, b) => a + b, 0)).toBeCloseTo(1);
	});

	it("collapses nested branches left with a single child", () => {
		const nested: PaneLayoutState = {
			root: {
				type: "branch",
				direction: "horizontal",
				children: [
					{ type: "leaf", id: "g1" },
					{
						type: "branch",
						direction: "vertical",
						children: [
							{ type: "leaf", id: "g2" },
							{ type: "leaf", id: "g3" },
						],
						ratios: [0.5, 0.5],
					},
				],
				ratios: [0.5, 0.5],
			},
			groups: layoutOf({ g1: ["t1"], g2: ["t2"], g3: ["t3"] }).groups,
			activeGroupId: "g1",
		};

		const out = pruneLayoutToLiveTerminals(nested, live("t1", "t2"));

		expect(out?.root?.type).toBe("branch");
		// g3's death leaves the inner branch with one child; it must flatten to a plain leaf.
		expect(out?.root && "children" in out.root ? out.root.children.map((c) => c.type) : []).toEqual(["leaf", "leaf"]);
	});

	it("removes only the dead tab from a pane that still has other terminals", () => {
		const out = pruneLayoutToLiveTerminals(layoutOf({ g1: ["t1", "t2"], g2: ["t3"] }), live("t1", "t3"));
		expect(out?.groups.g1.tabs.map((t) => t.id)).toEqual(["t1"]);
	});

	it("repairs a pane's active tab when the active one was pruned", () => {
		const input = layoutOf({ g1: ["t1", "t2"], g2: ["t3"] });
		input.groups.g1.activeTabId = "t2";
		const out = pruneLayoutToLiveTerminals(input, live("t1", "t3"));
		expect(out?.groups.g1.activeTabId).toBe("t1");
	});

	it("keeps a pane's still-valid active tab", () => {
		const input = layoutOf({ g1: ["t1", "t2"], g2: ["t3"] });
		input.groups.g1.activeTabId = "t1";
		const out = pruneLayoutToLiveTerminals(input, live("t1", "t2", "t3"));
		expect(out?.groups.g1.activeTabId).toBe("t1");
	});

	it("moves the active group to the first remaining pane when the active pane was pruned", () => {
		const out = pruneLayoutToLiveTerminals(layoutOf({ g1: ["t1"], g2: ["t2"], g3: ["t3"] }, "g2"), live("t1", "t3"));
		expect(out?.activeGroupId).toBe("g1");
	});

	it("falls back to the first pane when there was no active group", () => {
		const out = pruneLayoutToLiveTerminals(layoutOf({ g1: ["t1"], g2: ["t2"] }, null), live("t1", "t2"));
		expect(out?.activeGroupId).toBe("g1");
	});

	it("keeps a non-terminal tab even though it isn't in the live-terminal set", () => {
		const out = pruneLayoutToLiveTerminals(layoutOf({ g1: ["t1", "md-1"], g2: ["t2"] }), live("t1", "t2"));
		expect(out?.groups.g1.tabs.map((t) => t.id)).toEqual(["t1", "md-1"]);
	});

	it("does not count a pane holding only a non-terminal tab as a surviving terminal", () => {
		expect(pruneLayoutToLiveTerminals(layoutOf({ g1: ["md-1"], g2: ["t2"] }), live())).toBeNull();
	});

	it("returns null when several panes remain but none holds a terminal", () => {
		expect(pruneLayoutToLiveTerminals(layoutOf({ g1: ["md-1"], g2: ["md-2"] }), live())).toBeNull();
	});

	it("returns null when the only other pane is empty and the rest hold no terminal", () => {
		expect(pruneLayoutToLiveTerminals(layoutOf({ g1: ["md-1"], g2: [] }), live())).toBeNull();
	});

	it("returns null when no terminal survives", () => {
		expect(pruneLayoutToLiveTerminals(layoutOf({ g1: ["t1"], g2: ["t2"] }), live("t9"))).toBeNull();
	});

	it("treats a leaf with no group record as a ghost and removes it", () => {
		const input = layoutOf({ g1: ["t1"], g2: ["t2"], g3: ["t3"] });
		delete input.groups.g2;
		const out = pruneLayoutToLiveTerminals(input, live("t1", "t2", "t3"));
		expect(out && allLeafIds(out.root as PaneNode)).toEqual(["g1", "g3"]);
	});

	it("keeps a pane that was already empty, and still prunes a dead one beside it", () => {
		const input = layoutOf({ g1: ["t1"], g2: ["t2"], g3: [] });
		const out = pruneLayoutToLiveTerminals(input, live("t1"));
		expect(out && allLeafIds(out.root as PaneNode)).toEqual(["g1", "g3"]);
		expect(out?.groups.g3).toEqual({ id: "g3", tabs: [], activeTabId: null });
	});

	it("does not mutate its input", () => {
		const input = layoutOf({ g1: ["t1", "t2"], g2: ["t3"] });
		const snapshot = JSON.stringify(input);
		pruneLayoutToLiveTerminals(input, live("t1"));
		expect(JSON.stringify(input)).toBe(snapshot);
	});
});

describe("arrangeLayoutState", () => {
	const EMPTY: PaneLayoutState = { root: null, groups: {}, activeGroupId: null };

	function ok(r: ReturnType<typeof arrangeLayoutState>): PaneLayoutState {
		if (!r.ok) throw new Error(`expected ok, got ${r.reason}`);
		return r.state;
	}

	it("reports 'empty' for no ids, and ignores blank and duplicate ids", () => {
		expect(arrangeLayoutState(EMPTY, [], "tiled")).toEqual({ ok: false, reason: "empty" });
		expect(arrangeLayoutState(EMPTY, ["", ""], "tiled")).toEqual({ ok: false, reason: "empty" });
		expect(tabsOf(ok(arrangeLayoutState(EMPTY, ["t1", "t1", "t2"], "tiled")))).toEqual(["t1", "t2"]);
	});

	it("builds one pane per session from an empty layout, active on the first", () => {
		const out = ok(arrangeLayoutState(EMPTY, ["t1", "t2", "t3"], "tiled"));
		expect(allLeafIds(out.root as PaneNode)).toHaveLength(3);
		expect(out.activeGroupId).toBe(allLeafIds(out.root as PaneNode)[0]);
		for (const g of Object.values(out.groups)) expect(g.activeTabId).toBe(g.tabs[0].id);
	});

	it("gives a lone session a bare leaf (not a split)", () => {
		expect(ok(arrangeLayoutState(EMPTY, ["t1"], "tiled")).root?.type).toBe("leaf");
	});

	it("lays 'main-vertical' out with the first id as the leader column", () => {
		const out = ok(arrangeLayoutState(EMPTY, ["lead", "w1", "w2"], "main-vertical"));
		const root = out.root;
		expect(root?.type).toBe("branch");
		if (root?.type !== "branch") return;
		expect(root.direction).toBe("horizontal");
		const leaderGroup = root.children[0];
		expect(leaderGroup.type).toBe("leaf");
		expect(leaderGroup.type === "leaf" && out.groups[leaderGroup.id].tabs[0].id).toBe("lead");
	});

	it("treats any layout name other than main-vertical as tiled", () => {
		const a = ok(arrangeLayoutState(EMPTY, ["t1", "t2", "t3", "t4"], "tiled"));
		const b = ok(arrangeLayoutState(EMPTY, ["t1", "t2", "t3", "t4"], "even-horizontal"));
		expect(JSON.stringify(a.root)).toBe(JSON.stringify(b.root));
	});

	it("refuses when the tree holds a pane none of the sessions live in", () => {
		const current = layoutOf({ g1: ["mine"], g2: ["theirs"] });
		expect(arrangeLayoutState(current, ["mine", "t2"], "tiled")).toEqual({ ok: false, reason: "unrelated-panes" });
	});

	it("reuses a pane that is already a session's sole tab", () => {
		const current = layoutOf({ g1: ["t1"], g2: ["t2"] });
		const out = ok(arrangeLayoutState(current, ["t1", "t2"], "tiled"));
		expect(Object.keys(out.groups).sort()).toEqual(["g1", "g2"]);
	});

	it("moves a session out of a shared pane, leaving the others behind", () => {
		const current = layoutOf({ g1: ["t1", "md-1"] });
		const out = ok(arrangeLayoutState(current, ["t1", "t2"], "tiled"));
		expect(out.groups.g1.tabs.map((t) => t.id)).toEqual(["md-1"]);
		expect(Object.values(out.groups).filter((g) => g.tabs.some((t) => t.id === "t1"))).toHaveLength(1);
	});

	it("repairs the source pane's active tab when the moved session was active", () => {
		const current = layoutOf({ g1: ["md-1", "t1"] });
		current.groups.g1.activeTabId = "t1";
		const out = ok(arrangeLayoutState(current, ["t1"], "tiled"));
		expect(out.groups.g1.activeTabId).toBe("md-1");
	});

	it("leaves the source pane's active tab alone when a different tab was active", () => {
		const current = layoutOf({ g1: ["md-1", "t1"] });
		current.groups.g1.activeTabId = "md-1";
		const out = ok(arrangeLayoutState(current, ["t1"], "tiled"));
		expect(out.groups.g1.activeTabId).toBe("md-1");
	});

	it("never reuses an existing group id for a new pane", () => {
		const current = layoutOf({ g5: ["t1", "md-1"] });
		const out = ok(arrangeLayoutState(current, ["t1", "t2"], "tiled"));
		const fresh = Object.keys(out.groups).filter((g) => g !== "g5");
		expect(fresh.sort()).toEqual(["g6", "g7"]);
	});

	it("keeps the active pane when it is still one of those arranged", () => {
		const current = layoutOf({ g1: ["t1"], g2: ["t2"] }, "g2");
		expect(ok(arrangeLayoutState(current, ["t1", "t2"], "tiled")).activeGroupId).toBe("g2");
	});

	it("falls back to the first arranged pane when the active pane isn't one of them", () => {
		const current = layoutOf({ g1: ["t1", "md-1"] }, "g1");
		const out = ok(arrangeLayoutState(current, ["t1", "t2"], "tiled"));
		expect(out.activeGroupId).toBe(allLeafIds(out.root as PaneNode)[0]);
		expect(out.activeGroupId).not.toBe("g1");
	});

	it("does not mutate its input", () => {
		const current = layoutOf({ g1: ["t1", "md-1"] }, "g1");
		const snapshot = JSON.stringify(current);
		arrangeLayoutState(current, ["t1", "t2"], "tiled");
		expect(JSON.stringify(current)).toBe(snapshot);
	});
});

describe("paneLayoutStore.arrangeSessionsAsLayout (live-store wrapper)", () => {
	beforeEach(() => {
		paneLayoutStore.reset();
	});

	afterEach(() => {
		paneLayoutStore.flushSave();
		vi.restoreAllMocks();
	});

	it("applies the arrangement to the live layout", () => {
		paneLayoutStore.arrangeSessionsAsLayout(["t1", "t2", "t3"], "tiled");

		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(paneLayoutStore.getTerminalTabIds().sort()).toEqual(["t1", "t2", "t3"]);
	});

	it("leaves the live layout untouched and says why when it holds panes the request doesn't own", () => {
		const warn = vi.spyOn(appLogger, "warn").mockImplementation(() => {});
		paneLayoutStore.arrangeSessionsAsLayout(["mine1", "mine2"], "tiled");
		const before = JSON.stringify(paneLayoutStore.serialize());

		paneLayoutStore.arrangeSessionsAsLayout(["swarm1", "swarm2"], "tiled");

		expect(JSON.stringify(paneLayoutStore.serialize())).toBe(before);
		expect(warn).toHaveBeenCalledWith("app", expect.stringContaining("skipped"));
	});

	it("is a silent no-op for an empty request", () => {
		const warn = vi.spyOn(appLogger, "warn").mockImplementation(() => {});

		paneLayoutStore.arrangeSessionsAsLayout([], "tiled");

		expect(paneLayoutStore.isSplit()).toBe(false);
		expect(warn).not.toHaveBeenCalled();
	});
});
