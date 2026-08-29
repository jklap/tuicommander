/**
 * Phase 4/5 of the Smart Selection plan: mount-level coverage for the two
 * gestures the pin suite (`canvasTerminalGestures.pin.test.ts`) doesn't
 * exercise — quad-click (always runs smart selection, regardless of
 * `doubleClickAction`) and Alt/Option+double-click (runs a matched rule's
 * default action). Double-click's own smart-vs-word behavior is covered by
 * the pin suite's rewritten URL test.
 */

import { fireEvent, waitFor, within } from "@solidjs/testing-library";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../../../__tests__/mocks/tauri";
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

async function selectionText(ref: { getSelectionText: () => string }, expected: string) {
	await waitFor(() => expect(ref.getSelectionText()).toBe(expected));
}

describe("CanvasTerminal smart-selection gestures (Phase 4/5)", () => {
	let restoreEnv: () => void;

	beforeEach(() => {
		restoreEnv = stubCanvasEnvironment();
		fakeTransport.current = createFakeTransport();
	});

	afterEach(() => {
		restoreEnv();
		settingsStore.setDoubleClickAction("smart");
	});

	it("quad-click runs smart selection even when doubleClickAction is 'word'", async () => {
		settingsStore.setDoubleClickAction("word");
		const mounted = await mountCanvasTerminal({ sessionId: "q1", terminalId: "tq1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["open https://example.com/a now"], 40));

		const point = cellPoint(7, 0); // inside "https"
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });

		// Word mode alone would give "https"; quad-click's forced smart pass
		// finds the built-in iterm-http-url rule instead.
		await selectionText(mounted.ref, "https://example.com/a");
		await mounted.dispose();
	});

	it("quad-click falls back to whole-line selection when no rule matches", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "q2", terminalId: "tq2" });
		// A row of only symbols no default rule's regex matches at the click
		// offset (the `\S+` word rule still would — disable smart selection
		// entirely to prove the "no match" line-select fallback works).
		settingsStore.setSmartSelectionEnabled(false);
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 80));

		const point = cellPoint(5, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });

		await selectionText(mounted.ref, "foo bar baz");
		settingsStore.setSmartSelectionEnabled(true);
		await mounted.dispose();
	});

	it("Alt+double-click runs the matched rule's default action (iterm-http-url's Open)", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "a1", terminalId: "ta1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["open https://example.com/a now"], 40));

		const point = cellPoint(7, 0); // inside "https"
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, altKey: true, ...point });

		await waitFor(() => expect(tauriOpenUrl).toHaveBeenCalledWith("https://example.com/a"));
		// The default action ran IN ADDITION to selecting the match — the
		// selection itself isn't suppressed by Alt.
		await selectionText(mounted.ref, "https://example.com/a");
		await mounted.dispose();
	});

	it("plain double-click (no Alt) selects the match but does NOT run its default action", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "a2", terminalId: "ta2" });
		fakeTransport.current!.pushFrame(buildTextFrame(["open https://example.com/a now"], 40));
		vi.mocked(tauriOpenUrl).mockClear();

		const point = cellPoint(7, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });

		await selectionText(mounted.ref, "https://example.com/a");
		expect(tauriOpenUrl).not.toHaveBeenCalled();
		await mounted.dispose();
	});

	it("Alt+double-click on a rule with no default action just selects, running nothing", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "a3", terminalId: "ta3" });
		vi.mocked(tauriOpenUrl).mockClear();
		// iterm-word (no actions) is the only rule that can match inside a
		// plain word with smart selection otherwise limited to it.
		settingsStore.setSmartSelectionRules([
			{ id: "plain", name: "word", regex: "\\S+", precision: "normal", enabled: true, actions: [] },
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		const point = cellPoint(5, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, altKey: true, ...point });

		await selectionText(mounted.ref, "bar");
		expect(tauriOpenUrl).not.toHaveBeenCalled();
		settingsStore.setSmartSelectionRules([]);
		await mounted.dispose();
	});

	it("right-clicking a smart-match-only span (no conventional link) surfaces the rule's actions in the menu", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m1", terminalId: "tm1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["fix: handle EOF a1b2c3d today"], 40));

		const point = cellPoint(20, 0); // inside "a1b2c3d" — the built-in git-sha rule
		fireEvent.contextMenu(mounted.canvas, point);

		await waitFor(() => expect(within(mounted.container).getByText("Show commit")).toBeTruthy());
		expect(within(mounted.container).getByText("Copy SHA")).toBeTruthy();
		// No conventional link was involved — the Open/Copy-link pair must not appear.
		expect(within(mounted.container).queryByText("Copy link")).toBeNull();
		await mounted.dispose();
	});

	it("the smart-match menu is headed by the matched rule's name, so it's clear which rule fired", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m3", terminalId: "tm3" });
		fakeTransport.current!.pushFrame(buildTextFrame(["fix: handle EOF a1b2c3d today"], 40));

		const point = cellPoint(20, 0);
		fireEvent.contextMenu(mounted.canvas, point);

		const header = await waitFor(() => within(mounted.container).getByText("Git commit SHA"));
		// The header renders as a non-interactive label, not a clickable action.
		expect(header.tagName).not.toBe("BUTTON");
		expect(header.classList.contains("header")).toBe(true);
		await mounted.dispose();
	});

	it("a rule with a blank name shows its actions with no header row", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m4", terminalId: "tm4" });
		settingsStore.setSmartSelectionRules([
			{
				id: "blank-name",
				name: "",
				regex: "\\S+",
				precision: "normal",
				enabled: true,
				actions: [{ kind: "copy", title: "Copy word", parameter: "\\0", isDefault: false }],
			},
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		const point = cellPoint(5, 0);
		fireEvent.contextMenu(mounted.canvas, point);

		await waitFor(() => expect(within(mounted.container).getByText("Copy word")).toBeTruthy());
		expect(mounted.container.querySelectorAll('[class*="header"]').length).toBe(0);
		settingsStore.setSmartSelectionRules([]);
		await mounted.dispose();
	});

	it("a rule with a whitespace-only name also shows no header row (not just an exact empty string)", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m4b", terminalId: "tm4b" });
		settingsStore.setSmartSelectionRules([
			{
				id: "whitespace-name",
				name: "   ",
				regex: "\\S+",
				precision: "normal",
				enabled: true,
				actions: [{ kind: "copy", title: "Copy word", parameter: "\\0", isDefault: false }],
			},
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		const point = cellPoint(5, 0);
		fireEvent.contextMenu(mounted.canvas, point);

		await waitFor(() => expect(within(mounted.container).getByText("Copy word")).toBeTruthy());
		expect(mounted.container.querySelectorAll('[class*="header"]').length).toBe(0);
		settingsStore.setSmartSelectionRules([]);
		await mounted.dispose();
	});

	it("link-detection wins over a smart match on the same span: no rule header, no rule actions, just Open/Copy link", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m5", terminalId: "tm5" });
		// A bare https:// URL is both a detected link AND matches the built-in
		// iterm-http-url smart-selection rule — link detection must win.
		const rowText = "open https://example.com/a now";
		fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => rowText);
		fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
		fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));

		const point = cellPoint(7, 0); // inside "https"
		fireEvent.contextMenu(mounted.canvas, point);

		await waitFor(() => expect(within(mounted.container).getByText("Open")).toBeTruthy());
		expect(within(mounted.container).getByText("Copy link")).toBeTruthy();
		// The rule's own name/actions (e.g. "HTTP URL") must not also appear.
		expect(within(mounted.container).queryByText("HTTP URL")).toBeNull();
		await mounted.dispose();
	});

	it('clicking "Open" in the link-detection menu opens the URL', async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m6", terminalId: "tm6" });
		const rowText = "open https://example.com/a now";
		fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => rowText);
		fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
		fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));
		vi.mocked(tauriOpenUrl).mockClear();

		fireEvent.contextMenu(mounted.canvas, cellPoint(7, 0));
		await waitFor(() => expect(within(mounted.container).getByText("Open")).toBeTruthy());
		fireEvent.click(within(mounted.container).getByText("Open"));

		await waitFor(() => expect(tauriOpenUrl).toHaveBeenCalledWith("https://example.com/a"));
		await mounted.dispose();
	});

	it('clicking "Copy link" in the link-detection menu copies the URL', async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m7", terminalId: "tm7" });
		const rowText = "open https://example.com/a now";
		fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => rowText);
		fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
		fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));

		fireEvent.contextMenu(mounted.canvas, cellPoint(7, 0));
		await waitFor(() => expect(within(mounted.container).getByText("Copy link")).toBeTruthy());
		mockInvoke.mockClear();
		fireEvent.click(within(mounted.container).getByText("Copy link"));

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("plugin:clipboard-manager|write_text", {
				text: "https://example.com/a",
				label: undefined,
			}),
		);
		await mounted.dispose();
	});

	it("clicking a smart-match action in the context menu dispatches it", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m2", terminalId: "tm2" });
		fakeTransport.current!.pushFrame(buildTextFrame(["fix: handle EOF a1b2c3d today"], 40));

		const point = cellPoint(20, 0);
		fireEvent.contextMenu(mounted.canvas, point);
		await waitFor(() => expect(within(mounted.container).getByText("Copy SHA")).toBeTruthy());

		mockInvoke.mockClear();
		fireEvent.click(within(mounted.container).getByText("Copy SHA"));

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("plugin:clipboard-manager|write_text", {
				text: "a1b2c3d",
				label: undefined,
			}),
		);
		await mounted.dispose();
	});
});
