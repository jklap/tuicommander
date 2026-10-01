import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/plugin-notification", () => ({
	isPermissionGranted: vi.fn().mockResolvedValue(true),
	requestPermission: vi.fn().mockResolvedValue("granted"),
	sendNotification: vi.fn(),
}));
vi.mock("../../invoke", () => ({ invoke: invokeMock }));
vi.mock("../../stores/toasts", () => ({ toastsStore: { add: vi.fn() } }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { error: vi.fn() } }));
vi.mock("../../stores/repositories", () => ({
	repositoriesStore: { state: { activeRepoPath: "/repo" }, getPaths: () => ["/repo"], get: () => undefined },
}));
vi.mock("../../stores/terminals", () => ({ terminalsStore: { state: { activeId: null, terminals: {} } } }));

const flow = (tag: string) => ({ project: "/repo", tag, columns: [], steps: [] });
const flowCalls = () => invokeMock.mock.calls.filter(([cmd]) => cmd === "progress_flow").length;

async function makeStore() {
	const { createProgressStore } = await import("../../stores/progress");
	return createProgressStore();
}

describe("sidebar flow lifecycle (critic r7)", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		vi.resetModules();
		vi.useFakeTimers();
	});

	it("a release function called twice does not cancel the trailing read of the other mounted row", async () => {
		// catches: release that decrements on every call, so a double release steals another row's hold
		invokeMock.mockResolvedValue(flow("x"));
		const store = await makeStore();
		const releaseA = store.holdSidebarFlow("/repo");
		store.holdSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo"); // schedules trailing
		releaseA();
		releaseA();
		await vi.advanceTimersByTimeAsync(6000);
		expect(flowCalls()).toBe(2);
		vi.useRealTimers();
	});

	it("remounting a row after the last release schedules and runs a fresh trailing read", async () => {
		// catches: a cancelled trailing entry left in the map so the remounted row's ask is swallowed
		invokeMock.mockResolvedValue(flow("x"));
		const store = await makeStore();
		const release = store.holdSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo");
		release();
		store.holdSidebarFlow("/repo");
		await store.refreshSidebarFlow("/repo"); // still inside the gap
		await vi.advanceTimersByTimeAsync(6000);
		expect(flowCalls()).toBe(2);
		vi.useRealTimers();
	});

	it("a late older response is dropped once a newer one was applied", async () => {
		// catches: last-arrival-wins, so a slow stale read overwrites the fresh flow
		let resolveOld: (value: unknown) => void = () => {};
		invokeMock.mockImplementationOnce(() => new Promise((resolve) => (resolveOld = resolve)));
		invokeMock.mockResolvedValueOnce(flow("new"));
		const store = await makeStore();
		const old = store.refreshSidebarFlow("/repo");
		await vi.advanceTimersByTimeAsync(5001);
		await store.refreshSidebarFlow("/repo");
		resolveOld(flow("old"));
		await old;
		expect((store.sidebarFlow("/repo") as unknown as { tag: string }).tag).toBe("new");
		vi.useRealTimers();
	});

	it("an older response is applied when the newer read failed", async () => {
		// catches: the guard comparing with the latest requested seq instead of the latest applied one
		let resolveOld: (value: unknown) => void = () => {};
		invokeMock.mockImplementationOnce(() => new Promise((resolve) => (resolveOld = resolve)));
		invokeMock.mockRejectedValueOnce(new Error("boom"));
		const store = await makeStore();
		const old = store.refreshSidebarFlow("/repo");
		await vi.advanceTimersByTimeAsync(5001);
		await store.refreshSidebarFlow("/repo");
		resolveOld(flow("old"));
		await old;
		expect((store.sidebarFlow("/repo") as unknown as { tag: string }).tag).toBe("old");
		vi.useRealTimers();
	});

	it("a response in flight across resetForTests is dropped", async () => {
		// catches: epoch not bumped on reset, so a pre-reset read repopulates the store
		let resolveOld: (value: unknown) => void = () => {};
		invokeMock.mockImplementationOnce(() => new Promise((resolve) => (resolveOld = resolve)));
		const store = await makeStore();
		const pending = store.refreshSidebarFlow("/repo");
		store.resetForTests();
		resolveOld(flow("old"));
		await pending;
		expect(store.sidebarFlow("/repo")).toBeUndefined();
		vi.useRealTimers();
	});
});
