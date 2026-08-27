/**
 * Failure-path coverage for CanvasTerminal's clipboard writes (copySelection,
 * copyLink, and the smart-selection "copy" action) — these already had
 * correct catch/status handling, they just had no test proving it.
 */

import { fireEvent, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
		delete (window as unknown as Record<string, unknown>).__tuic_setStatusInfo;
		vi.restoreAllMocks();
	});

	it("copySelection surfaces a failure status and logs instead of claiming success", async () => {
		mockInvoke.mockRejectedValueOnce(new DOMException("Write permission denied.", "NotAllowedError"));
		const mounted = await mountCanvasTerminal({ sessionId: "clip1", terminalId: "tclip1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		doubleClick(mounted.canvas, 5, 0); // selects "bar", mouseup fires copySelection()

		await waitFor(() => {
			expect(setStatusInfo).toHaveBeenCalledWith("Copy failed — clipboard unavailable");
		});
		expect(setStatusInfo).not.toHaveBeenCalledWith("Copied to clipboard");
		expect(appLogger.warn).toHaveBeenCalledWith("terminal", "Clipboard write failed", expect.anything());
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
		mockInvoke.mockRejectedValueOnce(new DOMException("Write permission denied.", "NotAllowedError"));
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
});
