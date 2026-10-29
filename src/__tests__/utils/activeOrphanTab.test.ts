import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

vi.mock("../../invoke", () => ({
	invoke: vi.fn(() => Promise.resolve(null)),
}));

import { _testCancelPendingTimers, useSplitPanes } from "../../hooks/useSplitPanes";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { paneLayoutStore, resetGroupCounter } from "../../stores/paneLayout";
import { terminalsStore } from "../../stores/terminals";
import { closeActiveTarget, getActiveOrphanTabId, resolveCloseTarget } from "../../utils/activeOrphanTab";

const addTerminal = (name: string) =>
	terminalsStore.add({ sessionId: null, fontSize: 14, name, cwd: null, awaitingInput: null });

function resetStores() {
	for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	diffTabsStore.clearAll();
	mdTabsStore.clearAll();
	editorTabsStore.clearAll();
	paneLayoutStore.reset();
	resetGroupCounter();
}

/** A tmux-shim style split holding two agent terminals. */
function agentSplit() {
	const a = addTerminal("agent-a");
	const b = addTerminal("agent-b");
	paneLayoutStore.arrangeSessionsAsLayout([a, b], "tiled");
	return { a, b };
}

beforeEach(resetStores);
afterEach(async () => {
	paneLayoutStore._testCancelPendingSave();
	_testCancelPendingTimers();
	await new Promise((r) => setImmediate(r));
});

describe("getActiveOrphanTabId", () => {
	it("is null when there is no split, even with an active terminal", () => {
		terminalsStore.setActive(addTerminal("main 1"));
		expect(getActiveOrphanTabId()).toBeNull();
	});

	it("is null when the active terminal belongs to a pane group", () => {
		const { a } = agentSplit();
		terminalsStore.setActive(a);
		expect(getActiveOrphanTabId()).toBeNull();
	});

	it("is null in a split when nothing is active", () => {
		agentSplit();
		expect(getActiveOrphanTabId()).toBeNull();
	});

	it("returns an active terminal that is not in any group", () => {
		agentSplit();
		const main = addTerminal("main 1");
		terminalsStore.setActive(main);
		expect(getActiveOrphanTabId()).toBe(main);
	});

	it.each([
		["diff", () => diffTabsStore.add("/repo", "file.ts", "M"), (id: string) => diffTabsStore.setActive(id)],
		["markdown", () => mdTabsStore.add("/repo", "notes.md"), (id: string) => mdTabsStore.setActive(id)],
		["editor", () => editorTabsStore.add("/repo", "main.rs"), (id: string) => editorTabsStore.setActive(id)],
	])("returns an active %s tab that is not in any group", (_name, add, activate) => {
		agentSplit();
		const id = add();
		activate(id);
		expect(getActiveOrphanTabId()).toBe(id);
	});

	it("is null when a docked diff tab is the active one", () => {
		const { a } = agentSplit();
		const diff = diffTabsStore.add("/repo", "file.ts", "M");
		const group = paneLayoutStore.getGroupForTab(a) as string;
		paneLayoutStore.addTab(group, { id: diff, type: "diff" });
		diffTabsStore.setActive(diff);
		expect(getActiveOrphanTabId()).toBeNull();
	});
});

describe("resolveCloseTarget", () => {
	it("closes the active terminal when there is no split", () => {
		const id = addTerminal("main 1");
		terminalsStore.setActive(id);
		expect(resolveCloseTarget()).toEqual({ kind: "tab", id });
	});

	it("returns null when nothing is active and there is no split", () => {
		expect(resolveCloseTarget()).toBeNull();
	});

	it("closes the active pane when the active terminal belongs to the split", () => {
		const { a } = agentSplit();
		terminalsStore.setActive(a);
		expect(resolveCloseTarget()).toEqual({ kind: "pane" });
	});

	it("closes the active pane in a split when no tab is active (e.g. a fresh empty pane)", () => {
		agentSplit();
		expect(resolveCloseTarget()).toEqual({ kind: "pane" });
	});

	it("closes the visible orphan terminal, not the hidden split behind it", () => {
		agentSplit();
		const main = addTerminal("main 1");
		terminalsStore.setActive(main);
		expect(paneLayoutStore.getGroupForTab(main)).toBeFalsy();
		expect(resolveCloseTarget()).toEqual({ kind: "tab", id: main });
	});

	it("closes a visible orphan diff tab over a split too", () => {
		agentSplit();
		const diff = diffTabsStore.add("/repo", "file.ts", "M");
		diffTabsStore.setActive(diff);
		expect(resolveCloseTarget()).toEqual({ kind: "tab", id: diff });
	});

	it("closes a visible orphan markdown or editor tab over a split too", () => {
		agentSplit();
		const md = mdTabsStore.add("/repo", "notes.md");
		mdTabsStore.setActive(md);
		expect(resolveCloseTarget()).toEqual({ kind: "tab", id: md });
		mdTabsStore.setActive(null);

		const edit = editorTabsStore.add("/repo", "main.rs");
		editorTabsStore.setActive(edit);
		expect(resolveCloseTarget()).toEqual({ kind: "tab", id: edit });
	});
});

describe("closeActiveTarget", () => {
	const fakes = () => ({ closeTab: vi.fn(), closePane: vi.fn() });

	it("calls closeTab with the active terminal when not split", () => {
		const id = addTerminal("main 1");
		terminalsStore.setActive(id);
		const deps = fakes();
		closeActiveTarget(deps);
		expect(deps.closeTab).toHaveBeenCalledExactlyOnceWith(id);
		expect(deps.closePane).not.toHaveBeenCalled();
	});

	it("does nothing when there is nothing to close", () => {
		const deps = fakes();
		closeActiveTarget(deps);
		expect(deps.closeTab).not.toHaveBeenCalled();
		expect(deps.closePane).not.toHaveBeenCalled();
	});

	it("calls closePane when the active terminal is part of the split", () => {
		const { a } = agentSplit();
		terminalsStore.setActive(a);
		const deps = fakes();
		closeActiveTarget(deps);
		expect(deps.closePane).toHaveBeenCalledOnce();
		expect(deps.closeTab).not.toHaveBeenCalled();
	});

	it("calls closeTab, never closePane, for an orphan overlaying the split", () => {
		agentSplit();
		const main = addTerminal("main 1");
		terminalsStore.setActive(main);
		const deps = fakes();
		closeActiveTarget(deps);
		expect(deps.closeTab).toHaveBeenCalledExactlyOnceWith(main);
		expect(deps.closePane).not.toHaveBeenCalled();
	});
});

// The user-visible regression: repeated Ctrl-W with "main 1" in front of a hidden
// tmux-shim split used to destroy the split's agent terminals one pane at a time.
describe("closeActiveTarget with the real pane closer", () => {
	it("leaves the hidden agent split untouched when the front-most orphan is closed", () => {
		const { a, b } = agentSplit();
		const main = addTerminal("main 1");
		terminalsStore.setActive(main);
		const splitPanes = useSplitPanes();
		const closeTab = vi.fn((id: string) => terminalsStore.remove(id));

		closeActiveTarget({ closeTab, closePane: splitPanes.closeActivePane });

		expect(closeTab).toHaveBeenCalledExactlyOnceWith(main);
		expect(terminalsStore.get(a)).toBeDefined();
		expect(terminalsStore.get(b)).toBeDefined();
		expect(paneLayoutStore.isSplit()).toBe(true);
	});

	it("still closes the active pane's terminals when the user is looking at the split", () => {
		const { a, b } = agentSplit();
		terminalsStore.setActive(a);
		const splitPanes = useSplitPanes();
		const closeTab = vi.fn();

		closeActiveTarget({ closeTab, closePane: splitPanes.closeActivePane });

		expect(closeTab).not.toHaveBeenCalled();
		expect([a, b].filter((id) => terminalsStore.get(id)).length).toBe(1);
	});

	it("still cancels a freshly-split empty pane instead of destroying the terminal", () => {
		const id = addTerminal("T1");
		terminalsStore.setActive(id);
		const splitPanes = useSplitPanes();
		splitPanes.handleSplit("vertical");
		expect(paneLayoutStore.isSplit()).toBe(true);

		closeActiveTarget({ closeTab: vi.fn(), closePane: splitPanes.closeActivePane });

		expect(paneLayoutStore.isSplit()).toBe(false);
		expect(terminalsStore.get(id)).toBeDefined();
	});
});
