import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// PaneTree's terminal content area mounts the real Terminal component, which
// needs a live PTY/xterm — irrelevant to the mini-tab-bar (globe/repo badges)
// this file actually tests, and heavy to satisfy for real in jsdom.
vi.mock("../../components/Terminal", () => ({
	Terminal: () => null,
}));

import { PaneNodeView } from "../../components/PaneTree/PaneTree";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { paneLayoutStore } from "../../stores/paneLayout";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";

describe("PaneTree", () => {
	beforeEach(() => {
		for (const id of terminalsStore.getIds()) {
			terminalsStore.remove(id);
		}
		for (const path of repositoriesStore.getPaths()) {
			repositoriesStore.remove(path);
		}
		repositoriesStore.setActive(null);
		paneLayoutStore.reset();
		if (globalWorkspaceStore.isActive()) {
			globalWorkspaceStore.deactivate();
		}
		for (const id of globalWorkspaceStore.getPromotedIds()) {
			globalWorkspaceStore.unpromote(id);
		}
	});

	afterEach(() => {
		paneLayoutStore._testCancelPendingSave();
		repositoriesStore._testCancelPendingSave();
	});

	/** Two real, live terminals in one group — PaneGroupView's mini tab bar
	 *  (`showTabBar()`) only renders once `aliveTabs().length > 1`. */
	function seedTwoTabGroup(): { groupId: string; termA: string; termB: string } {
		const termA = terminalsStore.add({ name: "A", sessionId: null, fontSize: 14, cwd: null, awaitingInput: null });
		const termB = terminalsStore.add({ name: "B", sessionId: null, fontSize: 14, cwd: null, awaitingInput: null });
		paneLayoutStore.restore({
			root: { type: "leaf", id: "g1" },
			groups: {
				g1: {
					id: "g1",
					tabs: [
						{ id: termA, type: "terminal" },
						{ id: termB, type: "terminal" },
					],
					activeTabId: termA,
				},
			},
			activeGroupId: "g1",
		});
		return { groupId: "g1", termA, termB };
	}

	function renderTree() {
		// Snapshot the root as a plain value before rendering — passing
		// paneLayoutStore.getRoot()! inline makes Solid treat it as a reactive
		// prop getter (PaneNodeView's `node` prop is typed as a plain value, not
		// an accessor), and something inside the mounted tree writes to
		// paneLayoutStore during mount, transiently re-evaluating it against a
		// stale/null root.
		const node = paneLayoutStore.getRoot()!;
		return render(() => (
			<PaneNodeView node={node} onCloseTab={vi.fn()} onOpenFilePath={vi.fn()} onTerminalFocus={vi.fn()} />
		));
	}

	describe("globe badge", () => {
		it("shows on a promoted tab when the manual Global Workspace isn't the active view", () => {
			const { termA } = seedTwoTabGroup();
			globalWorkspaceStore.promote(termA);

			const { container } = renderTree();

			const tab = container.querySelector(`[data-drop-target="pane"] .pane-tab-bar .pane-tab`)!;
			expect(tab.querySelector(".pane-tab-globe")).not.toBeNull();
		});

		it("does not show on a non-promoted tab", () => {
			seedTwoTabGroup();

			const { container } = renderTree();

			for (const tab of container.querySelectorAll(".pane-tab-bar .pane-tab")) {
				expect(tab.querySelector(".pane-tab-globe")).toBeNull();
			}
		});

		/**
		 * PaneTree's globe badge used to have no active-view guard at all, unlike
		 * TabViews.tsx's equivalent (which hides while `isManualWorkspaceActive()`
		 * is true, since every tab in that view is already known-promoted by
		 * definition, so the icon is redundant clutter there). Added for parity.
		 */
		it("hides while the manual Global Workspace is the active view (parity with TabViews.tsx)", () => {
			const { termA } = seedTwoTabGroup();
			globalWorkspaceStore.promote(termA);
			globalWorkspaceStore.activate();
			expect(globalWorkspaceStore.isManualWorkspaceActive()).toBe(true);

			const { container } = renderTree();

			for (const tab of container.querySelectorAll(".pane-tab-bar .pane-tab")) {
				expect(tab.querySelector(".pane-tab-globe")).toBeNull();
			}
		});

		it("clicking it unpromotes the tab without closing it", () => {
			const { termA } = seedTwoTabGroup();
			globalWorkspaceStore.promote(termA);

			const { container } = renderTree();
			const globe = container.querySelector(".pane-tab-globe")!;
			fireEvent.click(globe);

			expect(globalWorkspaceStore.isPromoted(termA)).toBe(false);
			// Not closed — still a live terminal.
			expect(terminalsStore.get(termA)).toBeDefined();
		});
	});

	describe("repo badge", () => {
		it("shows only while the manual Global Workspace is the active view", () => {
			// Both tabs promoted: activate() replaces paneLayoutStore with
			// globalWorkspaceStore's OWN cached layout for the active scope, so
			// the mini tab bar (which needs 2+ alive tabs to render at all) must
			// come from what's actually promoted, not the group seedTwoTabGroup
			// wrote directly into paneLayoutStore.
			const { termA, termB } = seedTwoTabGroup();
			globalWorkspaceStore.promote(termA);
			globalWorkspaceStore.promote(termB);
			globalWorkspaceStore.activate();

			const { container } = renderTree();

			expect(container.querySelector(".pane-tab-repo-badge")).not.toBeNull();
		});

		it("does not show for a repo's own auto-consolidated view (not the manual one)", () => {
			const { termA, termB } = seedTwoTabGroup();
			globalWorkspaceStore.syncScopeMembers("/test/consolidated", [termA, termB]);
			globalWorkspaceStore.setScope("/test/consolidated");
			globalWorkspaceStore.activate();
			expect(globalWorkspaceStore.isActive()).toBe(true);
			expect(globalWorkspaceStore.isManualWorkspaceActive()).toBe(false);

			try {
				const { container } = renderTree();
				expect(container.querySelector(".pane-tab-repo-badge")).toBeNull();
			} finally {
				globalWorkspaceStore.deactivate();
				globalWorkspaceStore.syncScopeMembers("/test/consolidated", []);
				globalWorkspaceStore.setScope("__manual__");
			}
		});

		it("does not show when no Global Workspace view is active at all", () => {
			seedTwoTabGroup();

			const { container } = renderTree();

			expect(container.querySelector(".pane-tab-repo-badge")).toBeNull();
		});
	});

	describe("promote/unpromote context menu", () => {
		it("promotes a non-promoted tab via 'Promote to Global Workspace'", () => {
			const { termA } = seedTwoTabGroup();

			const { container } = renderTree();
			const tab = container.querySelectorAll(".pane-tab-bar .pane-tab")[0]!;
			vi.spyOn(tab, "getBoundingClientRect").mockReturnValue({
				left: 0,
				bottom: 0,
				top: 0,
				right: 0,
				width: 0,
				height: 0,
				x: 0,
				y: 0,
				toJSON: () => {},
			} as DOMRect);
			fireEvent.contextMenu(tab);

			const menus = container.querySelectorAll(".menu");
			expect(menus.length).toBeGreaterThan(0);
			const promoteItem = Array.from(menus[menus.length - 1].querySelectorAll(".label")).find(
				(l) => l.textContent === "Promote to Global Workspace",
			);
			expect(promoteItem).toBeDefined();
			fireEvent.click(promoteItem!);

			expect(globalWorkspaceStore.isPromoted(termA)).toBe(true);
		});

		it("unpromotes an already-promoted tab via 'Remove from Global Workspace'", () => {
			const { termA } = seedTwoTabGroup();
			globalWorkspaceStore.promote(termA);

			const { container } = renderTree();
			const tab = container.querySelectorAll(".pane-tab-bar .pane-tab")[0]!;
			vi.spyOn(tab, "getBoundingClientRect").mockReturnValue({
				left: 0,
				bottom: 0,
				top: 0,
				right: 0,
				width: 0,
				height: 0,
				x: 0,
				y: 0,
				toJSON: () => {},
			} as DOMRect);
			fireEvent.contextMenu(tab);

			const menus = container.querySelectorAll(".menu");
			const removeItem = Array.from(menus[menus.length - 1].querySelectorAll(".label")).find(
				(l) => l.textContent === "Remove from Global Workspace",
			);
			expect(removeItem).toBeDefined();
			fireEvent.click(removeItem!);

			expect(globalWorkspaceStore.isPromoted(termA)).toBe(false);
			expect(terminalsStore.get(termA)).toBeDefined();
		});
	});
});
