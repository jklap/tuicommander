import { render, waitFor } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

const { subscribe, unsubscribe, eventHandlers } = vi.hoisted(() => ({
	eventHandlers: new Map<string, (error: unknown) => void>(),
	subscribe: vi.fn().mockRejectedValue(new Error("WebSocket connection failed")),
	unsubscribe: vi.fn(),
}));

vi.mock("../../components/Terminal/canvasTerminalTransport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/canvasTerminalTransport")>()),
	createTransport: () => ({
		onStreamError: (handler: (error: unknown) => void) => eventHandlers.set("stream-error", handler),
		onEvent: vi.fn(async (type: string, handler: (error: unknown) => void) => {
			// Stream failures are WS-local callbacks, never PTY events.
			// Catches reverting the consumer to an orphan Tauri listener.
			if (type !== "stream-error") eventHandlers.set(type, handler);
		}),
		subscribe,
		unsubscribe,
		invoke: vi.fn().mockResolvedValue(undefined),
	}),
}));
vi.mock("../../components/Terminal/gridRenderer", () => ({
	createGridRenderer: () => ({
		setTheme: vi.fn(),
		invalidateCaches: vi.fn(),
		paintGrid: vi.fn(),
		paintRow: vi.fn(),
		buildFontStyle: () => "14px monospace",
	}),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";
import { ToastContainer } from "../../components/ToastContainer/ToastContainer";
import { toastsStore } from "../../stores/toasts";

function openTerminal() {
	vi.clearAllMocks();
	eventHandlers.clear();
	Object.defineProperty(document, "fonts", {
		configurable: true,
		value: { load: () => Promise.resolve([]), ready: Promise.resolve() },
	});
	vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
		clearRect: vi.fn(),
		setTransform: vi.fn(),
	} as unknown as CanvasRenderingContext2D);
	vi.stubGlobal(
		"ResizeObserver",
		class {
			observe() {}
			disconnect() {}
		},
	);
	vi.stubGlobal(
		"IntersectionObserver",
		class {
			observe() {}
			disconnect() {}
		},
	);
	return render(() => (
		<>
			<CanvasTerminal sessionId="remote-stream" terminalId="remote-tab" />
			<ToastContainer />
		</>
	));
}

describe("CanvasTerminal stream errors", () => {
	// Catches: a failed WebSocket grid subscription only reaches the log and leaves a blank terminal.
	it("shows a toast when the grid stream cannot attach", async () => {
		const view = openTerminal();
		try {
			await waitFor(() => expect(subscribe).toHaveBeenCalled());
			await waitFor(() =>
				expect(
					toastsStore.toasts.some(
						(toast) =>
							toast.title === "Terminal stream failed" && toast.message.includes("WebSocket connection failed"),
					),
				).toBe(true),
			);
			expect(unsubscribe).toHaveBeenCalled();
		} finally {
			view.unmount();
			for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
			vi.restoreAllMocks();
			vi.unstubAllGlobals();
		}
	});
	// Catches: only rejected subscribe promises show a toast; failures after socket open never reach the rendered notification.
	it("keeps a post-attach stream error visible until dismissed", async () => {
		subscribe.mockResolvedValueOnce(undefined);
		const view = openTerminal();
		try {
			await waitFor(() => expect(eventHandlers.has("stream-error")).toBe(true));
			vi.useFakeTimers();
			eventHandlers.get("stream-error")?.(new Error("Timed out waiting for the initial terminal frame"));
			expect(view.container.textContent).toContain("Terminal stream failed");
			expect(view.container.textContent).toContain("Timed out waiting for the initial terminal frame");
			await vi.advanceTimersByTimeAsync(60_000);
			expect(view.container.textContent).toContain("Terminal stream failed");
		} finally {
			vi.useRealTimers();
			view.unmount();
			eventHandlers.clear();
			for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
			vi.restoreAllMocks();
			vi.unstubAllGlobals();
		}
	});
});
