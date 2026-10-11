import { describe, expect, it, vi } from "vitest";
// @ts-expect-error plain MV3 module
import { HEARTBEAT_MS, heartbeatReply } from "../../../browser-extension/design-mode/heartbeat.js";

function browser() {
	let saved = {};
	return {
		storage: {
			get: async () => saved,
			set: async (value: object) => {
				saved = { ...saved, ...value };
			},
		},
		tabs: { reload: vi.fn(async () => {}), query: vi.fn(async () => [{ id: 7 }]) },
	};
}

describe("Design Mode heartbeat reload delivery", () => {
	// Catches: a pending reload lost on the first reply or repeated after a worker restart.
	it("reload_request_reaches_the_extension_once", async () => {
		const { storage, tabs } = browser();
		await heartbeatReply({ armed: true, reloadSeq: 1 }, storage, tabs, 9);
		await heartbeatReply({ armed: true, reloadSeq: 1 }, storage, tabs, 9);
		expect(tabs.reload.mock.calls).toEqual([[9]]);
		await heartbeatReply({ armed: true, reloadSeq: 2 }, storage, tabs, 9);
		expect(tabs.reload.mock.calls).toEqual([[9], [9]]);
	});
	// Catches: a failed Chrome reload acknowledged before the page reloads.
	it("retries a failed reload on the next beat", async () => {
		const { storage, tabs } = browser();
		tabs.reload.mockRejectedValueOnce(new Error("Tab unavailable"));
		await expect(heartbeatReply({ armed: true, reloadSeq: 1 }, storage, tabs, 9)).rejects.toThrow("Tab unavailable");
		await heartbeatReply({ armed: true, reloadSeq: 1 }, storage, tabs, 9);
		expect(tabs.reload).toHaveBeenCalledTimes(2);
	});
	// Catches: an unarmed or malformed reply reloading a user's unrelated tab.
	it("ignores unarmed and malformed replies and resolves the review tab when unset", async () => {
		const { storage, tabs } = browser();
		for (const reply of [
			{ armed: false, reloadSeq: 1 },
			{ armed: true, reloadSeq: undefined },
			{ armed: true, reloadSeq: 0 },
		]) {
			await heartbeatReply(reply, storage, tabs);
		}
		expect(tabs.reload).not.toHaveBeenCalled();
		await heartbeatReply({ armed: true, reloadSeq: 1 }, storage, tabs);
		expect(tabs.reload).toHaveBeenCalledWith(7);
		expect(HEARTBEAT_MS).toBe(5000);
	});
});
