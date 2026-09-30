import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../mocks/tauri";
import { ptyCaptureStore } from "../../utils/ptyCapture";
import { mockInvoke } from "../mocks/tauri";

describe("ptyCaptureStore", () => {
	beforeEach(() => {
		// toggle()'s success/failure path calls toastsStore.add(), which arms a
		// real dismiss-timeout (toasts.ts) and an activity-mirror save timer
		// (activityStore.ts) — fake timers keep both from leaking past the test.
		vi.useFakeTimers();
	});

	afterEach(() => {
		vi.useRealTimers();
		// The tap's status signal is module-level state — reset it so one test's
		// "recording" doesn't leak into the next.
		ptyCaptureStore.applyStatus({ enabled: false });
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
	});

	describe("isRecording", () => {
		it("is false when the tap is disabled, regardless of session_filter", () => {
			ptyCaptureStore.applyStatus({ enabled: false, session_filter: "sess-a" });
			expect(ptyCaptureStore.isRecording("sess-a")).toBe(false);
		});

		it("is true only for the exact filtered session when a filter is set", () => {
			ptyCaptureStore.applyStatus({ enabled: true, session_filter: "sess-a" });
			expect(ptyCaptureStore.isRecording("sess-a")).toBe(true);
			expect(ptyCaptureStore.isRecording("sess-b")).toBe(false);
		});

		// Regression: an unfiltered tap (session_filter: null — the
		// `-d '{"enabled":true}'` curl form, or `{"enabled":true}` with no
		// session_id) records EVERY session, but isRecording used to only ever
		// match an exact session_filter string, so a session genuinely being
		// captured under an unfiltered tap silently read as "not recording."
		it("is true for every session when the tap has no filter", () => {
			ptyCaptureStore.applyStatus({ enabled: true, session_filter: null });
			expect(ptyCaptureStore.isRecording("sess-a")).toBe(true);
			expect(ptyCaptureStore.isRecording("sess-b")).toBe(true);
		});

		it("is true for every session when session_filter is omitted entirely", () => {
			ptyCaptureStore.applyStatus({ enabled: true });
			expect(ptyCaptureStore.isRecording("sess-a")).toBe(true);
		});
	});

	describe("bytes", () => {
		// `applyStatus()` only ever carries the `pty-capture-changed` push
		// event's payload (`enabled`/`session_filter`, no byte counts) — the
		// fuller `{sessions: [...]}` shape only ever arrives via `refresh()`'s
		// `get_pty_capture` round-trip, so that's what these go through.
		it("is 0 for a session with no entry", async () => {
			mockInvoke.mockResolvedValueOnce({ enabled: true, session_filter: "sess-a", sessions: [] });
			await ptyCaptureStore.refresh();
			expect(ptyCaptureStore.bytes("sess-a")).toBe(0);
		});

		it("returns the recorded byte count for a matching session", async () => {
			mockInvoke.mockResolvedValueOnce({
				enabled: true,
				session_filter: "sess-a",
				sessions: [{ session_id: "sess-a", bytes: 4096 }],
			});
			await ptyCaptureStore.refresh();
			expect(ptyCaptureStore.bytes("sess-a")).toBe(4096);
			expect(ptyCaptureStore.bytes("sess-b")).toBe(0);
		});
	});

	describe("applyStatus", () => {
		it("adopts a well-formed status directly, with no IPC call", () => {
			ptyCaptureStore.applyStatus({ enabled: true, session_filter: "sess-a" });
			expect(ptyCaptureStore.isRecording("sess-a")).toBe(true);
			expect(mockInvoke).not.toHaveBeenCalled();
		});

		it("ignores a malformed payload instead of adopting undefined/garbage", () => {
			ptyCaptureStore.applyStatus({ enabled: true, session_filter: "sess-a" });
			// @ts-expect-error — deliberately malformed, mirroring an unmapped
			// command or a stubbed bridge resolving with nothing.
			ptyCaptureStore.applyStatus(null);
			// The previous good status must still be in effect — a malformed
			// answer must never blank out a known-good state.
			expect(ptyCaptureStore.isRecording("sess-a")).toBe(true);
		});

		// Regression: applyStatus() used to fully replace the status object via
		// the same `adopt()` refresh() uses — since the push event's payload
		// only ever carries {enabled, session_filter}, that silently wiped
		// `dir`/`sessions` populated by an earlier refresh(). Reachable in a
		// single window: `toggle()`'s own `invoke("set_pty_capture", ...)`
		// triggers the backend to emit this exact event, and this window's own
		// listener can process it before or after `toggle()`'s own `adopt(next)`
		// — either ordering must leave `dir`/`sessions` intact.
		it("merges enabled/session_filter without wiping dir/sessions from a prior refresh", async () => {
			mockInvoke.mockResolvedValueOnce({
				enabled: true,
				session_filter: "sess-a",
				dir: "/captures",
				sessions: [{ session_id: "sess-a", bytes: 4096 }],
			});
			await ptyCaptureStore.refresh();
			expect(ptyCaptureStore.bytes("sess-a")).toBe(4096);

			// A push event for the SAME toggle (or an unrelated one elsewhere)
			// carries no byte data at all.
			ptyCaptureStore.applyStatus({ enabled: true, session_filter: "sess-a" });

			expect(ptyCaptureStore.isRecording("sess-a")).toBe(true);
			expect(ptyCaptureStore.bytes("sess-a")).toBe(4096);
		});
	});

	describe("refresh", () => {
		it("fetches get_pty_capture and adopts the result", async () => {
			mockInvoke.mockResolvedValueOnce({ enabled: true, session_filter: "sess-a" });
			await ptyCaptureStore.refresh();
			expect(mockInvoke).toHaveBeenCalledWith("get_pty_capture");
			expect(ptyCaptureStore.isRecording("sess-a")).toBe(true);
		});

		it("does not throw when the IPC call rejects", async () => {
			mockInvoke.mockRejectedValueOnce(new Error("boom"));
			await expect(ptyCaptureStore.refresh()).resolves.toBeUndefined();
		});
	});

	describe("toggle", () => {
		it("starts capture on this session when it isn't already recording", async () => {
			ptyCaptureStore.applyStatus({ enabled: false });
			mockInvoke.mockResolvedValueOnce({ enabled: true, session_filter: "sess-a" });

			await ptyCaptureStore.toggle("sess-a");

			expect(mockInvoke).toHaveBeenCalledWith("set_pty_capture", { enabled: true, sessionId: "sess-a" });
			expect(ptyCaptureStore.isRecording("sess-a")).toBe(true);
		});

		it("stops capture on this session when it is already recording", async () => {
			ptyCaptureStore.applyStatus({ enabled: true, session_filter: "sess-a" });
			mockInvoke.mockResolvedValueOnce({ enabled: false });

			await ptyCaptureStore.toggle("sess-a");

			expect(mockInvoke).toHaveBeenCalledWith("set_pty_capture", { enabled: false, sessionId: null });
			expect(ptyCaptureStore.isRecording("sess-a")).toBe(false);
		});

		it("does not adopt a new status when the IPC call rejects", async () => {
			ptyCaptureStore.applyStatus({ enabled: false });
			mockInvoke.mockRejectedValueOnce(new Error("boom"));

			await ptyCaptureStore.toggle("sess-a");

			expect(ptyCaptureStore.isRecording("sess-a")).toBe(false);
		});
	});
});
