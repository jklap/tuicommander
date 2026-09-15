import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { resolveActiveAccentColor } from "../../components/PaneTree/PaneTree";
import type { PaneGroup } from "../../stores/paneLayout";
import { terminalsStore } from "../../stores/terminals";

/**
 * `resolveActiveAccentColor` backs both PaneTree.tsx's split-view pane
 * border and (via the same `TerminalData.accentColor` field) TerminalArea's
 * flat-view border — the tmux compatibility shim's `set-option ...
 * *-border-style` (Claude Code's per-teammate `--agent-color`). Extracted
 * as a pure exported function (same precedent as `tabColorClass` in this
 * same file) so this logic is testable without mounting the full,
 * Terminal/CanvasTerminal-heavy component tree.
 */
describe("resolveActiveAccentColor", () => {
	beforeEach(() => {
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	});

	afterEach(() => {
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	});

	function makeGroup(activeTabId: string | null, tabs: PaneGroup["tabs"]): PaneGroup {
		return { id: "g1", tabs, activeTabId };
	}

	it("returns undefined for an undefined group", () => {
		expect(resolveActiveAccentColor(undefined)).toBeUndefined();
	});

	it("returns the active terminal tab's accent color", () => {
		const id = terminalsStore.add({ name: "Term", sessionId: null, fontSize: 14, cwd: null, awaitingInput: null });
		terminalsStore.update(id, { accentColor: "blue" });
		const group = makeGroup(id, [{ id, type: "terminal" }]);
		expect(resolveActiveAccentColor(group)).toBe("blue");
	});

	it("returns undefined when the active terminal has no accent color set", () => {
		const id = terminalsStore.add({ name: "Term", sessionId: null, fontSize: 14, cwd: null, awaitingInput: null });
		const group = makeGroup(id, [{ id, type: "terminal" }]);
		expect(resolveActiveAccentColor(group)).toBeUndefined();
	});

	it("returns undefined when the active tab is not a terminal (e.g. a diff/markdown tab)", () => {
		const group = makeGroup("diff-1", [{ id: "diff-1", type: "diff" }]);
		expect(resolveActiveAccentColor(group)).toBeUndefined();
	});

	it("reads the ACTIVE tab specifically, not just any terminal tab in a multi-tab group", () => {
		const colored = terminalsStore.add({ name: "A", sessionId: null, fontSize: 14, cwd: null, awaitingInput: null });
		terminalsStore.update(colored, { accentColor: "red" });
		const plain = terminalsStore.add({ name: "B", sessionId: null, fontSize: 14, cwd: null, awaitingInput: null });
		const group = makeGroup(plain, [
			{ id: colored, type: "terminal" },
			{ id: plain, type: "terminal" },
		]);
		expect(resolveActiveAccentColor(group)).toBeUndefined();
	});
});
