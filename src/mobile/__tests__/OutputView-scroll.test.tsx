// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SubscribePtyOptions } from "../../transport";
import { OutputView } from "../components/OutputView";

const { subscribePty } = vi.hoisted(() => ({ subscribePty: vi.fn() }));
vi.mock("../../transport", () => ({ subscribePty }));
vi.mock("../../stores/appLogger", () => ({ appLogger: { warn: vi.fn(), error: vi.fn() } }));

let frames: FrameRequestCallback[];
let stream: SubscribePtyOptions;

beforeEach(() => {
	frames = [];
	vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
		frames.push(callback);
		return frames.length;
	});
	vi.stubGlobal(
		"fetch",
		vi.fn(async () => ({
			ok: true,
			json: async () => ({ lines: ["initial"], total_lines: 1, offset: 0 }),
		})),
	);
	subscribePty.mockImplementation(async (_id, _output, _exit, options) => {
		stream = options;
		return () => {};
	});
});

afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
});

function paint() {
	const pending = frames.splice(0);
	for (const callback of pending) callback(0);
}

async function mountOutput() {
	const view = render(() => <OutputView sessionId="scroll-session" />);
	const viewport = view.container.firstElementChild as HTMLDivElement;
	let height = 2000;
	let top = 0;
	Object.defineProperties(viewport, {
		clientHeight: { get: () => 400 },
		scrollHeight: { get: () => height },
		scrollTop: {
			get: () => top,
			set: (value: number) => {
				top = Math.max(0, Math.min(value, height - 400));
			},
		},
	});
	await waitFor(() => expect(subscribePty).toHaveBeenCalled());
	paint();
	fireEvent.scroll(viewport);
	return {
		viewport,
		chunk() {
			height += 20;
			stream.onLogLines?.([{ spans: [{ text: "next line" }] }]);
			stream.onScreenRows?.(["live output"]);
		},
	};
}

function dragUp(viewport: HTMLDivElement, pixels: number) {
	fireEvent.touchStart(viewport, { touches: [{ clientY: 200 }] });
	viewport.scrollTop -= pixels;
	fireEvent.touchMove(viewport, { touches: [{ clientY: 200 - pixels }] });
	fireEvent.scroll(viewport);
	fireEvent.touchEnd(viewport, { touches: [] });
}

describe("mobile output follow", () => {
	it("keeps a small upward touch drag detached across output chunks instead of treating it as bottom", async () => {
		const { viewport, chunk } = await mountOutput();
		dragUp(viewport, 40);
		const detachedTop = viewport.scrollTop;
		for (let i = 0; i < 3; i++) {
			chunk();
			paint();
			expect(viewport.scrollTop).toBe(detachedTop);
		}
	});

	it("does not let an output frame queued before the upward drag overwrite the gesture", async () => {
		const { viewport, chunk } = await mountOutput();
		chunk();
		dragUp(viewport, 240);
		const detachedTop = viewport.scrollTop;
		paint();
		expect(viewport.scrollTop).toBe(detachedTop);
	});

	it("resumes following only after returning to the bottom, including fractional scroll positions", async () => {
		const { viewport, chunk } = await mountOutput();
		dragUp(viewport, 40);
		viewport.scrollTop = viewport.scrollHeight - viewport.clientHeight - 0.5;
		fireEvent.scroll(viewport);
		chunk();
		paint();
		expect(viewport.scrollTop).toBe(viewport.scrollHeight - viewport.clientHeight);
	});

	it("keeps a stationary touch from scrolling output and releases the guard on touch cancel", async () => {
		const { viewport, chunk } = await mountOutput();
		const before = viewport.scrollTop;
		fireEvent.touchStart(viewport);
		chunk();
		paint();
		expect(viewport.scrollTop).toBe(before);
		fireEvent.touchCancel(viewport);
		chunk();
		paint();
		expect(viewport.scrollTop).toBe(viewport.scrollHeight - viewport.clientHeight);
	});
});
