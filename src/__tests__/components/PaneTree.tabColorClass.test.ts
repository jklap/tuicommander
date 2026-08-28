import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { tabColorClass } from "../../components/PaneTree/PaneTree";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { mdTabsStore } from "../../stores/mdTabs";
import { terminalsStore } from "../../stores/terminals";

/**
 * `tabColorClass` (PaneTree.tsx) had zero tests. It previously returned ""
 * unconditionally for every terminal tab and fell every non-file,
 * non-pr-diff markdown tab (including html-preview) into "pane-tab-panel" —
 * both gaps closed as part of wiring PaneTree.css onto the indicator
 * registry (see PaneTree.css's ".pane-tab-remote"/".pane-tab-html" comments).
 */
describe("tabColorClass", () => {
	beforeEach(() => {
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const id of diffTabsStore.getIds()) diffTabsStore.remove(id);
		for (const id of mdTabsStore.getIds()) mdTabsStore.remove(id);
		for (const id of editorTabsStore.getIds()) editorTabsStore.remove(id);
	});

	afterEach(() => {
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const id of diffTabsStore.getIds()) diffTabsStore.remove(id);
		for (const id of mdTabsStore.getIds()) mdTabsStore.remove(id);
		for (const id of editorTabsStore.getIds()) editorTabsStore.remove(id);
	});

	it("gives a plain local terminal tab no color class", () => {
		const id = terminalsStore.add({ name: "Local", sessionId: null, fontSize: 14, cwd: null, awaitingInput: null });
		expect(tabColorClass({ id, type: "terminal" })).toBe("");
	});

	it("gives a remote terminal tab pane-tab-remote", () => {
		const id = terminalsStore.add({
			name: "Remote",
			sessionId: null,
			fontSize: 14,
			cwd: null,
			awaitingInput: null,
			isRemote: true,
		});
		expect(tabColorClass({ id, type: "terminal" })).toBe("pane-tab-remote");
	});

	it("gives a diff tab pane-tab-diff", () => {
		const id = diffTabsStore.add("/repo", "/repo/change.ts", "M");
		expect(tabColorClass({ id, type: "diff" })).toBe("pane-tab-diff");
	});

	it("gives an editor tab pane-tab-edit", () => {
		const id = editorTabsStore.add("/repo", "/repo/edit.ts");
		expect(tabColorClass({ id, type: "editor" })).toBe("pane-tab-edit");
	});

	it("gives a markdown file tab pane-tab-md", () => {
		const id = mdTabsStore.add("/repo", "/repo/readme.md");
		expect(tabColorClass({ id, type: "markdown" })).toBe("pane-tab-md");
	});

	it("gives a pr-diff markdown tab pane-tab-diff", () => {
		const id = mdTabsStore.addPrDiff("/repo", 1, "PR", "diff");
		expect(tabColorClass({ id, type: "markdown" })).toBe("pane-tab-diff");
	});

	it("gives an html-preview markdown tab pane-tab-html, not pane-tab-panel", () => {
		const id = mdTabsStore.addHtmlPreview("/repo", "/repo/out.html");
		expect(tabColorClass({ id, type: "markdown" })).toBe("pane-tab-html");
	});

	it("falls back to pane-tab-panel for other markdown tab kinds", () => {
		const id = mdTabsStore.addCommandOverview();
		expect(tabColorClass({ id, type: "markdown" })).toBe("pane-tab-panel");
	});

	it("falls back to pane-tab-panel when the markdown tab id resolves to nothing", () => {
		expect(tabColorClass({ id: "missing", type: "markdown" })).toBe("pane-tab-panel");
	});
});
