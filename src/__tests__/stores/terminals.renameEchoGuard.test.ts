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
});
