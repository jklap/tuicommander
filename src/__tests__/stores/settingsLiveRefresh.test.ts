import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import recordedAppConfig from "../fixtures/settings-concurrency/app-defaults.json";

const io = vi.hoisted(() => ({ invoke: vi.fn(), subscribe: vi.fn() }));
vi.mock("../../invoke", async (original) => ({
	...(await original<typeof import("../../invoke")>()),
	invoke: io.invoke,
}));
vi.mock("../../transport", async (original) => ({
	...(await original<typeof import("../../transport")>()),
	subscribeEvents: io.subscribe,
}));

const flush = async () => {
	for (let i = 0; i < 12; i++) await Promise.resolve();
};

describe("settings live refresh", () => {
	let store: typeof import("../../stores/settings").settingsStore;
	let document: Record<string, unknown>;
	let handlers: Record<string, (payload: unknown) => void>;
	let resync: () => void;
	let stop: () => void;
	const event = (generation: number) =>
		handlers["settings-changed"]({ section: "app", generation, persisted: true, applied: true });

	beforeEach(async () => {
		vi.useFakeTimers();
		vi.resetModules();
		localStorage.clear();
		document = structuredClone(recordedAppConfig);
		io.invoke
			.mockReset()
			.mockImplementation(async (command: string) =>
				command === "load_config" ? structuredClone(document) : undefined,
			);
		io.subscribe.mockReset().mockImplementation(async (callbacks, options) => {
			handlers = callbacks;
			resync = () => options.onResync("reconnect");
			return vi.fn();
		});
		store = (await import("../../stores/settings")).settingsStore;
		await store.hydrate();
		stop = store.startRemoteSync();
		await flush();
	});
	afterEach(() => {
		stop();
		vi.useRealTimers();
	});

	// Catches: an external write staying invisible or being echoed through save_config.
	it("mcp_write_refreshes_ui_without_save", async () => {
		document.theme = "light";
		event(1);
		await flush();
		expect(store.state.theme).toBe("light");
		await vi.advanceTimersByTimeAsync(1000);
		expect(io.invoke.mock.calls.filter(([command]) => command === "save_config")).toEqual([]);
	});

	// Catches: incoming state clobbering a draft or a stale debounce overwriting the external edit.
	it("dirty_edit_survives_patch", async () => {
		store.setShell("/draft-shell");
		document.shell = "/external-shell";
		document.theme = "light";
		event(1);
		await flush();
		expect(store.state.shell).toBe("/draft-shell");
		expect(store.state.theme).toBe("light");
		expect(store.conflicts()).toContain("shell");
		await vi.advanceTimersByTimeAsync(1000);
		expect(io.invoke.mock.calls.filter(([command]) => command === "save_config")).toEqual([]);
	});

	// Catches: a missed event or a restarted server's reset counter leaving stale settings.
	it("sse_reconnect_refetches", async () => {
		document.theme = "before-restart";
		event(100);
		await flush();
		document.theme = "after-restart";
		resync();
		await flush();
		expect(store.state.theme).toBe("after-restart");
		document.theme = "new-process-write";
		event(1);
		await flush();
		expect(store.state.theme).toBe("new-process-write");
	});
});
