import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../invoke", () => ({
	invoke: vi.fn(() => Promise.resolve(null)),
}));

import { invoke } from "../../invoke";
import { paneLayoutStore } from "../../stores/paneLayout";
import { terminalsStore } from "../../stores/terminals";
import { assignTabToActiveGroup } from "../../utils/paneTabAssign";

describe("assignTabToActiveGroup with an agent-owned split", () => {
	beforeEach(() => paneLayoutStore.reset());
	afterEach(() => {
		paneLayoutStore.reset();
		paneLayoutStore._testCancelPendingSave();
	});

	const swarmSplit = () => paneLayoutStore.arrangeSessionsAsLayout(["a", "b"], "tiled");

	it("flags a tmux-shim arrangement as agent-owned", () => {
		swarmSplit();
		expect(paneLayoutStore.isSplit()).toBe(true);
		expect(paneLayoutStore.isAgentOwned()).toBe(true);
	});

	it("keeps a user-created terminal out of the agent's split", () => {
		swarmSplit();
		assignTabToActiveGroup("user-term", "terminal");
		expect(paneLayoutStore.getGroupForTab("user-term")).toBeFalsy();
	});

	it("still docks an agent-spawned terminal when the caller allows it", () => {
		swarmSplit();
		assignTabToActiveGroup("agent-term", "terminal", false, { agent: true });
		expect(paneLayoutStore.getGroupForTab("agent-term")).toBeTruthy();
	});

	it("docks into a split the user started", () => {
		const g = paneLayoutStore.createGroup();
		paneLayoutStore.addTab(g, { id: "a", type: "terminal" });
		paneLayoutStore.setRoot({ type: "leaf", id: g });
		paneLayoutStore.setActiveGroup(g);
		paneLayoutStore.split(g, "vertical");
		assignTabToActiveGroup("user-term", "terminal");
		expect(paneLayoutStore.getGroupForTab("user-term")).toBeTruthy();
	});

	it("a user split adopts an agent-owned layout", () => {
		swarmSplit();
		paneLayoutStore.split(paneLayoutStore.state.activeGroupId as string, "horizontal");
		expect(paneLayoutStore.isAgentOwned()).toBe(false);
		assignTabToActiveGroup("user-term", "terminal");
		expect(paneLayoutStore.getGroupForTab("user-term")).toBeTruthy();
	});

	it("persists the flag to disk and reads it back", async () => {
		swarmSplit();
		expect(paneLayoutStore.serialize().agentOwned).toBe(true);
		const saved = paneLayoutStore.serialize();
		paneLayoutStore.reset();
		vi.mocked(invoke).mockResolvedValueOnce(saved);

		await paneLayoutStore.loadFromDisk();

		expect(paneLayoutStore.isAgentOwned()).toBe(true);
	});

	it("omits the flag from a user layout's snapshot", () => {
		const g = paneLayoutStore.createGroup();
		paneLayoutStore.addTab(g, { id: "a", type: "terminal" });
		paneLayoutStore.setRoot({ type: "leaf", id: g });
		paneLayoutStore.split(g, "vertical");
		expect(paneLayoutStore.serialize()).not.toHaveProperty("agentOwned");
	});

	it("survives serialize/restore and clears on reset", () => {
		swarmSplit();
		const saved = paneLayoutStore.serialize();
		paneLayoutStore.reset();
		expect(paneLayoutStore.isAgentOwned()).toBe(false);
		paneLayoutStore.restore(saved);
		expect(paneLayoutStore.isAgentOwned()).toBe(true);
	});

	describe("a displayed orphan terminal over a user split", () => {
		const userSplit = () => {
			const g = paneLayoutStore.createGroup();
			paneLayoutStore.addTab(g, { id: "a", type: "terminal" });
			paneLayoutStore.setRoot({ type: "leaf", id: g });
			paneLayoutStore.setActiveGroup(g);
			paneLayoutStore.split(g, "vertical");
		};

		it("keeps the new tab out of the split when the viewed terminal is an orphan", () => {
			userSplit();
			assignTabToActiveGroup("new-term", "terminal", true, { displayedId: "orphan" });
			expect(paneLayoutStore.getGroupForTab("new-term")).toBeFalsy();
		});

		it("docks when the viewed terminal is inside a pane", () => {
			userSplit();
			assignTabToActiveGroup("new-term", "terminal", true, { displayedId: "a" });
			expect(paneLayoutStore.getGroupForTab("new-term")).toBeTruthy();
		});

		it("defaults the viewed terminal to the active one", () => {
			userSplit();
			const orphan = terminalsStore.add({ sessionId: null, fontSize: 14, name: "o", cwd: null, awaitingInput: null });
			terminalsStore.setActive(orphan);
			assignTabToActiveGroup("new-term", "terminal");
			expect(paneLayoutStore.getGroupForTab("new-term")).toBeFalsy();
			terminalsStore.remove(orphan);
		});
	});
});
