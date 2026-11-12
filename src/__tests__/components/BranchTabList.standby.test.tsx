import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { BranchTabList } from "../../components/Sidebar/RepoSection";
import { terminalsStore } from "../../stores/terminals";

// A parked (SIGSTOP'd) tab has no running activity to age. The sidebar used to keep counting a
// "2m" beside it while the Activity Dashboard said 20h, so the pause mark now takes the age's slot.
describe("BranchTabList standby mark", () => {
	const clear = () => {
		for (const id of terminalsStore.getIds()) terminalsStore.remove(id);
	};
	beforeEach(clear);
	afterEach(clear);

	function addTerminal(standby: boolean) {
		const id = terminalsStore.add({
			name: "shell 2",
			sessionId: null,
			fontSize: 14,
			cwd: null,
			awaitingInput: null,
		});
		terminalsStore.update(id, { lastActivityAt: Date.now() - 2 * 60_000, standby });
		return id;
	}

	it("shows the age, and no pause mark, for a running tab", () => {
		const id = addTerminal(false);
		const { container } = render(() => <BranchTabList terminalIds={[id]} repoPath="/repo" />);
		expect(container.querySelector(".branchAgentTime")?.textContent).toBe("2m");
		expect(container.querySelector(".branchStandbyMark")).toBeNull();
	});

	it("shows the pause mark instead of the age for a parked tab", () => {
		const id = addTerminal(true);
		const { container } = render(() => <BranchTabList terminalIds={[id]} repoPath="/repo" />);
		const mark = container.querySelector(".branchStandbyMark");
		expect(mark).not.toBeNull();
		expect(mark?.getAttribute("aria-label")).toBe("Standby (paused)");
		expect(mark?.querySelectorAll("rect")).toHaveLength(2);
		expect(container.textContent).not.toContain("2m");
		// Exactly one element holds the slot: the mark replaces the age, it does not join it.
		expect(container.querySelectorAll(".branchAgentTime")).toHaveLength(1);
	});

	it("swaps live when the tab is parked and again when it wakes", () => {
		const id = addTerminal(false);
		const { container } = render(() => <BranchTabList terminalIds={[id]} repoPath="/repo" />);
		expect(container.querySelector(".branchStandbyMark")).toBeNull();

		terminalsStore.update(id, { standby: true });
		expect(container.querySelector(".branchStandbyMark")).not.toBeNull();
		expect(container.textContent).not.toContain("2m");

		terminalsStore.update(id, { standby: false });
		expect(container.querySelector(".branchStandbyMark")).toBeNull();
		expect(container.querySelector(".branchAgentTime")?.textContent).toBe("2m");
	});
});
