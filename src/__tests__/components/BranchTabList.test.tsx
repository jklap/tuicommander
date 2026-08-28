import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { BranchTabList } from "../../components/Sidebar/RepoSection";
import { terminalsStore } from "../../stores/terminals";

/** Read the dot's classList off the rendered list. */
function dotClasses(container: HTMLElement, index = 0): DOMTokenList {
	const dots = container.querySelectorAll(".branchTabDot");
	return dots[index]!.classList;
}

describe("BranchTabList dot priority", () => {
	beforeEach(() => {
		for (const id of terminalsStore.getIds()) {
			terminalsStore.remove(id);
		}
	});

	afterEach(() => {
		for (const id of terminalsStore.getIds()) {
			terminalsStore.remove(id);
		}
	});

	function addTerminal(overrides: Partial<{ name: string; awaitingInput: "question" | "error" | null }> = {}) {
		return terminalsStore.add({
			name: overrides.name ?? "Terminal",
			sessionId: null,
			fontSize: 14,
			cwd: null,
			awaitingInput: overrides.awaitingInput ?? null,
		});
	}

	it("renders a plain dot with no special state", () => {
		const id = addTerminal();
		const { container } = render(() => <BranchTabList terminalIds={[id]} />);
		const classes = dotClasses(container);
		expect(classes.contains("branchTabDotBusy")).toBe(false);
		expect(classes.contains("branchTabDotIdle")).toBe(false);
		expect(classes.contains("branchTabDotUnseen")).toBe(false);
		expect(classes.contains("branchTabDotError")).toBe(false);
		expect(classes.contains("branchTabDotQuestion")).toBe(false);
	});

	it("shows busy when the shell is busy", () => {
		const id = addTerminal();
		terminalsStore.update(id, { shellState: "busy" });
		const { container } = render(() => <BranchTabList terminalIds={[id]} />);
		expect(dotClasses(container).contains("branchTabDotBusy")).toBe(true);
	});

	it("shows idle when the shell is idle and not unseen", () => {
		const id = addTerminal();
		terminalsStore.update(id, { shellState: "idle" });
		const { container } = render(() => <BranchTabList terminalIds={[id]} />);
		expect(dotClasses(container).contains("branchTabDotIdle")).toBe(true);
	});

	it("shows unseen even though the shell is idle", () => {
		const id = addTerminal();
		terminalsStore.update(id, { shellState: "idle", unseen: true });
		const { container } = render(() => <BranchTabList terminalIds={[id]} />);
		const classes = dotClasses(container);
		expect(classes.contains("branchTabDotUnseen")).toBe(true);
		expect(classes.contains("branchTabDotIdle")).toBe(false);
	});

	it("shows busy over unseen", () => {
		const id = addTerminal();
		terminalsStore.update(id, { shellState: "busy", unseen: true });
		const { container } = render(() => <BranchTabList terminalIds={[id]} />);
		const classes = dotClasses(container);
		expect(classes.contains("branchTabDotBusy")).toBe(true);
		expect(classes.contains("branchTabDotUnseen")).toBe(false);
	});

	it("shows question over busy", () => {
		const id = addTerminal({ awaitingInput: "question" });
		terminalsStore.update(id, { shellState: "busy" });
		const { container } = render(() => <BranchTabList terminalIds={[id]} />);
		const classes = dotClasses(container);
		expect(classes.contains("branchTabDotQuestion")).toBe(true);
		expect(classes.contains("branchTabDotBusy")).toBe(false);
	});

	it("shows error over busy (awaitingInput and shellState are independent fields)", () => {
		const id = addTerminal({ awaitingInput: "error" });
		terminalsStore.update(id, { shellState: "busy" });
		const { container } = render(() => <BranchTabList terminalIds={[id]} />);
		const classes = dotClasses(container);
		expect(classes.contains("branchTabDotError")).toBe(true);
		expect(classes.contains("branchTabDotBusy")).toBe(false);
	});
});
