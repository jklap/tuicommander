import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { subscribe, streamError } = vi.hoisted(() => ({
	subscribe: vi.fn<() => Promise<void>>(),
	streamError: { handler: undefined as ((error: unknown) => void) | undefined },
}));

vi.mock("../../components/Terminal/canvasTerminalTransport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/canvasTerminalTransport")>()),
	createTransport: () => ({
		onStreamError: (handler: (error: unknown) => void) => {
			streamError.handler = handler;
		},
		onEvent: async () => {},
		subscribe,
		unsubscribe: () => {
			streamError.handler = undefined;
		},
		invoke: async () => undefined,
	}),
}));
vi.mock("../../components/Terminal/gridRenderer", () => ({
	createGridRenderer: () => ({ setTheme: vi.fn(), invalidateCaches: vi.fn(), paintGrid: vi.fn(), paintRow: vi.fn() }),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";
import { ToastContainer } from "../../components/ToastContainer/ToastContainer";
import { toastsStore } from "../../stores/toasts";

beforeEach(() => {
	subscribe.mockReset().mockResolvedValue(undefined);
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
});
afterEach(() => {
	for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

const openTerminal = () => render(() => <CanvasTerminal sessionId="closed-stream" terminalId="closed-tab" />);

describe("stream notice ownership on terminal disposal", () => {
	// Catches: an orphan reconnect banner survives closing and reopening a healthy terminal.
	it("closing a reconnecting terminal removes its notice and preserves unrelated errors", async () => {
		const notices = render(() => <ToastContainer />);
		const terminal = openTerminal();
		try {
			await waitFor(() => expect(subscribe).toHaveBeenCalled());
			toastsStore.add("Unrelated failure", "Keep this error", "error", false, undefined, 0);
			streamError.handler?.(new Error("Terminal stream disconnected"));
			expect(notices.container.textContent).toContain("Terminal stream reconnecting");
			terminal.unmount();
			expect(notices.container.textContent).not.toContain("Terminal stream reconnecting");
			expect(notices.container.textContent).toContain("Keep this error");
		} finally {
			terminal.unmount();
			notices.unmount();
		}
	});

	// Catches: a pending attach rejection recreates a banner after the terminal has been closed.
	it("a subscription rejection after terminal closure cannot publish a stale failure notice", async () => {
		let rejectAttach!: (error: Error) => void;
		subscribe.mockImplementationOnce(
			() =>
				new Promise<void>((_, reject) => {
					rejectAttach = reject;
				}),
		);
		const notices = render(() => <ToastContainer />);
		const terminal = openTerminal();
		try {
			await waitFor(() => expect(subscribe).toHaveBeenCalled());
			terminal.unmount();
			rejectAttach(new Error("Closed socket attach failed"));
			await Promise.resolve();
			await Promise.resolve();
			expect(notices.container.textContent).not.toContain("Closed socket attach failed");
		} finally {
			terminal.unmount();
			notices.unmount();
		}
	});
});
