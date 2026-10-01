import { render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Round 3: the context menu of a hover-only link opens from the hover. A left press on an underlined path while the app reports the mouse (Claude
 * Code fullscreen). The contract is the user's: the underlined name opens when
 * clicked, every other press still belongs to the app. The last case is the same
 * contract without mouse reporting (Claude with the alternate screen disabled).
 */

const { invoke, frameSink } = vi.hoisted(() => ({
	invoke: vi.fn(),
	frameSink: { current: null as ((data: ArrayBuffer) => void) | null },
}));

vi.mock("../../components/Terminal/canvasTerminalTransport", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../components/Terminal/canvasTerminalTransport")>()),
	createTransport: () => ({
		onEvent: vi.fn().mockResolvedValue(undefined),
		subscribe: vi.fn(async (onFrame: (data: ArrayBuffer) => void) => {
			frameSink.current = onFrame;
		}),
		resubscribe: vi.fn().mockResolvedValue(undefined),
		unsubscribe: vi.fn(),
		ackFrame: vi.fn(),
		invoke,
	}),
}));
vi.mock("../../components/Terminal/gridRenderer", () => ({
	createGridRenderer: () => ({ setTheme: vi.fn(), invalidateCaches: vi.fn(), paintGrid: vi.fn(), paintRow: vi.fn() }),
}));

import CanvasTerminal from "../../components/Terminal/CanvasTerminal";

const ROWS = 2;
const COLS = 30;
const HEADER_SIZE = 26;
const CELL_SIZE = 11;
const GUTTER = 6;
const CELL_W = 8;
const ROW0 = "  followups.md  plain words";
const WRAP0 = "                  docs/followu";
const WRAP1 = "ps.md";
/** `followups.md` occupies columns 2-13 of row 0. */

/** Tagged by the app as an OSC 8 hyperlink to `followups.md`; the text itself is no path. Columns 6-8. */
const OSC8_ROW = "  see doc  plain words";
const OSC8_COL = 7;

/** A full screen whose row 0 is `row0`, with mouse reporting (SGR, button-event tracking) on unless `mouseMode` is 0. */
function frame(row0 = ROW0, mouseMode = 2): ArrayBuffer {
	const buffer = new ArrayBuffer(HEADER_SIZE + ROWS * (4 + COLS * CELL_SIZE));
	const view = new DataView(buffer);
	view.setUint16(0, ROWS, true);
	view.setUint8(6, 1);
	view.setUint8(17, mouseMode === 0 ? 0 : (mouseMode << 3) | 0x20);
	view.setUint16(18, ROWS, true);
	view.setUint16(20, COLS, true);
	let offset = HEADER_SIZE;
	for (let r = 0; r < ROWS; r++) {
		view.setUint16(offset, r, true);
		view.setUint16(offset + 2, COLS, true);
		offset += 4;
		for (const char of (r === 0 ? row0 : "").padEnd(COLS, " ")) {
			view.setUint32(offset, char.codePointAt(0) ?? 0, true);
			offset += CELL_SIZE;
		}
	}
	return buffer;
}

const x = (col: number) => GUTTER + col * CELL_W + CELL_W / 2;

function fire(target: Element, type: string, col: number, init: MouseEventInit = {}) {
	target.dispatchEvent(
		new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x(col), clientY: 2, button: 0, ...init }),
	);
}

function click(target: Element, pressCol: number, releaseCol: number, init: MouseEventInit = {}) {
	fire(target, "mousedown", pressCol, { buttons: 1, ...init });
	fire(target, "mouseup", releaseCol, init);
	fire(target, "click", releaseCol, init);
}

const ptyWrites = () =>
	invoke.mock.calls.filter(([cmd]) => cmd === "write_pty").map(([, a]) => (a as { data: string }).data);

/** Like frame() but with an explicit second row and the wrapped flag on row 0. */
function frame2(row0: string, row1: string, mouseMode: 0 | 2, wrapped: boolean): ArrayBuffer {
	const buffer = new ArrayBuffer(HEADER_SIZE + ROWS * (4 + COLS * CELL_SIZE));
	const view = new DataView(buffer);
	view.setUint16(0, ROWS, true);
	view.setUint8(6, 1);
	view.setUint8(17, mouseMode === 0 ? 0 : (mouseMode << 3) | 0x20);
	view.setUint16(18, ROWS, true);
	view.setUint16(20, COLS, true);
	let offset = HEADER_SIZE;
	for (let r = 0; r < ROWS; r++) {
		view.setUint16(offset, r, true);
		view.setUint16(offset + 2, COLS | (r === 0 && wrapped ? 0x8000 : 0), true);
		offset += 4;
		for (const char of (r === 0 ? row0 : row1).padEnd(COLS, " ")) {
			view.setUint32(offset, char.codePointAt(0) ?? 0, true);
			offset += CELL_SIZE;
		}
	}
	return buffer;
}

describe("CanvasTerminal link press, critic 1336 round 3", () => {
	const onOpen = vi.fn();
	let screenRow0 = ROW0;
	let screenRow1 = "";
	let lateFileExists = false;
	let osc8Active = false;
	let osc8Target = "followups.md";
	let canvas: Element;
	let unmount: () => void;

	beforeEach(async () => {
		onOpen.mockClear();
		invoke.mockReset();
		screenRow0 = ROW0;
		screenRow1 = "";
		lateFileExists = false;
		osc8Active = false;
		osc8Target = "followups.md";
		invoke.mockImplementation(
			async (cmd: string, args: { candidate?: string; candidates?: string[]; col?: number; row?: number }) => {
				if (cmd === "terminal_hyperlink_span") {
					const col = args.col ?? -1;
					return osc8Active && col >= 6 && col < 9 ? [6, 9, osc8Target] : null;
				}
				if (cmd === "terminal_get_row_text") return (args.row === 1 ? screenRow1 : screenRow0).trimEnd();
				if (cmd === "terminal_get_logical_line")
					return [0, screenRow1 ? (screenRow0.padEnd(COLS, " ") + screenRow1).trimEnd() : screenRow0.trimEnd()];
				const resolve = (c: string) =>
					c.startsWith("followups") ||
					c.startsWith("other") ||
					c.startsWith("docs/followups") ||
					(c.startsWith("later") && lateFileExists)
						? { absolute_path: `/cwd/${c}`, is_directory: false }
						: null;
				if (cmd === "resolve_terminal_path") return resolve(args.candidate ?? "");
				if (cmd === "resolve_terminal_paths") return (args.candidates ?? []).map(resolve);
				return null;
			},
		);
		frameSink.current = null;
		Object.defineProperty(document, "fonts", {
			configurable: true,
			value: { load: () => Promise.resolve([]), ready: Promise.resolve() },
		});
		vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
			new Proxy(
				{},
				{
					get: (_t, prop) =>
						prop === "measureText"
							? () => ({ width: CELL_W, fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 })
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

		const view = render(() => (
			<CanvasTerminal sessionId="link-1336r3" terminalId="link-1336r3" onOpenFilePath={onOpen} />
		));
		unmount = view.unmount;
		await waitFor(() => expect(frameSink.current).not.toBeNull());
		// The reset above narrows `current` to null for the compiler; the subscribe mock refills it.
		(frameSink.current as ((data: ArrayBuffer) => void) | null)?.(frame());
		const found = view.container.querySelector('canvas[tabindex="0"]');
		if (!found) throw new Error("terminal canvas not mounted");
		canvas = found;
		// The underline exists once verification has resolved the name.
		await waitFor(() => expect(invoke.mock.calls.map(([c]) => c)).toContain("resolve_terminal_paths"));
		await new Promise((r) => setTimeout(r, 20));
		invoke.mockClear();
	});

	afterEach(() => {
		unmount();
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	const send = (data: ArrayBuffer) => (frameSink.current as ((d: ArrayBuffer) => void) | null)?.(data);
	const hoverOsc8 = async () => {
		osc8Active = true;
		screenRow0 = OSC8_ROW;
		send(frame(OSC8_ROW, 2));
		await new Promise((r) => setTimeout(r, 20));
		fire(document.body, "mousemove", OSC8_COL);
		await waitFor(() => expect(canvas.getAttribute("style") ?? "").toContain("pointer"));
	};

	const contextMenu = (col: number) => {
		fire(canvas, "mousedown", col, { button: 2, buttons: 2 });
		const ev = new MouseEvent("contextmenu", {
			bubbles: true,
			cancelable: true,
			clientX: x(col),
			clientY: 2,
			button: 2,
		});
		canvas.dispatchEvent(ev);
		return ev;
	};

	const clickMenuItem = (label: string) => {
		const el = Array.from(document.body.querySelectorAll("*")).find(
			(n) => n.children.length === 0 && n.textContent?.trim() === label,
		) as HTMLElement | undefined;
		if (!el) throw new Error(`menu item ${label} not rendered`);
		el.click();
	};

	// Catches: the menu serving the target the hover resolved earlier, so Open after the hyperlink
	// target changed (same visible text) opens a link that is no longer under the pointer.
	it("Open from a hover-only link menu opens the target under the pointer now", async () => {
		await hoverOsc8();
		osc8Target = "other.md";
		const ev = contextMenu(OSC8_COL);
		expect(ev.defaultPrevented).toBe(true);
		await waitFor(() => expect(document.body.textContent ?? "").toContain("Copy link"));
		clickMenuItem("Open");
		expect(onOpen).toHaveBeenCalledWith("/cwd/other.md", undefined, undefined);
		expect(onOpen).not.toHaveBeenCalledWith("/cwd/followups.md", undefined, undefined);
	});

	// Catches: the hover-only menu opening but its Open item wired to nothing (menu target unset).
	it("Open from a hover-only link menu opens the hovered link", async () => {
		await hoverOsc8();
		contextMenu(OSC8_COL);
		await waitFor(() => expect(document.body.textContent ?? "").toContain("Copy link"));
		clickMenuItem("Open");
		expect(onOpen).toHaveBeenCalledWith("/cwd/followups.md", undefined, undefined);
	});

	// Catches: the hover branch firing for a cell outside the hovered link, so a right press next to
	// the link steals the app's default menu / opens a link menu for a link not under the pointer.
	it("leaves the default menu alone on a right press next to a hovered link", async () => {
		await hoverOsc8();
		const ev = contextMenu(OSC8_COL + 3);
		await new Promise((r) => setTimeout(r, 50));
		expect(ev.defaultPrevented).toBe(false);
		expect(document.body.textContent ?? "").not.toContain("Copy link");
	});

	// Catches: a hover claim surviving an in-place redraw of its text, so the menu of a link that is
	// no longer on screen opens from the stale hover.
	it("leaves the default menu alone once the hovered text was redrawn in place", async () => {
		await hoverOsc8();
		screenRow0 = "  see xyz  plain words";
		send(frame("  see xyz  plain words", 2));
		await new Promise((r) => setTimeout(r, 20));
		const ev = contextMenu(OSC8_COL);
		await new Promise((r) => setTimeout(r, 50));
		expect(document.body.textContent ?? "").not.toContain("Copy link");
		expect(ev.defaultPrevented).toBe(false);
	});

	// Catches: the default menu being suppressed asynchronously (after the probe) for a hover-only
	// link: preventDefault must already be set when the handler's synchronous part returns.
	it("suppresses the default menu synchronously for a hover-only link", async () => {
		await hoverOsc8();
		const ev = contextMenu(OSC8_COL);
		expect(ev.defaultPrevented).toBe(true);
	});
});
