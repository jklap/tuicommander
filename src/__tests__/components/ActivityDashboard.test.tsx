import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";

const { mockNavigateToTerminal } = vi.hoisted(() => ({ mockNavigateToTerminal: vi.fn() }));

vi.mock("../../utils/navigateToTerminal", () => ({ navigateToTerminal: mockNavigateToTerminal }));
// Not under test here — stub to keep panelRouter/Tauri surface out of these tests.
vi.mock("../../components/ui/PanelWindowControls", () => ({ PanelWindowControls: () => null }));

import {
	ActivityDashboard,
	moveActivitySelection,
	type TerminalRow,
} from "../../components/ActivityDashboard/ActivityDashboard";
import { activityDashboardStore } from "../../stores/activityDashboard";
import { globalWorkspaceStore } from "../../stores/globalWorkspace";
import { __resetModalStackForTest } from "../../stores/modalStack";

function row(overrides: Partial<TerminalRow> = {}): TerminalRow {
	return {
		id: "t1",
		name: "shell",
		project: null,
		projectColor: undefined,
		agent: "claude",
		status: { label: "Idle", className: "statusIdle" },
		isWorking: false,
		lastDataAt: null,
		idleSince: null,
		lastPrompt: null,
		agentIntent: null,
		currentTask: null,
		activeSubTasks: 0,
		isActive: false,
		isPromoted: false,
		...overrides,
	};
}

const rowEls = () => Array.from(document.querySelectorAll(".row"));
const emptyEl = () => document.querySelector(".empty");
const overlayEl = () => document.querySelector(".overlay");
const footerEl = () => document.querySelector(".footer");
const rowFor = (id: string) => document.querySelector(`[data-term-id="${id}"]`);
const selectedRowEl = () => document.querySelector(".selectedRow");

/** Dispatched on document.body (a descendant of document, not document itself) so the
 *  dashboard's document-CAPTURE listener actually runs before any target-phase
 *  listener — see stores/modalStack.test.ts's identical convention for Escape. */
function dispatchKey(key: string, opts: Partial<KeyboardEvent> = {}): KeyboardEvent {
	const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...opts });
	document.body.dispatchEvent(event);
	return event;
}

describe("ActivityDashboard", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		__resetModalStackForTest();
		activityDashboardStore.close();
	});

	afterEach(() => {
		document.body.innerHTML = "";
		activityDashboardStore.close();
	});

	describe("embedded rendering", () => {
		it("renders one .row per terminal, in the given order", () => {
			render(() => (
				<ActivityDashboard embedded terminals={() => [row({ id: "a" }), row({ id: "b" }), row({ id: "c" })]} />
			));
			expect(rowEls().map((el) => el.textContent)).toHaveLength(3);
		});

		it("shows the empty state when there are no terminals", () => {
			render(() => <ActivityDashboard embedded terminals={() => []} />);
			expect(emptyEl()?.textContent).toBe("No active terminals");
			expect(rowEls()).toHaveLength(0);
		});

		it("renders without an .overlay wrapper", () => {
			render(() => <ActivityDashboard embedded terminals={() => [row()]} />);
			expect(overlayEl()).toBeNull();
		});

		it("renders the terminal count in the footer", () => {
			render(() => <ActivityDashboard embedded terminals={() => [row({ id: "a" }), row({ id: "b" })]} />);
			expect(footerEl()?.textContent).toContain("2 terminal(s)");
		});
	});

	describe("row styling", () => {
		it("applies .idleRow only to non-working rows", () => {
			render(() => (
				<ActivityDashboard
					embedded
					terminals={() => [row({ id: "working", isWorking: true }), row({ id: "idle", isWorking: false })]}
				/>
			));
			const [working, idle] = rowEls();
			expect(working.classList.contains("idleRow")).toBe(false);
			expect(idle.classList.contains("idleRow")).toBe(true);
		});

		it("applies .activeRow only to the active terminal", () => {
			render(() => (
				<ActivityDashboard
					embedded
					terminals={() => [row({ id: "active", isActive: true }), row({ id: "inactive", isActive: false })]}
				/>
			));
			const [active, inactive] = rowEls();
			expect(active.classList.contains("activeRow")).toBe(true);
			expect(inactive.classList.contains("activeRow")).toBe(false);
		});
	});

	describe("last-activity cell", () => {
		it("is blank for a working row", () => {
			render(() => <ActivityDashboard embedded terminals={() => [row({ isWorking: true, idleSince: 1000 })]} />);
			expect(document.querySelector(".lastActivity")?.textContent).toBe("");
		});

		it("shows a relative time for an idle row", () => {
			const idleSince = Date.now() - 2 * 60 * 1000; // 2 minutes ago
			render(() => <ActivityDashboard embedded terminals={() => [row({ isWorking: false, idleSince })]} />);
			expect(document.querySelector(".lastActivity")?.textContent).toBe("2m ago");
		});

		it("shows 'never' for an idle row with no idleSince", () => {
			render(() => <ActivityDashboard embedded terminals={() => [row({ isWorking: false, idleSince: null })]} />);
			expect(document.querySelector(".lastActivity")?.textContent).toBe("never");
		});
	});

	describe("row click", () => {
		it("calls props.onSelect and does not call navigateToTerminal", () => {
			const onSelect = vi.fn();
			render(() => <ActivityDashboard embedded onSelect={onSelect} terminals={() => [row({ id: "t1" })]} />);
			fireEvent.click(rowEls()[0]);
			expect(onSelect).toHaveBeenCalledWith("t1");
			expect(mockNavigateToTerminal).not.toHaveBeenCalled();
		});

		it("falls back to navigateToTerminal when no onSelect is given", () => {
			render(() => <ActivityDashboard embedded terminals={() => [row({ id: "t1" })]} />);
			fireEvent.click(rowEls()[0]);
			expect(mockNavigateToTerminal).toHaveBeenCalledWith("t1");
		});

		it("closes the overlay on click when not embedded", () => {
			activityDashboardStore.open();
			render(() => <ActivityDashboard terminals={() => [row({ id: "t1" })]} />);
			fireEvent.click(rowEls()[0]);
			expect(activityDashboardStore.state.isOpen).toBe(false);
		});

		it("does not close anything when embedded", () => {
			activityDashboardStore.open();
			render(() => <ActivityDashboard embedded terminals={() => [row({ id: "t1" })]} />);
			fireEvent.click(rowEls()[0]);
			expect(activityDashboardStore.state.isOpen).toBe(true);
		});
	});

	describe("promote button", () => {
		it("calls props.onPromote and does not also select the row", () => {
			const onPromote = vi.fn();
			const onSelect = vi.fn();
			render(() => (
				<ActivityDashboard embedded onPromote={onPromote} onSelect={onSelect} terminals={() => [row({ id: "t1" })]} />
			));
			fireEvent.click(document.querySelector(".promoteBtn")!);
			expect(onPromote).toHaveBeenCalledWith("t1");
			expect(onSelect).not.toHaveBeenCalled();
		});

		it("falls back to globalWorkspaceStore.togglePromote when no onPromote is given", () => {
			const spy = vi.spyOn(globalWorkspaceStore, "togglePromote").mockImplementation(() => {});
			render(() => <ActivityDashboard embedded terminals={() => [row({ id: "t1" })]} />);
			fireEvent.click(document.querySelector(".promoteBtn")!);
			expect(spy).toHaveBeenCalledWith("t1");
			spy.mockRestore();
		});
	});

	describe("overlay mode", () => {
		it("renders nothing when the store is closed", () => {
			activityDashboardStore.close();
			render(() => <ActivityDashboard terminals={() => [row()]} />);
			expect(overlayEl()).toBeNull();
		});

		it("renders the overlay when the store is open", () => {
			activityDashboardStore.open();
			render(() => <ActivityDashboard terminals={() => [row()]} />);
			expect(overlayEl()).not.toBeNull();
		});

		it("closes on backdrop click", () => {
			activityDashboardStore.open();
			render(() => <ActivityDashboard terminals={() => [row()]} />);
			fireEvent.click(overlayEl()!);
			expect(activityDashboardStore.state.isOpen).toBe(false);
		});

		it("does not close on a click inside the dashboard panel", () => {
			activityDashboardStore.open();
			render(() => <ActivityDashboard terminals={() => [row()]} />);
			fireEvent.click(document.querySelector(".dashboard")!);
			expect(activityDashboardStore.state.isOpen).toBe(true);
		});

		it("closes on Escape via the central modal stack", () => {
			activityDashboardStore.open();
			render(() => <ActivityDashboard terminals={() => [row()]} />);
			const e = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
			document.body.dispatchEvent(e);
			expect(activityDashboardStore.state.isOpen).toBe(false);
		});
	});

	describe("sub-row precedence", () => {
		it("renders currentTask", () => {
			render(() => <ActivityDashboard embedded terminals={() => [row({ currentTask: "Running migration" })]} />);
			expect(document.body.textContent).toContain("Running migration");
		});

		it("renders the active sub-task count", () => {
			render(() => <ActivityDashboard embedded terminals={() => [row({ activeSubTasks: 3 })]} />);
			expect(document.body.textContent).toContain("3 sub-tasks running");
		});

		it("renders agentIntent", () => {
			render(() => <ActivityDashboard embedded terminals={() => [row({ agentIntent: "Refactoring the parser" })]} />);
			expect(document.body.textContent).toContain("Refactoring the parser");
		});

		it("renders lastPrompt only when agentIntent is absent", () => {
			const { unmount } = render(() => (
				<ActivityDashboard embedded terminals={() => [row({ lastPrompt: "Write tests", agentIntent: null })]} />
			));
			expect(document.body.textContent).toContain("Write tests");
			unmount();

			render(() => (
				<ActivityDashboard embedded terminals={() => [row({ lastPrompt: "Write tests", agentIntent: "Testing" })]} />
			));
			expect(document.body.textContent).not.toContain("Write tests");
			expect(document.body.textContent).toContain("Testing");
		});
	});

	describe("keyboard navigation", () => {
		it("ArrowDown from no selection selects the first row", () => {
			activityDashboardStore.open();
			render(() => <ActivityDashboard terminals={() => [row({ id: "a" }), row({ id: "b" })]} />);
			dispatchKey("ArrowDown");
			expect(rowFor("a")?.classList.contains("selectedRow")).toBe(true);
			expect(selectedRowEl()).toBe(rowFor("a"));
		});

		it("ArrowUp from no selection selects the last row", () => {
			activityDashboardStore.open();
			render(() => <ActivityDashboard terminals={() => [row({ id: "a" }), row({ id: "b" })]} />);
			dispatchKey("ArrowUp");
			expect(rowFor("b")?.classList.contains("selectedRow")).toBe(true);
		});

		it("walks down and clamps at the last row without wrapping", () => {
			activityDashboardStore.open();
			render(() => <ActivityDashboard terminals={() => [row({ id: "a" }), row({ id: "b" })]} />);
			dispatchKey("ArrowDown");
			dispatchKey("ArrowDown");
			expect(rowFor("b")?.classList.contains("selectedRow")).toBe(true);
			dispatchKey("ArrowDown");
			expect(rowFor("b")?.classList.contains("selectedRow")).toBe(true);
		});

		it("walks up and clamps at the first row without wrapping", () => {
			activityDashboardStore.open();
			render(() => <ActivityDashboard terminals={() => [row({ id: "a" }), row({ id: "b" })]} />);
			dispatchKey("ArrowUp");
			dispatchKey("ArrowUp");
			expect(rowFor("a")?.classList.contains("selectedRow")).toBe(true);
			dispatchKey("ArrowUp");
			expect(rowFor("a")?.classList.contains("selectedRow")).toBe(true);
		});

		it("Return with a selection activates that row", () => {
			const onSelect = vi.fn();
			activityDashboardStore.open();
			render(() => <ActivityDashboard onSelect={onSelect} terminals={() => [row({ id: "a" }), row({ id: "b" })]} />);
			dispatchKey("ArrowDown");
			dispatchKey("Enter");
			expect(onSelect).toHaveBeenCalledWith("a");
		});

		it("Return with no selection does nothing", () => {
			const onSelect = vi.fn();
			activityDashboardStore.open();
			render(() => <ActivityDashboard onSelect={onSelect} terminals={() => [row({ id: "a" })]} />);
			dispatchKey("Enter");
			expect(onSelect).not.toHaveBeenCalled();
		});

		it("digit 3 activates the third row", () => {
			const onSelect = vi.fn();
			activityDashboardStore.open();
			render(() => (
				<ActivityDashboard
					onSelect={onSelect}
					terminals={() => [row({ id: "a" }), row({ id: "b" }), row({ id: "c" })]}
				/>
			));
			dispatchKey("3");
			expect(onSelect).toHaveBeenCalledWith("c");
		});

		it("digit 1 activates the first row", () => {
			// Activating a row closes the (non-embedded) overlay — same as a mouse click —
			// so this is a separate case rather than a second dispatch reusing one render.
			const onSelect = vi.fn();
			activityDashboardStore.open();
			render(() => (
				<ActivityDashboard
					onSelect={onSelect}
					terminals={() => [row({ id: "a" }), row({ id: "b" }), row({ id: "c" })]}
				/>
			));
			dispatchKey("1");
			expect(onSelect).toHaveBeenCalledWith("a");
		});

		it("swallows an out-of-range digit (prevents default) without activating anything", () => {
			const onSelect = vi.fn();
			activityDashboardStore.open();
			render(() => <ActivityDashboard onSelect={onSelect} terminals={() => [row({ id: "a" })]} />);
			const event = dispatchKey("7");
			expect(onSelect).not.toHaveBeenCalled();
			// Still consumed — an out-of-range digit must not fall through to the PTY.
			expect(event.defaultPrevented).toBe(true);
		});

		it("does not handle a modified digit — Cmd+1 is left for the global switch-tab-1 shortcut", () => {
			const onSelect = vi.fn();
			activityDashboardStore.open();
			render(() => <ActivityDashboard onSelect={onSelect} terminals={() => [row({ id: "a" })]} />);
			const event = dispatchKey("1", { metaKey: true });
			expect(onSelect).not.toHaveBeenCalled();
			expect(event.defaultPrevented).toBe(false);
		});

		it("keeps the cursor on the same terminal id across a reorder, not the same index", () => {
			activityDashboardStore.open();
			const [terms, setTerms] = createSignal([row({ id: "a" }), row({ id: "b" })]);
			render(() => <ActivityDashboard terminals={terms} />);
			dispatchKey("ArrowDown"); // selects "a" (currently at index 0)
			expect(rowFor("a")?.classList.contains("selectedRow")).toBe(true);

			// Same ids, swapped order — simulates a spine reorder (e.g. an idle sort
			// change) with no keypress involved.
			setTerms([row({ id: "b" }), row({ id: "a" })]);

			expect(rowFor("a")?.classList.contains("selectedRow")).toBe(true);
			expect(rowFor("b")?.classList.contains("selectedRow")).toBe(false);
		});

		it("clears the selection when the selected terminal leaves the list", () => {
			activityDashboardStore.open();
			const [terms, setTerms] = createSignal([row({ id: "a" }), row({ id: "b" })]);
			render(() => <ActivityDashboard terminals={terms} />);
			dispatchKey("ArrowDown");
			expect(rowFor("a")?.classList.contains("selectedRow")).toBe(true);

			setTerms([row({ id: "b" })]);

			expect(rowFor("a")).toBeNull();
			expect(selectedRowEl()).toBeNull();
		});

		it("is inert while the overlay is closed", () => {
			const onSelect = vi.fn();
			activityDashboardStore.close();
			render(() => <ActivityDashboard onSelect={onSelect} terminals={() => [row({ id: "a" })]} />);
			dispatchKey("ArrowDown");
			dispatchKey("Enter");
			expect(onSelect).not.toHaveBeenCalled();
		});

		it("no longer responds after unmount", () => {
			const onSelect = vi.fn();
			activityDashboardStore.open();
			const { unmount } = render(() => <ActivityDashboard onSelect={onSelect} terminals={() => [row({ id: "a" })]} />);
			unmount();
			dispatchKey("ArrowDown");
			dispatchKey("Enter");
			expect(onSelect).not.toHaveBeenCalled();
		});

		describe("embedded mode", () => {
			it("Return routes through props.onSelect and does not close anything", () => {
				const onSelect = vi.fn();
				activityDashboardStore.close();
				render(() => <ActivityDashboard embedded onSelect={onSelect} terminals={() => [row({ id: "a" })]} />);
				dispatchKey("ArrowDown");
				dispatchKey("Enter");
				expect(onSelect).toHaveBeenCalledWith("a");
				expect(activityDashboardStore.state.isOpen).toBe(false);
			});

			it("a digit routes through props.onSelect", () => {
				const onSelect = vi.fn();
				render(() => (
					<ActivityDashboard embedded onSelect={onSelect} terminals={() => [row({ id: "a" }), row({ id: "b" })]} />
				));
				dispatchKey("2");
				expect(onSelect).toHaveBeenCalledWith("b");
			});
		});
	});
});

describe("moveActivitySelection", () => {
	it("returns null for an empty list", () => {
		expect(moveActivitySelection([], null, 1)).toBeNull();
		expect(moveActivitySelection([], null, -1)).toBeNull();
	});

	it("from no selection, down picks the first and up picks the last", () => {
		expect(moveActivitySelection(["a", "b", "c"], null, 1)).toBe("a");
		expect(moveActivitySelection(["a", "b", "c"], null, -1)).toBe("c");
	});

	it("a single-row list always resolves to that row", () => {
		expect(moveActivitySelection(["a"], null, 1)).toBe("a");
		expect(moveActivitySelection(["a"], "a", 1)).toBe("a");
		expect(moveActivitySelection(["a"], "a", -1)).toBe("a");
	});

	it("walks down and clamps at the end", () => {
		expect(moveActivitySelection(["a", "b", "c"], "a", 1)).toBe("b");
		expect(moveActivitySelection(["a", "b", "c"], "c", 1)).toBe("c");
	});

	it("walks up and clamps at the start", () => {
		expect(moveActivitySelection(["a", "b", "c"], "c", -1)).toBe("b");
		expect(moveActivitySelection(["a", "b", "c"], "a", -1)).toBe("a");
	});

	it("falls back to an end of the list when the current id is no longer present", () => {
		expect(moveActivitySelection(["a", "b", "c"], "gone", 1)).toBe("a");
		expect(moveActivitySelection(["a", "b", "c"], "gone", -1)).toBe("c");
	});
});
