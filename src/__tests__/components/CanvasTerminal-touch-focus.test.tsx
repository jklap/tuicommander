import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, subscribed } = vi.hoisted(() => ({ invoke: vi.fn(), subscribed: vi.fn() }));
vi.mock("../../components/Terminal/canvasTerminalTransport", async (original) => ({
	...(await original<typeof import("../../components/Terminal/canvasTerminalTransport")>()),
	createTransport: () => ({
		onEvent: vi.fn().mockResolvedValue(undefined),
		subscribe: subscribed,
		unsubscribe: vi.fn(),
		ackFrame: vi.fn(),
		invoke,
	}),
}));
vi.mock("../../components/Terminal/gridRenderer", () => ({
	createGridRenderer: () => ({ setTheme: vi.fn(), invalidateCaches: vi.fn(), paintGrid: vi.fn(), paintRow: vi.fn() }),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";

function touch(canvas: Element, type: string, y: number) {
	const point: Touch = {
		identifier: 0,
		clientX: 100,
		clientY: y,
		target: canvas,
		screenX: 100,
		screenY: y,
		pageX: 100,
		pageY: y,
		force: 1,
		radiusX: 1,
		radiusY: 1,
		rotationAngle: 0,
	};
	canvas.dispatchEvent(
		new TouchEvent(type, {
			bubbles: true,
			cancelable: true,
			touches: type === "touchend" ? [] : [point],
			changedTouches: [point],
		}),
	);
}

const writes = () =>
	invoke.mock.calls.filter(([command]) => command === "write_pty").map(([, args]) => (args as { data: string }).data);

describe("CanvasTerminal touch input focus", () => {
	let canvas: Element;
	let unmount: () => void;
	beforeEach(async () => {
		invoke.mockReset().mockResolvedValue(undefined);
		subscribed.mockReset().mockResolvedValue(undefined);
		vi.spyOn(navigator, "maxTouchPoints", "get").mockReturnValue(5);
		Object.defineProperty(document, "fonts", {
			configurable: true,
			value: { load: () => Promise.resolve([]), ready: Promise.resolve() },
		});
		vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
			new Proxy(
				{},
				{
					get: (_target, property) =>
						property === "measureText"
							? () => ({ width: 8, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 })
							: vi.fn(),
				},
			) as unknown as CanvasRenderingContext2D,
		);
		vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
			width: 800,
			height: 1000,
			top: 0,
			left: 0,
			right: 800,
			bottom: 1000,
			x: 0,
			y: 0,
			toJSON: () => ({}),
		});
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
		const view = render(() => <CanvasTerminal sessionId="touch-focus" terminalId="touch-focus" />);
		unmount = view.unmount;
		await waitFor(() => expect(subscribed).toHaveBeenCalled());
		canvas = view.container.querySelector('canvas[tabindex="0"]')!;
		invoke.mockClear();
	});
	afterEach(() => {
		unmount?.();
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	// Catches: a tap focuses the offscreen textarea, then the compatibility mouse event switches inputs.
	it("keeps the same input focused after a tap and compatibility mouse press", () => {
		touch(canvas, "touchstart", 700);
		touch(canvas, "touchend", 700);
		const input = document.activeElement;
		expect(input).toBeInstanceOf(HTMLInputElement);
		canvas.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
		expect(document.activeElement).toBe(input);
	});

	// Catches: touch input bypasses the shared iOS input handler or forwards text twice.
	it("forwards soft keyboard text once through the focused input", () => {
		touch(canvas, "touchstart", 700);
		touch(canvas, "touchend", 700);
		(document.activeElement as HTMLInputElement).value = "ciao";
		document.activeElement!.dispatchEvent(
			new InputEvent("input", {
				bubbles: true,
				inputType: "insertText",
				data: "ciao",
			}),
		);
		expect(writes()).toEqual(["ciao"]);
	});

	// Catches: the touch textarea clears to empty and drops soft keyboard deletion events.
	it("forwards repeated soft keyboard deletion through the focused input", () => {
		touch(canvas, "touchstart", 700);
		touch(canvas, "touchend", 700);
		const input = document.activeElement as HTMLInputElement;
		expect(input.value.length).toBeGreaterThan(0);
		for (let i = 0; i < 2; i++) {
			input.value = "";
			input.dispatchEvent(
				new InputEvent("input", {
					bubbles: true,
					inputType: "deleteContentBackward",
				}),
			);
			expect(input.value.length).toBeGreaterThan(0);
			expect(input.selectionStart).toBe(input.value.length);
		}
		expect(writes()).toEqual(["\x7f", "\x7f"]);
	});
});
