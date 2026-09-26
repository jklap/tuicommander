import { render, waitFor } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

const { subscribe, unsubscribe } = vi.hoisted(() => ({
	subscribe: vi.fn().mockRejectedValue(new Error("WebSocket connection failed")),
	unsubscribe: vi.fn(),
}));

vi.mock("../../components/Terminal/canvasTerminalTransport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/canvasTerminalTransport")>()),
	createTransport: () => ({
		onEvent: vi.fn().mockResolvedValue(undefined),
		subscribe,
		unsubscribe,
		invoke: vi.fn().mockResolvedValue(undefined),
	}),
}));
vi.mock("../../components/Terminal/gridRenderer", () => ({
	createGridRenderer: () => ({ setTheme: vi.fn(), invalidateCaches: vi.fn(), paintGrid: vi.fn(), paintRow: vi.fn() }),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";
import { toastsStore } from "../../stores/toasts";

describe("CanvasTerminal stream errors", () => {
	// Catches: a failed WebSocket grid subscription only reaches the log and leaves a blank terminal.
	it("shows a toast when the grid stream cannot attach", async () => {
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
		const view = render(() => <CanvasTerminal sessionId="remote-stream" terminalId="remote-tab" />);
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
});
