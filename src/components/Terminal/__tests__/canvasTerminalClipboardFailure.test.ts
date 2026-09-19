/**
 * Failure-path coverage for CanvasTerminal's clipboard writes (copySelection,
 * copyLink, and the smart-selection "copy" action) — these already had
 * correct catch/status handling, they just had no test proving it.
 */

import { fireEvent, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FakeClipboardItem } from "../../../__tests__/mocks/clipboardItem";
import { mockInvoke } from "../../../__tests__/mocks/tauri";
import { appLogger } from "../../../stores/appLogger";
import { settingsStore } from "../../../stores/settings";
import { GUTTER_PX } from "../canvasTerminalUtils";
import { buildTextFrame } from "./helpers/frameFixture";
import {
	createFakeTransport,
	FIXED_CELL_METRICS,
	mountCanvasTerminal,
	stubCanvasEnvironment,
} from "./helpers/mountCanvasTerminal";

const fakeTransport = vi.hoisted(() => ({ current: null as ReturnType<typeof createFakeTransport> | null }));

vi.mock("../canvasTerminalTransport", async (importOriginal) => {
	const actual = await importOriginal<typeof import("../canvasTerminalTransport")>();
	return { ...actual, createTransport: () => fakeTransport.current! };
});

vi.mock("../glyphCache", () => ({
	getSharedMetrics: () => FIXED_CELL_METRICS,
	acquireCache: vi.fn(),
	releaseCache: vi.fn(),
	invalidateGlyphCache: vi.fn(),
}));

function cellPoint(col: number, row: number) {
	return {
		clientX: GUTTER_PX + col * FIXED_CELL_METRICS.cellWidth + FIXED_CELL_METRICS.cellWidth / 2,
		clientY: row * FIXED_CELL_METRICS.cellHeight + FIXED_CELL_METRICS.cellHeight / 2,
	};
}

function doubleClick(canvas: HTMLCanvasElement, col: number, row: number) {
	const point = cellPoint(col, row);
	fireEvent.mouseDown(canvas, { button: 0, ...point });
	fireEvent.mouseDown(canvas, { button: 0, ...point });
	fireEvent.mouseUp(canvas, { button: 0, ...point });
}

describe("CanvasTerminal clipboard failure handling", () => {
	let restoreEnv: () => void;
	let setStatusInfo: ReturnType<typeof vi.fn>;

	beforeEach(() => {
		restoreEnv = stubCanvasEnvironment();
		fakeTransport.current = createFakeTransport();
		mockInvoke.mockReset();
		setStatusInfo = vi.fn();
		(window as unknown as Record<string, unknown>).__tuic_setStatusInfo = setStatusInfo;
		vi.spyOn(appLogger, "warn").mockImplementation(() => {});
	});

	afterEach(() => {
		restoreEnv();
		settingsStore.setLinkActivation("click");
		settingsStore.setDoubleClickAction("smart");
		settingsStore.setCopyOnSelect(true);
		delete (window as unknown as Record<string, unknown>).__tuic_setStatusInfo;
		vi.restoreAllMocks();
	});

	it('does not auto-copy on select when "Copy on Select" is disabled, but Cmd/Ctrl+C still copies manually', async () => {
		settingsStore.setCopyOnSelect(false);
		mockInvoke.mockResolvedValue(undefined);
		const mounted = await mountCanvasTerminal({ sessionId: "clip6", terminalId: "tclip6" });
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		doubleClick(mounted.canvas, 5, 0); // selects "bar" via mouseup — auto-copy must NOT fire
		await new Promise((r) => setTimeout(r, 0)); // let any (incorrectly) fired auto-copy settle

		expect(setStatusInfo).not.toHaveBeenCalled();

		// The selection itself is still intact — "Copy on Select" only gates the
		// auto-copy-on-drag path, never the selection or the manual Cmd+C shortcut.
		// Keydown is bound to the hidden composition <input> (keyInputRef), not the canvas.
		const keyInput = mounted.container.querySelector('input[aria-hidden="true"]') as HTMLInputElement;
		fireEvent.keyDown(keyInput, { key: "c", metaKey: true, ctrlKey: true });

		await waitFor(() => {
			expect(setStatusInfo).toHaveBeenCalledWith("Copied to clipboard");
		});
		expect(mounted.ref.getSelectionText()).toBe("bar");
		await mounted.dispose();
	});

	it("copySelection surfaces a failure status and logs instead of claiming success", async () => {
		mockInvoke.mockImplementation(async (cmd: string) => {
			// Target only the clipboard write: the component now issues other
			// mount-time invokes first (e.g. inline-image placement hydration),
			// so a bare mockRejectedValueOnce would be consumed by one of those.
			if (cmd === "plugin:clipboard-manager|write_text") {
				throw new DOMException("Write permission denied.", "NotAllowedError");
			}
			return undefined;
		});
		const mounted = await mountCanvasTerminal({ sessionId: "clip1", terminalId: "tclip1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		doubleClick(mounted.canvas, 5, 0); // selects "bar", mouseup fires copySelection()

		await waitFor(() => {
			expect(setStatusInfo).toHaveBeenCalledWith("Copy failed — clipboard unavailable");
		});
		expect(setStatusInfo).not.toHaveBeenCalledWith("Copied to clipboard");
		expect(appLogger.warn).toHaveBeenCalledWith("terminal", "Clipboard write failed", expect.anything());
		// The selected text is still cached even though the write itself failed — the
		// resolved text and the write outcome are independent (see copySelection's doc
		// comment): a failed clipboard write must not leave a stale/empty cache behind.
		expect(mounted.ref.getSelectionText()).toBe("bar");
		await mounted.dispose();
	});

	it("copySelection reports success once the clipboard write resolves", async () => {
		mockInvoke.mockResolvedValue(undefined);
		const mounted = await mountCanvasTerminal({ sessionId: "clip2", terminalId: "tclip2" });
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		doubleClick(mounted.canvas, 5, 0);

		await waitFor(() => {
			expect(setStatusInfo).toHaveBeenCalledWith("Copied to clipboard");
		});
		expect(appLogger.warn).not.toHaveBeenCalled();
		await mounted.dispose();
	});

	it("right-click Copy Link does not throw when the clipboard write is denied", async () => {
		mockInvoke.mockImplementation(async (cmd: string) => {
			// Target only the clipboard write: the component now issues other
			// mount-time invokes first (e.g. inline-image placement hydration),
			// so a bare mockRejectedValueOnce would be consumed by one of those.
			if (cmd === "plugin:clipboard-manager|write_text") {
				throw new DOMException("Write permission denied.", "NotAllowedError");
			}
			return undefined;
		});
		const mounted = await mountCanvasTerminal({ sessionId: "clip3", terminalId: "tclip3" });
		fakeTransport.current!.pushFrame(buildTextFrame(["open https://example.com/a now"], 40));

		const point = cellPoint(7, 0);
		fireEvent.contextMenu(mounted.canvas, { button: 2, ...point });
		const copyLinkItem = await waitFor(() => {
			const label = [...document.querySelectorAll(".label")].find((el) => el.textContent === "Copy");
			expect(label).toBeTruthy();
			return label!.closest("button") as HTMLElement;
		});
		expect(() => fireEvent.click(copyLinkItem)).not.toThrow();
		await mounted.dispose();
	});

	describe("browser mode", () => {
		/** setup.ts sets __TAURI_INTERNALS__ globally so every other suite in this
		 *  file defaults to Tauri mode; these tests flip to browser mode. */
		function setTauriMode(enabled: boolean) {
			if (enabled) {
				(globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
			} else {
				delete (globalThis as Record<string, unknown>).__TAURI_INTERNALS__;
			}
		}

		beforeEach(() => setTauriMode(false));
		afterEach(() => setTauriMode(true));

		it("copySelection calls the Clipboard API synchronously, deferring the HTTP round-trip's resolution", async () => {
			// http-copy: over a real network (Tailscale/remote, not just localhost),
			// awaiting terminal_get_selection_text before calling into the Clipboard API
			// can outlast the browser's user-activation window, silently breaking both
			// navigator.clipboard.writeText and the execCommand('copy') fallback — see
			// writeClipboardAsync's doc comment in utils/clipboard.ts. The fix: call
			// navigator.clipboard.write() synchronously with a ClipboardItem whose data
			// is the still-pending round-trip promise, so nothing awaits between the
			// triggering gesture and the Clipboard API call itself.
			const originalClipboardItem = globalThis.ClipboardItem;
			// biome-ignore lint/suspicious/noExplicitAny: test double for a DOM constructor
			(globalThis as any).ClipboardItem = FakeClipboardItem;
			const writeSpy = vi.spyOn(navigator.clipboard, "write").mockResolvedValue(undefined);
			let resolveInvoke!: (v: string) => void;
			const pending = new Promise<string>((resolve) => {
				resolveInvoke = resolve;
			});
			fakeTransport.current!.setInvokeHandler("terminal_get_selection_text", () => pending);

			try {
				const mounted = await mountCanvasTerminal({ sessionId: "clip4", terminalId: "tclip4" });
				fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

				doubleClick(mounted.canvas, 5, 0); // selects "bar", mouseup fires copySelection()

				// The Clipboard API call happens before the round-trip resolves — proving
				// it didn't wait for it.
				await waitFor(() => expect(writeSpy).toHaveBeenCalledTimes(1));
				const item = writeSpy.mock.calls[0][0][0] as unknown as FakeClipboardItem;
				expect(item.types).toEqual(["text/plain"]);

				resolveInvoke("bar-from-rust");
				const blob = await item.init["text/plain"];
				await expect(blob.text()).resolves.toBe("bar-from-rust");

				await waitFor(() => {
					expect(setStatusInfo).toHaveBeenCalledWith("Copied to clipboard");
				});
				await mounted.dispose();
			} finally {
				globalThis.ClipboardItem = originalClipboardItem;
			}
		});

		it("copySelection falls back to the synchronous path when ClipboardItem/write is unavailable", async () => {
			const originalClipboardItem = globalThis.ClipboardItem;
			// @ts-expect-error simulating a browser without ClipboardItem support
			delete globalThis.ClipboardItem;
			const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue(undefined);

			try {
				const mounted = await mountCanvasTerminal({ sessionId: "clip5", terminalId: "tclip5" });
				fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

				doubleClick(mounted.canvas, 5, 0); // selects "bar", mouseup fires copySelection()

				await waitFor(() => {
					expect(setStatusInfo).toHaveBeenCalledWith("Copied to clipboard");
				});
				expect(writeText).toHaveBeenCalledWith("bar");
				await mounted.dispose();
			} finally {
				globalThis.ClipboardItem = originalClipboardItem;
			}
		});
	});
});
