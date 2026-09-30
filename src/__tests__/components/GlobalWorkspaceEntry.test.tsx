import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { GlobalWorkspaceEntry } from "../../components/Sidebar/GlobalWorkspaceEntry";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { paneLayoutStore } from "../../stores/paneLayout";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";

/**
 * GlobalWorkspaceEntry.tsx had zero test coverage before this file — no
 * `GlobalWorkspaceEntry.test.tsx` existed at all, and no other test rendered
 * it even incidentally via a parent (Sidebar.test.tsx doesn't touch it
 * either). It's also the component at the center of every symptom in this
 * fix: the badge count, the highlight, and the one-way click behavior.
 */
describe("GlobalWorkspaceEntry", () => {
	beforeEach(() => {
		for (const id of terminalsStore.getIds()) {
			terminalsStore.remove(id);
		}
		for (const path of repositoriesStore.getPaths()) {
			repositoriesStore.remove(path);
		}
		repositoriesStore.setActive(null);
		if (globalWorkspaceStore.isActive()) {
			globalWorkspaceStore.deactivate();
		}
		for (const id of globalWorkspaceStore.getPromotedIds()) {
			globalWorkspaceStore.unpromote(id);
		}
		globalWorkspaceStore.setScope("__manual__");
	});

	afterEach(() => {
		repositoriesStore._testCancelPendingSave();
		paneLayoutStore._testCancelPendingSave();
	});

	function addTerminal(name: string) {
		return terminalsStore.add({ name, sessionId: null, fontSize: 14, cwd: null, awaitingInput: null });
	}

	describe("visibility", () => {
		it("does not render when nothing is manually promoted", () => {
			const { container } = render(() => <GlobalWorkspaceEntry />);
			expect(container.querySelector(".globalWorkspaceEntry")).toBeNull();
		});

		it("renders once a terminal is manually promoted", () => {
			const id = addTerminal("t1");
			globalWorkspaceStore.promote(id);

			const { container } = render(() => <GlobalWorkspaceEntry />);
			expect(container.querySelector(".globalWorkspaceEntry")).not.toBeNull();
		});
	});

	describe("badge count", () => {
		it("counts only manually-promoted, live terminals", () => {
			const a = addTerminal("a");
			const b = addTerminal("b");
			globalWorkspaceStore.promote(a);
			globalWorkspaceStore.promote(b);

			const { container } = render(() => <GlobalWorkspaceEntry />);
			expect(container.querySelector(".globalWorkspaceBadge")?.textContent).toBe("2");
		});

		/**
		 * A promoted id can outlive the terminal it names if the normal
		 * onTerminalRemoved cleanup path never fires for it (src/AGENTS.md's
		 * "paneLayoutStore Ghost Tabs" section names this exact symptom: "a
		 * stuck entry in the sidebar's Global Workspace badge count"). Simulated
		 * here by promoting, then removing the terminal via a path that leaves
		 * it dangling in `promoted` — onTerminalRemoved normally sweeps this,
		 * so we bypass it by promoting an id that was never a real terminal at
		 * all, which is the shape a ghost eventually decays to.
		 */
		it("excludes a promoted id that no longer has a live terminal", () => {
			const live = addTerminal("live");
			globalWorkspaceStore.promote(live);
			globalWorkspaceStore.promote("ghost-id-not-in-terminals-store");

			const { container } = render(() => <GlobalWorkspaceEntry />);
			expect(container.querySelector(".globalWorkspaceBadge")?.textContent).toBe("1");
		});

		it("does not count a repo's own auto-consolidated scope members, only the manual bucket", () => {
			const manual = addTerminal("manual");
			globalWorkspaceStore.promote(manual);
			globalWorkspaceStore.syncScopeMembers("/test/consolidated", [addTerminal("wt-1"), addTerminal("wt-2")]);

			const { container } = render(() => <GlobalWorkspaceEntry />);
			expect(container.querySelector(".globalWorkspaceBadge")?.textContent).toBe("1");

			globalWorkspaceStore.syncScopeMembers("/test/consolidated", []);
		});
	});

	describe("highlight", () => {
		it("is not highlighted when a repo's own auto-consolidated scope is active instead of the manual one", () => {
			const manual = addTerminal("manual");
			globalWorkspaceStore.promote(manual);
			const wt = addTerminal("wt-1");
			globalWorkspaceStore.syncScopeMembers("/test/consolidated", [wt]);
			globalWorkspaceStore.setScope("/test/consolidated");
			globalWorkspaceStore.activate();

			try {
				const { container } = render(() => <GlobalWorkspaceEntry />);
				expect(globalWorkspaceStore.isActive()).toBe(true);
				expect(container.querySelector(".globalWorkspaceEntry")?.classList.contains("globalWorkspaceActive")).toBe(
					false,
				);
			} finally {
				globalWorkspaceStore.deactivate();
				globalWorkspaceStore.syncScopeMembers("/test/consolidated", []);
				globalWorkspaceStore.setScope("__manual__");
			}
		});

		it("is highlighted once the manual workspace is activated", () => {
			const id = addTerminal("t1");
			globalWorkspaceStore.promote(id);
			globalWorkspaceStore.activate();

			const { container } = render(() => <GlobalWorkspaceEntry />);
			expect(container.querySelector(".globalWorkspaceEntry")?.classList.contains("globalWorkspaceActive")).toBe(true);
		});
	});

	describe("click behavior (one-way — never deactivates)", () => {
		it("activates the manual workspace, forcing scope to MANUAL_SCOPE, even while a different scope is ambient", () => {
			const manual = addTerminal("manual");
			globalWorkspaceStore.promote(manual);
			globalWorkspaceStore.syncScopeMembers("/test/consolidated", [addTerminal("wt-1")]);
			globalWorkspaceStore.setScope("/test/consolidated");

			const { container } = render(() => <GlobalWorkspaceEntry />);
			fireEvent.click(container.querySelector(".globalWorkspaceEntry")!);

			expect(globalWorkspaceStore.getScope()).toBe("__manual__");
			expect(globalWorkspaceStore.isManualWorkspaceActive()).toBe(true);

			globalWorkspaceStore.syncScopeMembers("/test/consolidated", []);
		});

		it("does nothing when clicked again while the manual workspace is already the active view", () => {
			const id = addTerminal("t1");
			globalWorkspaceStore.promote(id);
			globalWorkspaceStore.activate();
			expect(globalWorkspaceStore.isManualWorkspaceActive()).toBe(true);

			const { container } = render(() => <GlobalWorkspaceEntry />);
			fireEvent.click(container.querySelector(".globalWorkspaceEntry")!);

			// Still active — the pill is not a toggle. The only way to leave the
			// manual Global Workspace view is clicking a terminal in the sidebar
			// (navigateToTerminal.ts), not clicking this pill again.
			expect(globalWorkspaceStore.isActive()).toBe(true);
			expect(globalWorkspaceStore.isManualWorkspaceActive()).toBe(true);
		});
	});
});
