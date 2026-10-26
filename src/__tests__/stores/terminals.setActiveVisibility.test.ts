import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal, testInScope } from "../helpers/store";

// Regression coverage for parked tabs never waking on click: `setActive()` used to send the
// terminal-store id ("term-6") as `sessionId`, but the backend keys visibility and standby by PTY
// session id, so `wake_session` saw an unknown session and silently did nothing. The window-focus
// path (`useAppInit.ts`) already resolved the real id, which is why only click-to-wake was broken.
vi.mock("../../transport", () => ({ rpc: vi.fn(async () => undefined) }));

describe("terminalsStore setActive() — set_session_visible", () => {
	let store: typeof import("../../stores/terminals").terminalsStore;
	// biome-ignore lint/suspicious/noExplicitAny: vi.fn() mock reference, re-imported fresh each test after resetModules
	let rpc: any;

	beforeEach(async () => {
		vi.resetModules();
		vi.clearAllMocks();
		localStorage.clear();
		store = (await import("../../stores/terminals")).terminalsStore;
		rpc = (await import("../../transport")).rpc;
	});

	afterEach(() => {
		store._testCancelPendingTimers();
	});

	const visibleCalls = () => rpc.mock.calls.filter(([name]: [string]) => name === "set_session_visible");

	it("reports the PTY session id, not the terminal id, when a tab becomes active", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "pty-uuid-1" }));
			rpc.mockClear();
			store.setActive(id);
			const calls = visibleCalls();
			expect(calls).toHaveLength(1);
			expect(calls[0][1]).toMatchObject({ sessionId: "pty-uuid-1", visible: true });
			expect(calls[0][1].sessionId).not.toBe(id);
		});
	});

	it("reports the previous tab's PTY session id as hidden when switching away", () => {
		testInScope(() => {
			const first = store.add(makeTerminal({ sessionId: "pty-uuid-1" }));
			const second = store.add(makeTerminal({ sessionId: "pty-uuid-2" }));
			store.setActive(first);
			rpc.mockClear();
			store.setActive(second);
			const calls = visibleCalls().map(([, args]: [string, Record<string, unknown>]) => args);
			expect(calls).toContainEqual(expect.objectContaining({ sessionId: "pty-uuid-1", visible: false }));
			expect(calls).toContainEqual(expect.objectContaining({ sessionId: "pty-uuid-2", visible: true }));
		});
	});

	it("sends nothing for a tab with no PTY yet (a restore placeholder)", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: null }));
			rpc.mockClear();
			store.setActive(id);
			expect(visibleCalls()).toHaveLength(0);
		});
	});
});
