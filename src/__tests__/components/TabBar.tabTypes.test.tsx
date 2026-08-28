import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
	invoke: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/event", () => ({
	listen: vi.fn().mockResolvedValue(vi.fn()),
	emit: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/window", () => ({
	getCurrentWindow: vi.fn(() => ({
		listen: vi.fn().mockResolvedValue(vi.fn()),
		setTitle: vi.fn().mockResolvedValue(undefined),
	})),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
	open: vi.fn().mockResolvedValue(null),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
	openUrl: vi.fn().mockResolvedValue(undefined),
}));

import { TabBar } from "../../components/TabBar/TabBar";
import { diffTabsStore } from "../../stores/diffTabs";
import { editorTabsStore } from "../../stores/editorTabs";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { mdTabsStore } from "../../stores/mdTabs";
import { paneLayoutStore } from "../../stores/paneLayout";
import { repositoriesStore } from "../../stores/repositories";
import { settingsStore } from "../../stores/settings";
import { tabOrderingStore } from "../../stores/tabOrdering";
import { terminalsStore } from "../../stores/terminals";

/**
 * Every tab kind carries a distinct type class driving its color/border
 * (see docs/frontend/STYLE_GUIDE.md "Tab type color scheme"). This was
 * previously untested — TabBar.test.tsx's "tab-kind parity" suite asserts
 * presence and click-selection, not the type class or icon branch.
 */
describe("TabBar tab-type classes", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		localStorage.clear();
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
		for (const path of repositoriesStore.getPaths()) repositoriesStore.remove(path);
		repositoriesStore.setActive(null);
		for (const id of diffTabsStore.getIds()) diffTabsStore.remove(id);
		for (const id of mdTabsStore.getIds()) mdTabsStore.remove(id);
		for (const id of editorTabsStore.getIds()) editorTabsStore.remove(id);
		tabOrderingStore.clear();
		settingsStore.setTabOrderingMode("grouped-by-type");
		if (globalWorkspaceStore.isActive()) globalWorkspaceStore.deactivate();
		for (const id of globalWorkspaceStore.getPromotedIds()) globalWorkspaceStore.unpromote(id);

		// Diff/markdown/editor tabs are filtered by the active branch key
		// (TabBar.tsx visibleDiffIds/visibleMdIds/visibleEditIds) — without an
		// active repo+branch they render nothing at all, regardless of type.
		repositoriesStore.add({ path: "/repo", displayName: "repo" });
		repositoriesStore.setBranch("/repo", "main", { isMain: true, worktreePath: null });
		repositoriesStore.setActive("/repo");
		repositoriesStore.setActiveBranch("/repo", "main");
	});

	afterEach(() => {
		vi.useRealTimers();
		paneLayoutStore._testCancelPendingSave();
		repositoriesStore._testCancelPendingSave();
	});

	function renderBar() {
		return render(() => (
			<TabBar
				onTabSelect={() => {}}
				onTabClose={() => {}}
				onCloseOthers={() => {}}
				onCloseToRight={() => {}}
				onNewTab={() => {}}
			/>
		));
	}

	it("gives a diff tab the diffTab class", () => {
		const id = diffTabsStore.add("/repo", "/repo/change.ts", "M");
		const { container } = renderBar();
		expect(container.querySelector(`[data-tab-id="${id}"]`)?.classList.contains("diffTab")).toBe(true);
	});

	it("gives an editor tab the editTab class", () => {
		const id = editorTabsStore.add("/repo", "/repo/edit.ts");
		const { container } = renderBar();
		expect(container.querySelector(`[data-tab-id="${id}"]`)?.classList.contains("editTab")).toBe(true);
	});

	it("gives a markdown file tab the mdTab class", () => {
		const id = mdTabsStore.add("/repo", "/repo/readme.md");
		const { container } = renderBar();
		expect(container.querySelector(`[data-tab-id="${id}"]`)?.classList.contains("mdTab")).toBe(true);
	});

	it("gives a PR diff tab the diffTab class (the untested type === 'pr-diff' branch)", () => {
		const id = mdTabsStore.addPrDiff("/repo", 42, "Some PR", "diff --git a/x b/x");
		const { container } = renderBar();
		const tab = container.querySelector(`[data-tab-id="${id}"]`);
		expect(tab?.classList.contains("diffTab")).toBe(true);
		expect(tab?.classList.contains("mdTab")).toBe(false);
	});

	it("gives an html-preview tab its own htmlTab class, not panelTab (the orphaned --tab-html-rgb bug fix)", () => {
		const id = mdTabsStore.addHtmlPreview("/repo", "/repo/out.html");
		const { container } = renderBar();
		const tab = container.querySelector(`[data-tab-id="${id}"]`);
		expect(tab?.classList.contains("htmlTab")).toBe(true);
		expect(tab?.classList.contains("panelTab")).toBe(false);
		expect(tab?.classList.contains("diffTab")).toBe(false);
		expect(tab?.classList.contains("mdTab")).toBe(false);
	});

	it("still gives a plugin-panel-style tab (no dedicated type) the panelTab fallback class", () => {
		const id = mdTabsStore.addCommandOverview();
		const { container } = renderBar();
		const tab = container.querySelector(`[data-tab-id="${id}"]`);
		expect(tab?.classList.contains("panelTab")).toBe(true);
	});

	it("gives a remote terminal tab the remoteTab class", () => {
		const id = terminalsStore.add({
			name: "Remote",
			sessionId: null,
			fontSize: 14,
			cwd: null,
			awaitingInput: null,
			isRemote: true,
		});
		repositoriesStore.addTerminalToBranch("/repo", "main", id);
		const { container } = renderBar();
		expect(container.querySelector(`[data-tab-id="${id}"]`)?.classList.contains("remoteTab")).toBe(true);
	});

	it("does not give a local terminal tab the remoteTab class", () => {
		const id = terminalsStore.add({
			name: "Local",
			sessionId: null,
			fontSize: 14,
			cwd: null,
			awaitingInput: null,
		});
		repositoriesStore.addTerminalToBranch("/repo", "main", id);
		const { container } = renderBar();
		expect(container.querySelector(`[data-tab-id="${id}"]`)?.classList.contains("remoteTab")).toBe(false);
	});

	it("clicking a pr-diff tab still selects it (icon branch doesn't affect click wiring)", () => {
		const id = mdTabsStore.addPrDiff("/repo", 7, "PR seven", "diff");
		const { container } = renderBar();
		const tab = container.querySelector(`[data-tab-id="${id}"]`)!;
		fireEvent.click(tab);
		expect(mdTabsStore.state.activeId).toBe(id);
	});
});
