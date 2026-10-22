// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
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
	subscribePty.mockClear();
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
	it("keeps the line being read when streaming crosses the tail cap instead of evicting detached content", async () => {
		const { viewport, append, visibleLine } = await mountCappedOutput();
		dragUp(viewport, 240);
		const reading = visibleLine();
		append(600, 650);
		paint();
		expect(visibleLine()).toBe(reading);
	});

	it("reveals older history at the cap instead of discarding the fetched page and resetting scroll", async () => {
		const { viewport, visibleLine, append } = await mountCappedOutput();
		viewport.scrollTop = 100;
		const reading = visibleLine();
		fireEvent.scroll(viewport);
		await waitFor(() => expect(viewport.textContent).toContain("history 0"));
		append(600, 610); // live output between prepend and paint must not shift the anchor
		paint();
		expect(visibleLine()).toBe(reading);
		expect(viewport.scrollTop).toBe(2100);
		viewport.scrollTop += 200;
		fireEvent.scroll(viewport);
		paint();
		expect(viewport.scrollTop).toBe(2300);
	});

	it("restores the bounded live tail after a detached reader returns to bottom", async () => {
		const { viewport, append, rows } = await mountCappedOutput();
		dragUp(viewport, 240);
		append(600, 650);
		paint();
		viewport.scrollTop = viewport.scrollHeight;
		fireEvent.scroll(viewport);
		append(650, 651);
		paint();
		expect(rows()).toHaveLength(500);
		expect(rows()[0].textContent).toBe("history 151");
		expect(viewport.scrollTop).toBe(viewport.scrollHeight - viewport.clientHeight);
	});

	it("does not evict the reading window while a stationary touch guards output at the cap", async () => {
		const { viewport, append, visibleLine } = await mountCappedOutput();
		fireEvent.touchStart(viewport);
		const reading = visibleLine();
		append(600, 650);
		paint();
		expect(visibleLine()).toBe(reading);
		fireEvent.touchCancel(viewport);
		append(650, 651);
		paint();
		expect(viewport.scrollTop).toBe(viewport.scrollHeight - viewport.clientHeight);
	});

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
	it("keeps following when output grows between its own scroll-to-bottom and the scroll event it caused", async () => {
		const { viewport, chunk } = await mountOutput();
		chunk();
		paint(); // follow scroll: scrollTop = bottom, scroll event not yet delivered
		chunk(); // next chunk lands before the browser dispatches that scroll event
		fireEvent.scroll(viewport); // the follow scroll's own event, now measured against grown content
		chunk();
		paint();
		expect(viewport.scrollTop).toBe(viewport.scrollHeight - viewport.clientHeight);
	});
});

/** Fixed-height DOM rows model layout; assertions read the actual rendered content. */
async function mountCappedOutput() {
	vi.mocked(fetch).mockImplementation(async (input) => {
		const url = new URL(String(input), "http://localhost");
		const start = Number(url.searchParams.get("offset") ?? 100);
		const lines = Array.from({ length: 100 }, (_, i) => `history ${start + i}`);
		return { ok: true, json: async () => ({ lines, total_lines: 200, offset: start }) } as Response;
	});
	const view = render(() => <OutputView sessionId="scroll-session" />);
	const viewport = view.container.firstElementChild as HTMLDivElement;
	const rows = () => Array.from(viewport.querySelectorAll<HTMLDivElement>("pre > div"));
	let top = 0;
	Object.defineProperties(viewport, {
		clientHeight: { get: () => 400 },
		scrollHeight: { get: () => rows().length * 20 },
		scrollTop: {
			get: () => top,
			set: (value: number) => {
				top = Math.max(0, Math.min(value, viewport.scrollHeight - 400));
			},
		},
	});
	await waitFor(() => expect(subscribePty).toHaveBeenCalled());
	const append = (start: number, end: number) => {
		stream.onLogLines?.(Array.from({ length: end - start }, (_, i) => ({ spans: [{ text: `history ${start + i}` }] })));
	};
	append(200, 600);
	paint();
	fireEvent.scroll(viewport);
	return { viewport, rows, append, visibleLine: () => rows()[Math.floor(viewport.scrollTop / 20)]?.textContent };
}

// Catches: display:none leaves CLI detached or remounting loses the existing PTY stream.
it("returns the mounted output to bottom after hidden Chat output", async () => {
	const [visible, setVisible] = createSignal(true);
	const view = render(() => <OutputView sessionId="hidden-output" visible={visible()} />);
	const viewport = view.container.firstElementChild as HTMLDivElement;
	let top = 0;
	Object.defineProperties(viewport, {
		clientHeight: { value: 400 },
		scrollHeight: { value: 2000 },
		scrollTop: {
			get: () => top,
			set: (value: number) => {
				top = Math.min(value, 1600);
			},
		},
	});
	await waitFor(() => expect(subscribePty).toHaveBeenCalled());
	paint();
	fireEvent.scroll(viewport);
	dragUp(viewport, 240);
	setVisible(false);
	stream.onScreenRows?.(["output during Chat"]);
	paint();
	expect(viewport.style.display).toBe("none");
	setVisible(true);
	paint();
	expect(view.container.firstElementChild).toBe(viewport);
	expect(subscribePty).toHaveBeenCalledTimes(1);
	expect(viewport.scrollTop).toBe(1600);
	expect(viewport.textContent).toContain("output during Chat");
});

// Catches: changing font metrics while reading history loses the visible row.
it("keeps the detached reading row anchored when the font size changes", async () => {
	const { mobileFontSizes } = await import("../mobileFontSize");
	while (mobileFontSizes.terminal() > 14) mobileFontSizes.step("terminal", -1);
	const { viewport } = await mountOutput();
	dragUp(viewport, 600);
	const row = viewport.querySelector("pre > div") as HTMLElement;
	const text = viewport.querySelector("pre") as HTMLPreElement;
	Object.defineProperty(viewport, "getBoundingClientRect", { value: () => ({ top: 0 }) });
	// Model a row whose layout moves 100px as the font grows; the scroll owner
	// must compensate that delta rather than keep the stale pixel scrollTop.
	row.getBoundingClientRect = () =>
		({
			top: 1200 + (Number.parseInt(text.style.fontSize, 10) - 14) * 100 - viewport.scrollTop,
			bottom: 1250 + (Number.parseInt(text.style.fontSize, 10) - 14) * 100 - viewport.scrollTop,
		}) as DOMRect;
	const before = row.getBoundingClientRect().top;
	mobileFontSizes.step("terminal", 1);
	paint();
	expect(text.style.fontSize).toBe("15px");
	expect(row.getBoundingClientRect().top).toBe(before);
	mobileFontSizes.step("terminal", -1);
	paint();
});
