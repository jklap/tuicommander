import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
	syncDisabledList: vi.fn().mockResolvedValue(undefined),
	loadUserPlugins: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../plugins/pluginLoader", () => ({
	isPluginDisabled: vi.fn(() => false),
	loadUserPlugins: mocks.loadUserPlugins,
	syncDisabledList: mocks.syncDisabledList,
}));
vi.mock("../../features/agentUsage", () => ({ initAgentUsage: vi.fn(), destroyAgentUsage: vi.fn() }));

import { initPlugins } from "../../plugins";

describe("initPlugins", () => {
	beforeEach(() => vi.clearAllMocks());

	it("syncs disabled plugins once and reuses the result for user plugins", async () => {
		await initPlugins();

		expect(mocks.syncDisabledList).toHaveBeenCalledOnce();
		expect(mocks.loadUserPlugins).toHaveBeenCalledWith(false);
	});

	it("loads external plugins after synchronizing the disabled list", async () => {
		await initPlugins();

		expect(mocks.syncDisabledList).toHaveBeenCalledBefore(mocks.loadUserPlugins);
	});
});
