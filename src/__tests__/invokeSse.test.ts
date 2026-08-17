import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

// Browser mode — this is the path that serves the web UI / PWA / remote client.
vi.mock("../transport", () => ({
	isTauri: () => false,
	rpc: vi.fn(),
}));

/** Records every stream the module opens, and whether it was closed. */
class MockEventSource {
	static readonly instances: MockEventSource[] = [];
	static readonly CONNECTING = 0;
	static readonly OPEN = 1;
	static readonly CLOSED = 2;
	readyState = MockEventSource.OPEN;
	url: string;
	closed = false;
	onerror: ((ev: unknown) => void) | null = null;
	readonly attached: string[] = [];

	constructor(url: string | URL) {
		this.url = String(url);
		MockEventSource.instances.push(this);
	}

	addEventListener(type: string): void {
		this.attached.push(type);
	}
	removeEventListener(): void {}
	close(): void {
		this.closed = true;
		this.readyState = MockEventSource.CLOSED;
	}
}

/** The most recently opened stream. */
function last(): MockEventSource | undefined {
	return MockEventSource.instances[MockEventSource.instances.length - 1];
}

/** Opens are coalesced onto a macrotask, so drain one. */
function flush(): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, 0));
}

/** The `types` the given stream URL subscribes to. */
function typesOf(url: string): string[] {
	const query = url.split("?")[1] ?? "";
	const raw = new URLSearchParams(query).get("types");
	expect(raw, `stream URL carries no type filter: ${url}`).toBeTruthy();
	return raw!.split(",").sort();
}

/** A fresh module instance — the SSE connection is module-level state. */
async function freshInvoke() {
	vi.resetModules();
	return await import("../invoke");
}

describe("browser-mode shared SSE subscription", () => {
	beforeEach(() => {
		MockEventSource.instances.length = 0;
		vi.stubGlobal("EventSource", MockEventSource);
	});

	it("subscribes only to the event types that were registered", async () => {
		const { listen } = await freshInvoke();
		await listen("repo-changed", () => {});
		await flush();

		const stream = last();
		expect(stream, "no stream was opened").toBeTruthy();
		expect(typesOf(stream!.url)).toEqual(["repo-changed"]);
	});

	it("reopens with a widened filter when a new type is registered after connect", async () => {
		const { listen } = await freshInvoke();
		await listen("repo-changed", () => {});
		await flush();
		const first = last()!;
		expect(typesOf(first.url)).toEqual(["repo-changed"]);

		// The server fixes the filter at connect time, so a type registered later
		// can only be delivered by reconnecting.
		await listen("session-created", () => {});
		await flush();

		const second = last()!;
		expect(second, "a new type must not be silently undeliverable").not.toBe(first);
		expect(first.closed, "the superseded stream must be closed, not leaked").toBe(true);
		expect(typesOf(second.url)).toEqual(["repo-changed", "session-created"]);
	});

	it("opens a single stream for a burst of registrations", async () => {
		const { listen } = await freshInvoke();
		// Startup registers many types back to back. One connection, not one each.
		await listen("repo-changed", () => {});
		await listen("session-created", () => {});
		await listen("pty-exit", () => {});
		await flush();

		expect(MockEventSource.instances).toHaveLength(1);
		expect(typesOf(MockEventSource.instances[0].url)).toEqual(["pty-exit", "repo-changed", "session-created"]);
	});

	it("does not reopen for a second handler on an already-covered type", async () => {
		const { listen } = await freshInvoke();
		await listen("repo-changed", () => {});
		await flush();
		await listen("repo-changed", () => {});
		await flush();

		expect(MockEventSource.instances).toHaveLength(1);
	});

	it("still delivers to every handler of a type after a widening reopen", async () => {
		const { listen } = await freshInvoke();
		const seen: string[] = [];
		await listen("repo-changed", () => seen.push("first"));
		await flush();
		await listen("session-created", () => {});
		await flush();

		// The reopened stream must have re-attached the earlier type.
		const second = last()!;
		expect(second.attached).toContain("repo-changed");
		expect(second.attached).toContain("session-created");
		expect(seen).toEqual([]);
	});
});
