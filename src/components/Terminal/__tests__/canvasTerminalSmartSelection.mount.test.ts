/**
 * Phase 4/5 of the Smart Selection plan: mount-level coverage for the two
 * gestures the pin suite (`canvasTerminalGestures.pin.test.ts`) doesn't
 * exercise — quad-click (always runs smart selection, regardless of
 * `doubleClickAction`) and Alt/Option+double-click (runs a matched rule's
 * default action). Double-click's own smart-vs-word behavior is covered by
 * the pin suite's rewritten URL test.
 */

import { fireEvent, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockInvoke } from "../../../__tests__/mocks/tauri";
import { settingsStore } from "../../../stores/settings";
import { toastsStore } from "../../../stores/toasts";
import { uiStore } from "../../../stores/ui";
import { handleOpenUrl as openUrlSpy } from "../../../utils/openUrl";
import { GUTTER_PX } from "../canvasTerminalUtils";
import { buildTextFrame } from "./helpers/frameFixture";
import {
	createFakeTransport,
	FIXED_CELL_METRICS,
	mountCanvasTerminal,
	stubCanvasEnvironment,
} from "./helpers/mountCanvasTerminal";

const fakeTransport = vi.hoisted(() => ({ current: null as ReturnType<typeof createFakeTransport> | null }));

// URLs open through utils/openUrl's handleOpenUrl (Tauri `open_default_app` or window.open),
// so the spy sits on that seam rather than on the opener plugin.
vi.mock("../../../utils/openUrl", async (importOriginal) => ({
	...(await importOriginal<typeof import("../../../utils/openUrl")>()),
	handleOpenUrl: vi.fn(),
}));
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
		// There is no master on/off switch for smart selection — the built-in
		// `\S+` word rule would otherwise always match. Supply an explicit user
		// rule set with nothing that can match "foo bar baz" at any offset, to
		// exercise the "no match" line-select fallback.
		settingsStore.setSmartSelectionRules([
			{ id: "never-matches", name: "never", regex: "ZZZNEVERMATCHES", precision: "normal", enabled: true, actions: [] },
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 80));

		const point = cellPoint(5, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });

		await selectionText(mounted.ref, "foo bar baz");
		settingsStore.setSmartSelectionRules([]);
		await mounted.dispose();
	});

	it("Alt+double-click runs the matched rule's default action (iterm-http-url's Open)", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "a1", terminalId: "ta1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["open https://example.com/a now"], 40));

		const point = cellPoint(7, 0); // inside "https"
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, altKey: true, ...point });

		await waitFor(() => expect(openUrlSpy).toHaveBeenCalledWith("https://example.com/a"));
		// The default action ran IN ADDITION to selecting the match — the
		// selection itself isn't suppressed by Alt.
		await selectionText(mounted.ref, "https://example.com/a");
		await mounted.dispose();
	});

	it("Alt+double-click on a relative Path match resolves it on the backend before opening (not '/e2e/...')", async () => {
		const onOpenFilePath = vi.fn();
		const mounted = await mountCanvasTerminal({ sessionId: "a4", terminalId: "ta4", onOpenFilePath });
		fakeTransport.current!.setInvokeHandler("resolve_terminal_path", () => ({
			absolute_path: "/repo/e2e/fixtures/a.mp4",
			is_directory: false,
		}));
		fakeTransport.current!.pushFrame(buildTextFrame(["see e2e/fixtures/a.mp4 now"], 40));

		const point = cellPoint(8, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, altKey: true, ...point });

		await waitFor(() => expect(onOpenFilePath).toHaveBeenCalledWith("/repo/e2e/fixtures/a.mp4", undefined, undefined));
		expect(fakeTransport.current!.invokeCalls).toContainEqual({
			cmd: "resolve_terminal_path",
			args: expect.objectContaining({ candidate: "e2e/fixtures/a.mp4" }),
		});
		await mounted.dispose();
	});

	it("a file:line:col match resolves the bare path and passes line/col to the opener", async () => {
		const onOpenFilePath = vi.fn();
		const mounted = await mountCanvasTerminal({ sessionId: "a5", terminalId: "ta5", onOpenFilePath });
		fakeTransport.current!.setInvokeHandler("resolve_terminal_path", () => ({
			absolute_path: "/repo/src/a.ts",
			is_directory: false,
		}));
		fakeTransport.current!.pushFrame(buildTextFrame(["error at src/a.ts:12:3 here"], 40));

		const point = cellPoint(12, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, altKey: true, ...point });

		await waitFor(() => expect(onOpenFilePath).toHaveBeenCalledWith("/repo/src/a.ts", 12, 3));
		expect(fakeTransport.current!.invokeCalls).toContainEqual({
			cmd: "resolve_terminal_path",
			args: expect.objectContaining({ candidate: "src/a.ts" }),
		});
		await mounted.dispose();
	});

	it("a Path match that resolves to a directory opens the file browser, not a file tab", async () => {
		const onOpenFilePath = vi.fn();
		const mounted = await mountCanvasTerminal({ sessionId: "a6", terminalId: "ta6", onOpenFilePath });
		uiStore.setFileBrowserExternalRoot(null);
		fakeTransport.current!.setInvokeHandler("resolve_terminal_path", () => ({
			absolute_path: "/repo/e2e/fixtures",
			is_directory: true,
		}));
		fakeTransport.current!.pushFrame(buildTextFrame(["see e2e/fixtures/ now"], 40));

		const point = cellPoint(8, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, altKey: true, ...point });

		await waitFor(() => expect(uiStore.state.fileBrowserExternalRoot).toBe("/repo/e2e/fixtures"));
		expect(onOpenFilePath).not.toHaveBeenCalled();
		uiStore.setFileBrowserExternalRoot(null);
		await mounted.dispose();
	});

	it("a Path match that does not exist opens nothing and tells the user", async () => {
		const onOpenFilePath = vi.fn();
		// Stubbed: the real add() arms an auto-dismiss timer that happy-dom reports as a leak.
		const addToast = vi.spyOn(toastsStore, "add").mockReturnValue("" as never);
		const mounted = await mountCanvasTerminal({ sessionId: "a7", terminalId: "ta7", onOpenFilePath });
		fakeTransport.current!.setInvokeHandler("resolve_terminal_path", () => null);
		fakeTransport.current!.pushFrame(buildTextFrame(["see no/such/file.mp4 now"], 40));

		const point = cellPoint(8, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, altKey: true, ...point });

		await waitFor(() => expect(addToast).toHaveBeenCalledWith("Open path", "Path not found: no/such/file.mp4", "warn"));
		expect(onOpenFilePath).not.toHaveBeenCalled();
		addToast.mockRestore();
		await mounted.dispose();
	});

	it("plain double-click (no Alt) selects the match but does NOT run its default action", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "a2", terminalId: "ta2" });
		fakeTransport.current!.pushFrame(buildTextFrame(["open https://example.com/a now"], 40));
		vi.mocked(openUrlSpy).mockClear();

		const point = cellPoint(7, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });

		await selectionText(mounted.ref, "https://example.com/a");
		expect(openUrlSpy).not.toHaveBeenCalled();
		await mounted.dispose();
	});

	it("Alt+double-click on a rule with no default action just selects, running nothing", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "a3", terminalId: "ta3" });
		vi.mocked(openUrlSpy).mockClear();
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
		expect(openUrlSpy).not.toHaveBeenCalled();
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
		vi.mocked(openUrlSpy).mockClear();

		fireEvent.contextMenu(mounted.canvas, cellPoint(7, 0));
		await waitFor(() => expect(within(mounted.container).getByText("Open")).toBeTruthy());
		fireEvent.click(within(mounted.container).getByText("Open"));

		await waitFor(() => expect(openUrlSpy).toHaveBeenCalledWith("https://example.com/a"));
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
			expect(mockInvoke).toHaveBeenCalledWith("write_clipboard_text", {
				text: "https://example.com/a",
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
			expect(mockInvoke).toHaveBeenCalledWith("write_clipboard_text", {
				text: "a1b2c3d",
			}),
		);
		await mounted.dispose();
	});

	describe("rule highlight while the link modifier is held", () => {
		const ACCENT = "#59a8dd"; // CanvasTerminal's fallback when the theme defines no --accent
		const sha = {
			id: "sha",
			name: "SHA",
			regex: "[0-9a-f]{7,40}",
			precision: "high" as const,
			enabled: true,
			actions: [],
			highlightOnModifier: true,
		};
		const issue = { ...sha, id: "issue", name: "Issue", regex: "#\\d+", highlightOnModifier: false };

		type Draw = { op: string; x: number; y: number; w?: number; h?: number; fill?: string; alpha?: number };
		let draws: Draw[] = [];
		let restoreContext: () => void;

		beforeEach(() => {
			draws = [];
			const base = HTMLCanvasElement.prototype.getContext;
			HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, type: string) {
				const ctx = (base as (t: string) => CanvasRenderingContext2D | null).call(this, type);
				if (ctx) {
					ctx.fillRect = ((x: number, y: number, w: number, h: number) => {
						draws.push({ op: "fillRect", x, y, w, h, fill: String(ctx.fillStyle), alpha: ctx.globalAlpha });
					}) as CanvasRenderingContext2D["fillRect"];
					ctx.moveTo = ((x: number, y: number) =>
						draws.push({ op: "moveTo", x, y })) as CanvasRenderingContext2D["moveTo"];
					ctx.lineTo = ((x: number, y: number) =>
						draws.push({ op: "lineTo", x, y })) as CanvasRenderingContext2D["lineTo"];
				}
				return ctx;
			} as typeof HTMLCanvasElement.prototype.getContext;
			restoreContext = () => {
				HTMLCanvasElement.prototype.getContext = base;
			};
		});

		afterEach(() => {
			restoreContext();
			// The held-modifier signal is module-level: release it so no later test starts with Cmd down.
			fireEvent.keyUp(document, { key: "Meta", metaKey: false, ctrlKey: false });
			settingsStore.setSmartSelectionRules([]);
		});

		const hold = () => fireEvent.keyDown(document, { key: "Meta", metaKey: true, ctrlKey: true });
		const tints = () => draws.filter((d) => d.op === "fillRect" && d.fill === ACCENT && d.alpha === 0.18);
		const bars = () => draws.filter((d) => d.op === "fillRect" && d.fill === ACCENT && d.alpha === 0.95);

		it("highlights an opted-in rule's match, in the accent style, only while the modifier is held", async () => {
			settingsStore.setSmartSelectionRules([sha]);
			const mounted = await mountCanvasTerminal({ sessionId: "h1", terminalId: "th1" });
			fakeTransport.current!.pushFrame(buildTextFrame(["fix: handle EOF a1b2c3d today"], 40));

			// Catches: highlighting painted with no modifier held.
			await new Promise((r) => setTimeout(r, 30));
			expect(tints()).toEqual([]);

			hold();
			await waitFor(() => expect(tints().length).toBeGreaterThan(0));
			const cw = FIXED_CELL_METRICS.cellWidth;
			// "a1b2c3d" is 7 cells wide, starting at column 16; the bar is the thick underline.
			expect(tints()[0].w).toBeCloseTo(7 * cw);
			expect(tints()[0].x).toBeCloseTo(16 * cw);
			expect(bars()[0].w).toBeCloseTo(7 * cw);
			expect(bars()[0].h).toBe(2);
			await mounted.dispose();
		});

		// Catches: every rule highlighting once the modifier is down.
		it("does not highlight a rule that did not opt in", async () => {
			settingsStore.setSmartSelectionRules([sha, issue]);
			const mounted = await mountCanvasTerminal({ sessionId: "h2", terminalId: "th2" });
			fakeTransport.current!.pushFrame(buildTextFrame(["see #123 and a1b2c3d"], 40));

			hold();
			await waitFor(() => expect(tints().length).toBeGreaterThan(0));
			expect(tints()).toHaveLength(1); // the SHA only; "#123" matches `issue`, which is off
			expect(tints()[0].w).toBeCloseTo(7 * FIXED_CELL_METRICS.cellWidth);
			await mounted.dispose();
		});

		// Catches: the link underline still drawn under text the rule highlights (two visuals on one string).
		it("draws the rule's style, not the link underline, where a link and a rule cover the same text", async () => {
			settingsStore.setSmartSelectionRules([sha]);
			const mounted = await mountCanvasTerminal({ sessionId: "h3", terminalId: "th3" });
			// The URL ends in a SHA: cols 4-31 are the link, cols 24-31 are also the rule's match.
			fakeTransport.current!.pushFrame(buildTextFrame(["see https://example.com/a1b2c3d end"], 40));
			const cw = FIXED_CELL_METRICS.cellWidth;
			const dashedEnds = () => draws.filter((d) => d.op === "lineTo").map((d) => Math.round(d.x / cw));

			// Link underlines are on by default ("click" mode), so the whole URL is underlined first.
			await waitFor(() => expect(dashedEnds()).toContain(31));

			draws = [];
			hold();
			await waitFor(() => expect(tints().length).toBeGreaterThan(0));
			// Still underlined up to where the rule's match starts (col 24), but no longer under the SHA.
			await waitFor(() => expect(dashedEnds()).toContain(24));
			expect(dashedEnds()).not.toContain(31);
			expect(tints()[0].x).toBeCloseTo(24 * cw);
			await mounted.dispose();
		});
	});
});
