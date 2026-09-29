import { beforeEach, describe, expect, it, vi } from "vitest";
import { createConfigDeltaWriter } from "../../utils/configDeltaWriter";

const mockInvoke = vi.hoisted(() => vi.fn());
vi.mock("../../invoke", () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));

describe("configDeltaWriter", () => {
	beforeEach(() => mockInvoke.mockReset().mockResolvedValue(undefined));

	it("rejects a save before the configuration has loaded", async () => {
		const writer = createConfigDeltaWriter<{ enabled: boolean }>("save_ui_prefs");

		await expect(writer.save({ enabled: true })).rejects.toThrow("refused before config load");
		expect(mockInvoke).not.toHaveBeenCalled();
	});

	it("sends the loaded snapshot as base and advances it after a successful save", async () => {
		const writer = createConfigDeltaWriter<{ left: number; right: number }>("save_ui_prefs");
		const loaded = { left: 1, right: 2 };
		writer.loaded(loaded);
		loaded.left = 99;

		await writer.save({ left: 3, right: 2 });
		await writer.save({ left: 3, right: 4 });

		expect(mockInvoke.mock.calls).toEqual([
			["save_ui_prefs", { base: { left: 1, right: 2 }, config: { left: 3, right: 2 } }],
			["save_ui_prefs", { base: { left: 3, right: 2 }, config: { left: 3, right: 4 } }],
		]);
	});

	it("does not send a later save while the earlier write is pending", async () => {
		let finishFirst!: () => void;
		mockInvoke.mockImplementationOnce(() => new Promise<void>((resolve) => { finishFirst = resolve; }));
		const writer = createConfigDeltaWriter<{ count: number }>("save_ui_prefs");
		writer.loaded({ count: 0 });
		const first = writer.save({ count: 1 });
		const second = writer.save({ count: 2 });

		expect(mockInvoke).toHaveBeenCalledTimes(1);
		finishFirst();
		await Promise.all([first, second]);
		expect(mockInvoke.mock.calls[1][1]).toEqual({ base: { count: 1 }, config: { count: 2 } });
	});
});
