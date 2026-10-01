import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeTerminal, testInScope } from "../helpers/store";

// Regression coverage for the tab-name-flapping bug: `update()` echoes a
// `name`/`nameIsCustom` change back to the backend via `set_session_name` (so
// a reconnect can distinguish a user-protected rename from a transient OSC
// one). The backend's `session-renamed` event — fired for tmux
// `select-pane -T` calls and for OSC title repaints — feeds straight back
// into this same `update()`, so an unguarded echo bounces forever. See
// src-tauri/src/mcp_http/session.rs's `set_session_name_skips_emit_when_unchanged`
// and src-tauri/src/mcp_http/tmux_routes.rs's
// `rename_pane_is_idempotent_and_only_emits_on_real_change` for the backend
// half of this same invariant.
vi.mock("../../transport", () => ({ rpc: vi.fn(async () => undefined) }));

describe("terminalsStore update() — set_session_name echo guard", () => {
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

	it("echoes a real name change back to set_session_name", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			store.update(id, { name: "hello" });
			expect(rpc).toHaveBeenCalledTimes(1);
			expect(rpc).toHaveBeenCalledWith("set_session_name", {
				sessionId: "sess-1",
				name: "hello",
				isCustom: false,
			});
		});
	});

	it("does not re-echo an unchanged name/isCustom pair", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			store.update(id, { name: "hello" });
			rpc.mockClear();

			// This is exactly the backend's own session-renamed echo of the
			// rename this store just applied. Without the dedupe guard, this
			// re-triggers the rpc call, which re-triggers the backend's emit,
			// forever.
			store.update(id, { name: "hello", nameIsCustom: false });
			expect(rpc).not.toHaveBeenCalled();
		});
	});

	it("still echoes when nameIsCustom flips even though the name text is unchanged", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			store.update(id, { name: "hello", nameIsCustom: false });
			rpc.mockClear();

			store.update(id, { name: "hello", nameIsCustom: true });
			expect(rpc).toHaveBeenCalledTimes(1);
			expect(rpc).toHaveBeenCalledWith("set_session_name", {
				sessionId: "sess-1",
				name: "hello",
				isCustom: true,
			});
		});
	});

	it("echoes again once the name changes back after an unchanged no-op call", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			store.update(id, { name: "hello" });
			rpc.mockClear();

			store.update(id, { name: "hello" }); // no-op, must not fire
			store.update(id, { name: "world" }); // real change, must fire
			expect(rpc).toHaveBeenCalledTimes(1);
			expect(rpc).toHaveBeenCalledWith("set_session_name", {
				sessionId: "sess-1",
				name: "world",
				isCustom: false,
			});
		});
	});

	it("never calls set_session_name when the terminal has no backend session yet", () => {
		testInScope(() => {
			const id = store.add(makeTerminal());
			store.update(id, { name: "hello" });
			expect(rpc).not.toHaveBeenCalled();
		});
	});

	it("{ echo: false } skips the set_session_name echo even on a real name change", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			store.update(id, { name: "hello" }, { echo: false });
			expect(rpc).not.toHaveBeenCalled();
			expect(store.get(id)?.name).toBe("hello");
		});
	});
});

describe("terminalsStore update() — set_session_accent_color echo guard", () => {
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

	it("echoes a real accent-color change back to set_session_accent_color", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			store.update(id, { accentColor: "blue" });
			expect(rpc).toHaveBeenCalledTimes(1);
			expect(rpc).toHaveBeenCalledWith("set_session_accent_color", {
				sessionId: "sess-1",
				color: "blue",
			});
		});
	});

	it("does not re-echo an unchanged accent color", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			store.update(id, { accentColor: "blue" });
			rpc.mockClear();

			// The backend's own echo of the color this store just applied —
			// same ping-pong shape as the name guard above.
			store.update(id, { accentColor: "blue" });
			expect(rpc).not.toHaveBeenCalled();
		});
	});

	it("echoes again once the color changes back after an unchanged no-op call", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			store.update(id, { accentColor: "blue" });
			rpc.mockClear();

			store.update(id, { accentColor: "blue" }); // no-op, must not fire
			store.update(id, { accentColor: "red" }); // real change, must fire
			expect(rpc).toHaveBeenCalledTimes(1);
			expect(rpc).toHaveBeenCalledWith("set_session_accent_color", {
				sessionId: "sess-1",
				color: "red",
			});
		});
	});

	it("never calls set_session_accent_color when the terminal has no backend session yet", () => {
		testInScope(() => {
			const id = store.add(makeTerminal());
			store.update(id, { accentColor: "blue" });
			expect(rpc).not.toHaveBeenCalled();
		});
	});

	it("{ echo: false } skips the set_session_accent_color echo even on a real change", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			store.update(id, { accentColor: "blue" }, { echo: false });
			expect(rpc).not.toHaveBeenCalled();
			expect(store.get(id)?.accentColor).toBe("blue");
		});
	});
});

// Regression coverage for the fix itself: `applyBackendRename`/
// `applyBackendAccentColor` apply a backend-originated event without ever
// echoing back — see terminals.ts's doc comments on both and the ping-pong
// this closes (two backend writes in quick succession, e.g. an OSC title
// followed by a restore to the base name, used to bounce forever between
// this echo and the backend's own re-emit).
describe("terminalsStore.applyBackendRename / applyBackendAccentColor", () => {
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

	it("applyBackendRename never calls rpc and applies the name/isCustom", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			store.applyBackendRename("sess-1", "claude · resume", false);
			expect(rpc).not.toHaveBeenCalled();
			expect(store.get(id)?.name).toBe("claude · resume");
			expect(store.get(id)?.nameIsCustom).toBe(false);
		});
	});

	it("applyBackendRename is a no-op for a session with no bound terminal", () => {
		testInScope(() => {
			store.applyBackendRename("no-such-session", "hello", false);
			expect(rpc).not.toHaveBeenCalled();
		});
	});

	it("applyBackendAccentColor never calls rpc and applies the color, including clearing to null", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			store.applyBackendAccentColor("sess-1", "blue");
			expect(rpc).not.toHaveBeenCalled();
			expect(store.get(id)?.accentColor).toBe("blue");

			store.applyBackendAccentColor("sess-1", null);
			expect(rpc).not.toHaveBeenCalled();
			expect(store.get(id)?.accentColor).toBeNull();
		});
	});

	it("applyBackendAccentColor is a no-op for a session with no bound terminal", () => {
		testInScope(() => {
			store.applyBackendAccentColor("no-such-session", "blue");
			expect(rpc).not.toHaveBeenCalled();
		});
	});

	// The actual bug: two backend writes racing each other (A then B) each
	// look like a genuine change from the receiving side's point of view, so
	// a naive echo bounces forever. Simulate the backend by having the rpc
	// mock immediately "deliver" a session-renamed event for whatever it was
	// just told, through whichever apply path the test is checking.
	it("characterizes the pre-fix loop: plain update() with echo bounces forever between two backend values", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			let hops = 0;
			const MAX_HOPS = 25; // deep enough to prove the loop is unbounded, no deeper than needed
			rpc.mockImplementation(async (_cmd: string, args: { name?: string | null }) => {
				hops++;
				if (hops > MAX_HOPS) return undefined;
				// The backend "disagrees" and sends back the other value — the
				// A-then-B/B-then-A race this bug's root cause describes.
				const next = args.name === "claude · resume" ? "to-test" : "claude · resume";
				store.update(id, { name: next, nameIsCustom: false });
				return undefined;
			});

			store.update(id, { name: "claude · resume" });
			expect(hops).toBeGreaterThan(MAX_HOPS - 1);
		});
	});

	it("the fix: applyBackendRename never triggers rpc, so the same two-value race settles in one step", () => {
		testInScope(() => {
			const id = store.add(makeTerminal({ sessionId: "sess-1" }));
			rpc.mockImplementation(async () => {
				throw new Error("applyBackendRename must never call rpc");
			});

			store.applyBackendRename("sess-1", "claude · resume", false);
			store.applyBackendRename("sess-1", "to-test", false);

			expect(rpc).not.toHaveBeenCalled();
			expect(store.get(id)?.name).toBe("to-test");
		});
	});
});
