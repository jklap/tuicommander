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
import { shVar } from "../../../__tests__/helpers/shellVar";
import { mockInvoke } from "../../../__tests__/mocks/tauri";
import { settingsStore } from "../../../stores/settings";
import { terminalsStore } from "../../../stores/terminals";
import { toastsStore } from "../../../stores/toasts";
import { handleOpenUrl } from "../../../utils/openUrl";
import { GUTTER_PX } from "../canvasTerminalUtils";
import { DEFAULT_SMART_SELECTION_RULES } from "../smartSelectionDefaults";
import type { SmartSelectionRule } from "../smartSelectionTypes";
import { buildTextFrame } from "./helpers/frameFixture";
import {
	createFakeTransport,
	FIXED_CELL_METRICS,
	mountCanvasTerminal,
	stubCanvasEnvironment,
} from "./helpers/mountCanvasTerminal";

const fakeTransport = vi.hoisted(() => ({ current: null as ReturnType<typeof createFakeTransport> | null }));

// URL opens route through the shared opener (main sends them to the backend's
// open_default_app command), so assert at that seam rather than a Tauri plugin.
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

/** Contexts the terminal created, so a test can read what the overlay drew. */
const drawnContexts: CanvasRenderingContext2D[] = [];

/** Widths (in cells) of every underline the overlay stroked on `row`, in cell units. */
function underlineWidthsOnRow(row: number): number[] {
	const y = row * FIXED_CELL_METRICS.cellHeight + FIXED_CELL_METRICS.cellHeight - 1 + 0.5;
	const widths: number[] = [];
	for (const ctx of drawnContexts) {
		const moves = vi.mocked(ctx.moveTo).mock.calls;
		const lines = vi.mocked(ctx.lineTo).mock.calls;
		for (let i = 0; i < Math.min(moves.length, lines.length); i++) {
			if (moves[i][1] === y && lines[i][1] === y) {
				widths.push((lines[i][0] - moves[i][0]) / FIXED_CELL_METRICS.cellWidth);
			}
		}
	}
	return widths;
}

/** Wrap the (already stubbed) getContext so every context the terminal creates is kept.
 *  `stubCanvasEnvironment`'s restore function puts the original back, so no cleanup is needed. */
function recordCanvasContexts() {
	drawnContexts.length = 0;
	const stubbed = HTMLCanvasElement.prototype.getContext;
	HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, type: string) {
		const ctx = (stubbed as (t: string) => CanvasRenderingContext2D | null).call(this, type);
		if (ctx) drawnContexts.push(ctx);
		return ctx;
	} as typeof HTMLCanvasElement.prototype.getContext;
}

const userRule = (overrides: Partial<SmartSelectionRule> = {}): SmartSelectionRule => ({
	id: "sel-user",
	name: "User rule",
	regex: "\\S+",
	precision: "high",
	enabled: true,
	actions: [{ kind: "copy", title: "Copy it", parameter: "\\0", isDefault: false }],
	...overrides,
});

const builtIn = (id: string): SmartSelectionRule => {
	const found = DEFAULT_SMART_SELECTION_RULES.find((r) => r.id === id);
	if (!found) throw new Error(`no built-in rule ${id}`);
	return structuredClone(found);
};

describe("CanvasTerminal smart-selection gestures (Phase 4/5)", () => {
	let restoreEnv: () => void;

	beforeEach(() => {
		restoreEnv = stubCanvasEnvironment();
		recordCanvasContexts();
		fakeTransport.current = createFakeTransport();
	});

	afterEach(() => {
		restoreEnv();
		settingsStore.setDoubleClickAction("smart");
		// Tests below set rules and only reset them on success; never let a failed assertion leak them.
		settingsStore.setSmartSelectionRules([]);
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

		await waitFor(() => expect(handleOpenUrl).toHaveBeenCalledWith("https://example.com/a"));
		// The default action ran IN ADDITION to selecting the match — the
		// selection itself isn't suppressed by Alt.
		await selectionText(mounted.ref, "https://example.com/a");
		await mounted.dispose();
	});

	it("plain double-click (no Alt) selects the match but does NOT run its default action", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "a2", terminalId: "ta2" });
		fakeTransport.current!.pushFrame(buildTextFrame(["open https://example.com/a now"], 40));
		vi.mocked(handleOpenUrl).mockClear();

		const point = cellPoint(7, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });

		await selectionText(mounted.ref, "https://example.com/a");
		expect(handleOpenUrl).not.toHaveBeenCalled();
		await mounted.dispose();
	});

	it("Alt+double-click runs only the BEST rule's default, even though the menu would list every matching rule", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "a-best", terminalId: "ta-best" });
		settingsStore.setSmartSelectionRules([
			userRule({
				id: "low",
				name: "Any word",
				precision: "low",
				actions: [{ kind: "copy", title: "Copy word", parameter: "\\0", isDefault: true }],
			}),
			userRule({
				id: "md",
				name: "Markdown",
				regex: "[A-Za-z0-9._-]+\\.md",
				actions: [{ kind: "copy", title: "Copy md", parameter: "\\0", isDefault: true }],
			}),
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["see (README.md) now"], 40));
		mockInvoke.mockClear();

		const point = cellPoint(8, 0); // inside README.md
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, altKey: true, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, altKey: true, ...point });

		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("write_clipboard_text", { text: "README.md" }));
		const copies = mockInvoke.mock.calls.filter((c) => c[0] === "write_clipboard_text");
		expect(copies).toHaveLength(1);
		settingsStore.setSmartSelectionRules([]);
		await mounted.dispose();
	});

	it("Alt+double-click on a rule with no default action just selects, running nothing", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "a3", terminalId: "ta3" });
		vi.mocked(handleOpenUrl).mockClear();
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
		expect(handleOpenUrl).not.toHaveBeenCalled();
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

	it("a detected link no longer hides rule matches: the menu lists the rule's group AND a separate Link group", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m5", terminalId: "tm5" });
		// A bare https:// URL is both a detected link AND matches the built-in
		// iterm-http-url smart-selection rule — the menu shows both.
		const rowText = "open https://example.com/a now";
		fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => rowText);
		fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
		fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));

		const point = cellPoint(7, 0); // inside "https"
		fireEvent.contextMenu(mounted.canvas, point);

		await waitFor(() => expect(within(mounted.container).getByText("Link")).toBeTruthy());
		expect(within(mounted.container).getByText("Copy link")).toBeTruthy();
		// The rule's own header and actions appear too, as their own group.
		expect(within(mounted.container).getByText("HTTP URL")).toBeTruthy();
		const headers = [...mounted.container.querySelectorAll('[class*="header"]')].map((el) => el.textContent);
		expect(headers.indexOf("HTTP URL")).toBeGreaterThanOrEqual(0);
		// The link group comes after every rule group.
		expect(headers.indexOf("Link")).toBe(headers.length - 1);
		await mounted.dispose();
	});

	it("lists EVERY matching rule as its own group, best first, separated by dividers", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m8", terminalId: "tm8" });
		settingsStore.setSmartSelectionRules([
			{
				id: "low-word",
				name: "Any word",
				regex: "\\S+",
				precision: "low",
				enabled: true,
				actions: [{ kind: "copy", title: "Copy word", parameter: "\\0", isDefault: false }],
			},
			{
				id: "md-file",
				name: "Markdown files",
				regex: "~?/?([A-Za-z0-9._-]+/+)*[A-Za-z0-9._-]+\\.md",
				precision: "high",
				enabled: true,
				actions: [{ kind: "run_command", title: "Open in Imark", parameter: "imark \\0", isDefault: true }],
			},
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["see README.md now"], 40));

		fireEvent.contextMenu(mounted.canvas, cellPoint(6, 0)); // inside "README.md"

		await waitFor(() => expect(within(mounted.container).getByText("Open in Imark")).toBeTruthy());
		expect(within(mounted.container).getByText("Copy word")).toBeTruthy();
		const headers = [...mounted.container.querySelectorAll('[class*="header"]')].map((el) => el.textContent);
		// Higher-precision rule's group first, the catch-all after it.
		expect(headers).toEqual(["Markdown files", "Any word"]);
		// One divider between the two groups, none before the first / after the last.
		expect(mounted.container.querySelectorAll('[class*="separator"]').length).toBe(1);
		settingsStore.setSmartSelectionRules([]);
		await mounted.dispose();
	});

	it("a rule's action in a non-winning group still dispatches (the menu is not limited to the best rule)", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m9", terminalId: "tm9" });
		settingsStore.setSmartSelectionRules([
			{
				id: "low-word",
				name: "Any word",
				regex: "\\S+",
				precision: "low",
				enabled: true,
				actions: [{ kind: "copy", title: "Copy word", parameter: "\\0", isDefault: false }],
			},
			{
				id: "md-file",
				name: "Markdown files",
				regex: "[A-Za-z0-9._-]+\\.md",
				precision: "high",
				enabled: true,
				actions: [{ kind: "copy", title: "Copy md", parameter: "\\0", isDefault: false }],
			},
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["see README.md now"], 40));
		fireEvent.contextMenu(mounted.canvas, cellPoint(6, 0));
		await waitFor(() => expect(within(mounted.container).getByText("Copy word")).toBeTruthy());

		mockInvoke.mockClear();
		fireEvent.click(within(mounted.container).getByText("Copy word"));

		await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("write_clipboard_text", { text: "README.md" }));
		settingsStore.setSmartSelectionRules([]);
		await mounted.dispose();
	});

	it('clicking "Open" in the link-detection menu opens the URL', async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m6", terminalId: "tm6" });
		const rowText = "open https://example.com/a now";
		fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => rowText);
		fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
		fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));
		vi.mocked(handleOpenUrl).mockClear();

		fireEvent.contextMenu(mounted.canvas, cellPoint(7, 0));
		await waitFor(() => expect(within(mounted.container).getAllByText("Open").length).toBeGreaterThan(0));
		// The HTTP URL rule group has its own "Open"; the link group's comes last.
		fireEvent.click(within(mounted.container).getAllByText("Open").at(-1)!);

		await waitFor(() => expect(handleOpenUrl).toHaveBeenCalledWith("https://example.com/a"));
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

	it("hides built-in Copy-only groups when a more specific rule matched (default rule set + user rule)", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m10", terminalId: "tm10" });
		// Saved rules replace the defaults wholesale, so seed the shipped set and add the user's rule.
		settingsStore.setSmartSelectionRules([
			...DEFAULT_SMART_SELECTION_RULES,
			{
				id: "sel-md",
				name: "Markdown files",
				regex: "[A-Za-z0-9._-]+\\.md",
				precision: "high",
				enabled: true,
				actions: [{ kind: "run_command", title: "Open in Imark", parameter: "imark \\0", isDefault: true }],
			},
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["see README.md now"], 40));

		fireEvent.contextMenu(mounted.canvas, cellPoint(6, 0));

		await waitFor(() => expect(within(mounted.container).getByText("Open in Imark")).toBeTruthy());
		const headers = [...mounted.container.querySelectorAll('[class*="header"]')].map((el) => el.textContent);
		expect(headers).toEqual(["Markdown files"]);
		expect(within(mounted.container).queryByText("Word bounded by whitespace")).toBeNull();
		settingsStore.setSmartSelectionRules([]);
		await mounted.dispose();
	});

	it("keeps just the best built-in Copy group when nothing more specific matched", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "m11", terminalId: "tm11" });
		settingsStore.setSmartSelectionRules([]); // shipped defaults
		fakeTransport.current!.pushFrame(buildTextFrame(["see foo.bar.baz now"], 40));

		fireEvent.contextMenu(mounted.canvas, cellPoint(6, 0)); // inside "foo.bar.baz"

		await waitFor(() => expect(within(mounted.container).getAllByText("Copy").length).toBe(1));
		expect(mounted.container.querySelectorAll('[class*="header"]').length).toBe(1);
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
});

describe("CanvasTerminal link left-click and smart-selection rules", () => {
	let restoreEnv: () => void;

	beforeEach(() => {
		restoreEnv = stubCanvasEnvironment();
		fakeTransport.current = createFakeTransport();
		settingsStore.setLinkActivation("click");
		vi.mocked(handleOpenUrl).mockClear();
	});

	afterEach(() => {
		restoreEnv();
		settingsStore.setSmartSelectionRules([]);
	});

	async function leftClickLink(mounted: Awaited<ReturnType<typeof mountCanvasTerminal>>, rowText: string) {
		fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => rowText);
		fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
		fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));
		const point = cellPoint(6, 0);
		fireEvent.mouseMove(document, point);
		await new Promise((r) => setTimeout(r, 150)); // link probe is debounced
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });
		fireEvent.click(mounted.canvas, { button: 0, detail: 1, ...point });
	}

	it("runs a user rule's default action instead of opening the link", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "c1", terminalId: "tc1" });
		settingsStore.setSmartSelectionRules([
			{
				id: "sel-example",
				name: "Example URLs",
				regex: "https://example\\.com/\\S*",
				precision: "high",
				enabled: true,
				actions: [{ kind: "copy", title: "Copy it", parameter: "\\0", isDefault: true }],
			},
		]);
		mockInvoke.mockClear();

		await leftClickLink(mounted, "see https://example.com/a here");

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_clipboard_text", { text: "https://example.com/a" }),
		);
		expect(handleOpenUrl).not.toHaveBeenCalled();
		await mounted.dispose();
	});

	it("falls back to opening the link when no user rule has a default action", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "c2", terminalId: "tc2" });
		settingsStore.setSmartSelectionRules([
			{
				id: "sel-example",
				name: "Example URLs",
				regex: "https://example\\.com/\\S*",
				precision: "high",
				enabled: true,
				actions: [{ kind: "copy", title: "Copy it", parameter: "\\0", isDefault: false }],
			},
		]);
		mockInvoke.mockClear();

		await leftClickLink(mounted, "see https://example.com/a here");

		await waitFor(() => expect(handleOpenUrl).toHaveBeenCalledWith("https://example.com/a"));
		expect(mockInvoke).not.toHaveBeenCalledWith("write_clipboard_text", expect.anything());
		await mounted.dispose();
	});

	it("falls back to opening the link with only the built-in rules (their defaults are what link-open does)", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "c3", terminalId: "tc3" });
		settingsStore.setSmartSelectionRules([]); // shipped defaults

		await leftClickLink(mounted, "see https://example.com/a here");

		await waitFor(() => expect(handleOpenUrl).toHaveBeenCalledWith("https://example.com/a"));
		await mounted.dispose();
	});
});

describe("CanvasTerminal smart-selection right-click menu (groups, filtering, underline)", () => {
	let restoreEnv: () => void;

	beforeEach(() => {
		restoreEnv = stubCanvasEnvironment();
		recordCanvasContexts();
		fakeTransport.current = createFakeTransport();
	});

	afterEach(() => {
		restoreEnv();
		settingsStore.setSmartSelectionRules([]);
	});

	const headersOf = (container: HTMLElement) =>
		[...container.querySelectorAll('[class*="header"]')].map((el) => el.textContent);
	const separatorsIn = (container: HTMLElement) => container.querySelectorAll('[class*="separator"]').length;

	function primeUrlRow(rowText: string) {
		fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => rowText);
		fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
		fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));
	}

	it("right-clicking empty space past the end of the text opens no menu and leaves the default alone", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "r1", terminalId: "tr1" });
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		const notPrevented = fireEvent.contextMenu(mounted.canvas, cellPoint(35, 0));

		expect(notPrevented).toBe(true); // preventDefault was NOT called
		expect(mounted.container.querySelectorAll('[class*="header"]').length).toBe(0);
		expect(within(mounted.container).queryByText("Copy")).toBeNull();
		await mounted.dispose();
	});

	it("a matching rule with no actions contributes no group, no menu and no preventDefault", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "r2", terminalId: "tr2" });
		settingsStore.setSmartSelectionRules([userRule({ name: "Silent", actions: [] })]);
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		const notPrevented = fireEvent.contextMenu(mounted.canvas, cellPoint(1, 0));

		expect(notPrevented).toBe(true);
		expect(within(mounted.container).queryByText("Silent")).toBeNull();
		await mounted.dispose();
	});

	it("a rule with no actions is skipped but its action-bearing neighbour still gets a group", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "r3", terminalId: "tr3" });
		settingsStore.setSmartSelectionRules([
			userRule({ id: "silent", name: "Silent", actions: [], precision: "very_high" }),
			userRule({ id: "loud", name: "Loud", precision: "low" }),
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));

		fireEvent.contextMenu(mounted.canvas, cellPoint(1, 0));

		await waitFor(() => expect(within(mounted.container).getByText("Loud")).toBeTruthy());
		expect(headersOf(mounted.container)).toEqual(["Loud"]);
		await mounted.dispose();
	});

	it("a rule match and a link: one divider between the rule group and the Link group, in that order", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "r4", terminalId: "tr4" });
		settingsStore.setSmartSelectionRules([userRule({ name: "Example URLs", regex: "https://example\\.com/\\S*" })]);
		primeUrlRow("open https://example.com/a now");

		fireEvent.contextMenu(mounted.canvas, cellPoint(7, 0));

		await waitFor(() => expect(within(mounted.container).getByText("Copy link")).toBeTruthy());
		expect(headersOf(mounted.container)).toEqual(["Example URLs", "Link"]);
		expect(separatorsIn(mounted.container)).toBe(1);
		await mounted.dispose();
	});

	it("a link with only built-in Copy-only rule matches gets the plain Open / Copy link menu — no headers, no dividers", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "r5", terminalId: "tr5" });
		// Only the catch-all word rule is enabled, so the URL matches nothing more specific.
		settingsStore.setSmartSelectionRules([builtIn("iterm-word")]);
		primeUrlRow("open https://example.com/a now");

		fireEvent.contextMenu(mounted.canvas, cellPoint(7, 0));

		await waitFor(() => expect(within(mounted.container).getByText("Copy link")).toBeTruthy());
		expect(within(mounted.container).getByText("Open")).toBeTruthy();
		expect(headersOf(mounted.container)).toEqual([]);
		expect(separatorsIn(mounted.container)).toBe(0);
		await mounted.dispose();
	});

	it("a second right-click does not carry the previous menu's rule groups (matches are replaced, not accumulated)", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "r6", terminalId: "tr6" });
		settingsStore.setSmartSelectionRules([userRule({ name: "Markdown", regex: "[A-Za-z0-9._-]+\\.md" })]);
		primeUrlRow("https://example.com/a README.md");

		fireEvent.contextMenu(mounted.canvas, cellPoint(25, 0)); // README.md
		await waitFor(() => expect(within(mounted.container).getByText("Markdown")).toBeTruthy());
		fireEvent.keyDown(document, { key: "Escape" });
		await waitFor(() => expect(within(mounted.container).queryByText("Markdown")).toBeNull());

		fireEvent.contextMenu(mounted.canvas, cellPoint(4, 0)); // the URL — Markdown rule does not match it
		await waitFor(() => expect(within(mounted.container).getByText("Copy link")).toBeTruthy());
		expect(within(mounted.container).queryByText("Markdown")).toBeNull();
		expect(headersOf(mounted.container)).toEqual([]);
		await mounted.dispose();
	});

	it("underlines the span of every group the menu lists, each at its own extent", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "r7", terminalId: "tr7" });
		settingsStore.setSmartSelectionRules([
			userRule({ id: "word", name: "Any word", precision: "low" }),
			userRule({ id: "md", name: "Markdown", regex: "[A-Za-z0-9._-]+\\.md" }),
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["see (README.md) now"], 40));

		fireEvent.contextMenu(mounted.canvas, cellPoint(8, 0));
		await waitFor(() => expect(within(mounted.container).getByText("Markdown")).toBeTruthy());

		// "(README.md)" is 11 cells (Any word), "README.md" is 9 (Markdown).
		await waitFor(() => {
			const widths = underlineWidthsOnRow(0);
			expect(widths).toContain(11);
			expect(widths).toContain(9);
		});
		await mounted.dispose();
	});

	it("does not underline a built-in Copy-only group the menu hid", async () => {
		const mounted = await mountCanvasTerminal({ sessionId: "r8", terminalId: "tr8" });
		settingsStore.setSmartSelectionRules([
			...DEFAULT_SMART_SELECTION_RULES,
			userRule({ id: "md", name: "Markdown", regex: "[A-Za-z0-9._-]+\\.md" }),
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["see (README.md) now"], 40));

		fireEvent.contextMenu(mounted.canvas, cellPoint(8, 0));
		await waitFor(() => expect(within(mounted.container).getByText("Markdown")).toBeTruthy());
		expect(headersOf(mounted.container)).toEqual(["Markdown"]);

		await waitFor(() => expect(underlineWidthsOnRow(0)).toContain(9));
		// The built-in word rule ("(README.md)", 11 cells) is hidden, so it is not drawn either.
		expect(underlineWidthsOnRow(0)).not.toContain(11);
		await mounted.dispose();
	});
});

describe("CanvasTerminal link left-click: modifier mode, Alt, and non-link clicks", () => {
	let restoreEnv: () => void;

	beforeEach(() => {
		restoreEnv = stubCanvasEnvironment();
		recordCanvasContexts();
		fakeTransport.current = createFakeTransport();
		vi.mocked(handleOpenUrl).mockClear();
	});

	afterEach(() => {
		restoreEnv();
		settingsStore.setLinkActivation("click");
		settingsStore.setSmartSelectionRules([]);
		// A failed test must not leave the module-level "modifier held" flag set for the next one.
		fireEvent.keyUp(document, { key: "Meta" });
	});

	const copiesOf = () => mockInvoke.mock.calls.filter((c) => c[0] === "write_clipboard_text");

	function primeUrlRow(rowText: string) {
		fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => rowText);
		fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
		fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));
	}

	const exampleUrlRule = () =>
		userRule({
			name: "Example URLs",
			regex: "https://example\\.com/\\S*",
			actions: [{ kind: "copy", title: "Copy it", parameter: "\\0", isDefault: true }],
		});

	it("modifier mode: a plain click on a link runs neither the rule nor the link open", async () => {
		settingsStore.setLinkActivation("modifier");
		const mounted = await mountCanvasTerminal({ sessionId: "k1", terminalId: "tk1" });
		settingsStore.setSmartSelectionRules([exampleUrlRule()]);
		primeUrlRow("see https://example.com/a here");
		mockInvoke.mockClear();

		const point = cellPoint(6, 0);
		fireEvent.mouseMove(document, point);
		await new Promise((r) => setTimeout(r, 150));
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });
		fireEvent.click(mounted.canvas, { button: 0, detail: 1, ...point });
		await new Promise((r) => setTimeout(r, 50));

		expect(copiesOf()).toHaveLength(0);
		expect(handleOpenUrl).not.toHaveBeenCalled();
		await mounted.dispose();
	});

	it("modifier mode: the modifier-click runs the user rule's default action", async () => {
		settingsStore.setLinkActivation("modifier");
		const mounted = await mountCanvasTerminal({ sessionId: "k2", terminalId: "tk2" });
		settingsStore.setSmartSelectionRules([exampleUrlRule()]);
		primeUrlRow("see https://example.com/a here");
		mockInvoke.mockClear();

		// Both modifiers held so this passes whichever of Cmd/Ctrl the platform treats as the link modifier.
		const mods = { metaKey: true, ctrlKey: true };
		const point = cellPoint(6, 0);
		fireEvent.keyDown(document, { key: "Meta", ...mods });
		fireEvent.mouseMove(document, { ...point, ...mods });
		await new Promise((r) => setTimeout(r, 150));
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point, ...mods });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point, ...mods });
		fireEvent.click(mounted.canvas, { button: 0, detail: 1, ...point, ...mods });

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_clipboard_text", { text: "https://example.com/a" }),
		);
		expect(handleOpenUrl).not.toHaveBeenCalled();
		await mounted.dispose();
	});

	it("a plain click on text that is NOT a link never runs a rule's default action", async () => {
		settingsStore.setLinkActivation("click");
		const mounted = await mountCanvasTerminal({ sessionId: "k3", terminalId: "tk3" });
		settingsStore.setSmartSelectionRules([
			userRule({ actions: [{ kind: "copy", title: "Copy it", parameter: "\\0", isDefault: true }] }),
		]);
		fakeTransport.current!.pushFrame(buildTextFrame(["foo bar baz"], 40));
		mockInvoke.mockClear();

		const point = cellPoint(1, 0);
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });
		fireEvent.click(mounted.canvas, { button: 0, detail: 1, ...point });
		await new Promise((r) => setTimeout(r, 50));

		expect(copiesOf()).toHaveLength(0);
		await mounted.dispose();
	});

	it("a double-click on a link runs the rule once, not once per click", async () => {
		settingsStore.setLinkActivation("click");
		const mounted = await mountCanvasTerminal({ sessionId: "k4", terminalId: "tk4" });
		// "rule:" prefix tells the rule's copy apart from "Copy on Select" copying the double-click's selection.
		settingsStore.setSmartSelectionRules([
			userRule({
				regex: "https://example\\.com/\\S*",
				actions: [{ kind: "copy", title: "Copy it", parameter: "rule:\\0", isDefault: true }],
			}),
		]);
		primeUrlRow("see https://example.com/a here");
		const point = cellPoint(6, 0);
		fireEvent.mouseMove(document, point);
		await new Promise((r) => setTimeout(r, 150));
		mockInvoke.mockClear();

		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });
		fireEvent.click(mounted.canvas, { button: 0, detail: 1, ...point });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });
		fireEvent.click(mounted.canvas, { button: 0, detail: 2, ...point });
		await new Promise((r) => setTimeout(r, 50));

		const ruleCopies = copiesOf().filter((c) => String((c[1] as { text?: string })?.text).startsWith("rule:"));
		expect(ruleCopies).toHaveLength(1);
		await mounted.dispose();
	});

	it("Alt+double-click on a link runs the rule's default action once, with the click events a real double-click delivers", async () => {
		settingsStore.setLinkActivation("click");
		const mounted = await mountCanvasTerminal({ sessionId: "k6", terminalId: "tk6" });
		settingsStore.setSmartSelectionRules([
			userRule({
				regex: "https://example\\.com/\\S*",
				actions: [{ kind: "copy", title: "Copy it", parameter: "rule:\\0", isDefault: true }],
			}),
		]);
		primeUrlRow("see https://example.com/a here");
		const point = cellPoint(6, 0);
		const alt = { altKey: true };
		fireEvent.mouseMove(document, point);
		await new Promise((r) => setTimeout(r, 150));
		mockInvoke.mockClear();

		// A browser fires click (detail 1) after the first mouseup and BEFORE the second mousedown.
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point, ...alt });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point, ...alt });
		fireEvent.click(mounted.canvas, { button: 0, detail: 1, ...point, ...alt });
		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point, ...alt });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point, ...alt });
		fireEvent.click(mounted.canvas, { button: 0, detail: 2, ...point, ...alt });
		await new Promise((r) => setTimeout(r, 50));

		const ruleCopies = copiesOf().filter((c) => String((c[1] as { text?: string })?.text).startsWith("rule:"));
		expect(ruleCopies).toHaveLength(1);
		await mounted.dispose();
	});

	it("the best user rule wins on click when several match and have a default action", async () => {
		settingsStore.setLinkActivation("click");
		const mounted = await mountCanvasTerminal({ sessionId: "k5", terminalId: "tk5" });
		settingsStore.setSmartSelectionRules([
			userRule({
				id: "low",
				name: "Any word",
				precision: "low",
				actions: [{ kind: "copy", title: "Copy word", parameter: "word:\\0", isDefault: true }],
			}),
			userRule({
				id: "url",
				name: "Example URLs",
				regex: "https://example\\.com/\\S*",
				actions: [{ kind: "copy", title: "Copy url", parameter: "url:\\0", isDefault: true }],
			}),
		]);
		primeUrlRow("see https://example.com/a here");
		const point = cellPoint(6, 0);
		fireEvent.mouseMove(document, point);
		await new Promise((r) => setTimeout(r, 150));
		mockInvoke.mockClear();

		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });
		fireEvent.click(mounted.canvas, { button: 0, detail: 1, ...point });

		await waitFor(() =>
			expect(mockInvoke).toHaveBeenCalledWith("write_clipboard_text", { text: "url:https://example.com/a" }),
		);
		expect(copiesOf()).toHaveLength(1);
		await mounted.dispose();
	});
});

describe("CanvasTerminal Run Command (run_background) and Send Text (send_text, incl. the older run_command) actions", () => {
	let restoreEnv: () => void;
	let registeredTerminal: string | undefined;

	beforeEach(() => {
		restoreEnv = stubCanvasEnvironment();
		recordCanvasContexts();
		fakeTransport.current = createFakeTransport();
		vi.mocked(handleOpenUrl).mockClear();
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
	});

	afterEach(() => {
		restoreEnv();
		// A test may override the shared mock (e.g. to allow spawning); never leave that for the next one.
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
		settingsStore.setSmartSelectionRules([]);
		settingsStore.setLinkActivation("click");
		if (registeredTerminal) terminalsStore.remove(registeredTerminal);
		registeredTerminal = undefined;
		for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
	});

	const backgroundCalls = () => fakeTransport.current!.invokeCalls.filter((c) => c.cmd === "run_background_command");
	const typedCalls = () => fakeTransport.current!.invokeCalls.filter((c) => c.cmd === "write_pty");

	const markdownRule = (kind: "run_background" | "run_command", overrides: Partial<SmartSelectionRule> = {}) =>
		userRule({
			id: "md",
			name: "Markdown",
			regex: "\\S+\\.md",
			actions: [{ kind, title: "Open in Imark", parameter: "imark \\0", isDefault: true }],
			...overrides,
		});

	async function rightClickAndChoose(rowText: string, col: number, sessionId: string, terminalId: string) {
		const mounted = await mountCanvasTerminal({ sessionId, terminalId });
		fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));
		fireEvent.contextMenu(mounted.canvas, cellPoint(col, 0));
		await waitFor(() => expect(within(mounted.container).getByText("Open in Imark")).toBeTruthy());
		fireEvent.click(within(mounted.container).getByText("Open in Imark"));
		return mounted;
	}

	it("Run Command: choosing it from the menu runs the shell-quoted command in the background", async () => {
		settingsStore.setSmartSelectionRules([markdownRule("run_background")]);
		const mounted = await rightClickAndChoose("see README.md now", 6, "b1", "tb1");

		await waitFor(() => expect(backgroundCalls()).toHaveLength(1));
		// The match is a VALUE in env, never part of the command line the shell parses.
		expect(backgroundCalls()[0].args).toEqual({
			command: `imark "${shVar("TUIC_M0")}"`,
			cwd: null,
			env: { TUIC_M0: "README.md" },
			shell: null,
		});
		await mounted.dispose();
	});

	it("Run Command types nothing into the terminal", async () => {
		settingsStore.setSmartSelectionRules([markdownRule("run_background")]);
		const mounted = await rightClickAndChoose("see README.md now", 6, "b2", "tb2");

		await waitFor(() => expect(backgroundCalls()).toHaveLength(1));
		expect(typedCalls()).toHaveLength(0);
		await mounted.dispose();
	});

	it("Run Command: text that tries to break out of the command is passed as data and never appears in the command line", async () => {
		settingsStore.setSmartSelectionRules([markdownRule("run_background")]);
		const mounted = await rightClickAndChoose("x it's$(id);.md y", 6, "b3", "tb3");

		await waitFor(() => expect(backgroundCalls()).toHaveLength(1));
		const { command, env } = backgroundCalls()[0].args as { command: string; env: Record<string, string> };
		expect(command).toBe(`imark "${shVar("TUIC_M0")}"`);
		expect(env).toEqual({ TUIC_M0: "it's$(id);.md" });
		await mounted.dispose();
	});

	it("Run Command runs in the shell chosen in Settings → Shell, the same one a new terminal gets", async () => {
		settingsStore.setShell("/opt/homebrew/bin/fish");
		settingsStore.setSmartSelectionRules([markdownRule("run_background")]);
		const mounted = await rightClickAndChoose("see README.md now", 6, "b2s", "tb2s");

		await waitFor(() => expect(backgroundCalls()).toHaveLength(1));
		expect((backgroundCalls()[0].args as { shell?: string }).shell).toBe("/opt/homebrew/bin/fish");
		settingsStore.setShell(null);
		await mounted.dispose();
	});

	it("Run Command runs from the terminal's own folder", async () => {
		registeredTerminal = terminalsStore.add({
			sessionId: "b4",
			name: "t",
			fontSize: 14,
			cwd: "/work/tree",
			awaitingInput: null,
		});
		settingsStore.setSmartSelectionRules([markdownRule("run_background")]);
		const mounted = await rightClickAndChoose("see docs/a.md now", 6, "b4", "tb4");

		await waitFor(() => expect(backgroundCalls()).toHaveLength(1));
		expect(backgroundCalls()[0].args).toEqual({
			command: `imark "${shVar("TUIC_M0")}"`,
			cwd: "/work/tree",
			env: { TUIC_M0: "docs/a.md" },
			shell: null,
		});
		await mounted.dispose();
	});

	it("a command that failed raises a toast saying how it ended", async () => {
		fakeTransport.current!.setInvokeHandler("run_background_command", () => ({
			status: "failed",
			exit_code: 127,
		}));
		settingsStore.setSmartSelectionRules([markdownRule("run_background")]);
		const mounted = await rightClickAndChoose("see README.md now", 6, "b5", "tb5");

		await waitFor(() => expect(toastsStore.toasts.map((t) => t.title)).toContain("Command failed"));
		const toast = toastsStore.toasts.find((t) => t.title === "Command failed")!;
		expect(toast.message).toContain("not found");
		expect(toast.message).toContain("PATH");
		await mounted.dispose();
	});

	it.each([{ status: "succeeded" }, { status: "running" }])("a command that is %j says nothing", async (outcome) => {
		fakeTransport.current!.setInvokeHandler("run_background_command", () => outcome);
		settingsStore.setSmartSelectionRules([markdownRule("run_background")]);
		const mounted = await rightClickAndChoose("see README.md now", 6, "b6", "tb6");

		await waitFor(() => expect(backgroundCalls()).toHaveLength(1));
		await new Promise((r) => setTimeout(r, 50));
		expect(toastsStore.toasts).toHaveLength(0);
		await mounted.dispose();
	});

	it("a command that could not be started at all raises a toast with the reason", async () => {
		fakeTransport.current!.setInvokeHandler("run_background_command", () => {
			throw new Error("Working directory does not exist: /gone");
		});
		settingsStore.setSmartSelectionRules([markdownRule("run_background")]);
		const mounted = await rightClickAndChoose("see README.md now", 6, "b7", "tb7");

		await waitFor(() => expect(toastsStore.toasts.map((t) => t.title)).toContain("Couldn't run command"));
		expect(toastsStore.toasts.find((t) => t.title === "Couldn't run command")!.message).toContain("does not exist");
		await mounted.dispose();
	});

	it("Run Command 'In a new terminal': opens a tab that runs the quoted command, and starts nothing in the background", async () => {
		settingsStore.setSmartSelectionRules([
			markdownRule("run_background", {
				actions: [
					{
						kind: "run_background",
						title: "Open in Imark",
						parameter: "imark \\0",
						isDefault: true,
						runIn: "new_terminal",
					},
				],
			}),
		]);
		// "Run in a new terminal" first asks the backend whether another session may be spawned, and
		// (separately) whether the terminal's shell is one a command line may safely be typed into.
		mockInvoke.mockImplementation(async (cmd: string) => (cmd === "can_spawn_session" ? true : undefined));
		fakeTransport.current!.setInvokeHandler("run_command_shell_support", () => ({ shell: "zsh", supported: true }));
		const before = new Set(Object.keys(terminalsStore.state.terminals));
		const mounted = await rightClickAndChoose("see it's.md now", 6, "bn1", "tbn1");

		await waitFor(() => {
			const created = Object.values(terminalsStore.state.terminals).filter(
				(t) => t.pendingInitCommand === `TUIC_M0='it'\\''s.md' /bin/sh -c 'imark "\${TUIC_M0}"'`,
			);
			expect(created).toHaveLength(1);
		});
		expect(backgroundCalls()).toHaveLength(0);
		expect(fakeTransport.current!.invokeCalls.find((c) => c.cmd === "run_command_shell_support")?.args).toEqual({
			shell: null,
		});
		for (const id of Object.keys(terminalsStore.state.terminals)) {
			if (!before.has(id) && terminalsStore.state.terminals[id].pendingInitCommand) terminalsStore.remove(id);
		}
		await mounted.dispose();
	});

	describe("Run Command 'In a new terminal' when the shell cannot be trusted with a typed line", () => {
		const newTerminalRule = () =>
			markdownRule("run_background", {
				actions: [
					{
						kind: "run_background",
						title: "Open in Imark",
						parameter: "imark \\0",
						isDefault: true,
						runIn: "new_terminal",
					},
				],
			});

		async function chooseWithShell(answer: unknown, sessionId: string, terminalId: string) {
			settingsStore.setSmartSelectionRules([newTerminalRule()]);
			mockInvoke.mockImplementation(async (cmd: string) => (cmd === "can_spawn_session" ? true : undefined));
			fakeTransport.current!.setInvokeHandler("run_command_shell_support", () => answer);
			const before = new Set(Object.keys(terminalsStore.state.terminals));
			const mounted = await rightClickAndChoose("see README.md now", 6, sessionId, terminalId);
			return { mounted, before };
		}
		const opened = (before: Set<string>) =>
			Object.keys(terminalsStore.state.terminals).filter(
				(id) => !before.has(id) && terminalsStore.state.terminals[id].pendingInitCommand,
			);

		it("fish: opens no terminal, types nothing, starts nothing, and says which shell and what to do", async () => {
			const { mounted, before } = await chooseWithShell({ shell: "fish", supported: false }, "bu1", "tbu1");

			await waitFor(() => expect(toastsStore.toasts.map((t) => t.title)).toContain("Can't run in a new terminal"));
			const toast = toastsStore.toasts.find((t) => t.title === "Can't run in a new terminal")!;
			expect(toast.message).toContain("fish");
			expect(toast.message).toContain("In the background");
			expect(opened(before)).toEqual([]);
			expect(typedCalls()).toHaveLength(0);
			expect(backgroundCalls()).toHaveLength(0);
			await mounted.dispose();
		});

		it("no usable answer from the backend counts as unsupported, never as permission", async () => {
			for (const answer of [undefined, null, {}, { shell: "zsh" }, { supported: "yes" }]) {
				for (const toast of [...toastsStore.toasts]) toastsStore.remove(toast.id);
				const { mounted, before } = await chooseWithShell(answer, "bu2", "tbu2");

				await waitFor(() => expect(toastsStore.toasts.map((t) => t.title)).toContain("Can't run in a new terminal"));
				expect(opened(before), JSON.stringify(answer)).toEqual([]);
				expect(typedCalls()).toHaveLength(0);
				await mounted.dispose();
			}
		});

		it("a failing check is also unsupported, and is logged rather than thrown", async () => {
			settingsStore.setSmartSelectionRules([newTerminalRule()]);
			fakeTransport.current!.setInvokeHandler("run_command_shell_support", () => {
				throw new Error("backend unreachable");
			});
			const before = new Set(Object.keys(terminalsStore.state.terminals));
			const mounted = await rightClickAndChoose("see README.md now", 6, "bu3", "tbu3");

			await waitFor(() => expect(toastsStore.toasts.map((t) => t.title)).toContain("Can't run in a new terminal"));
			expect(opened(before)).toEqual([]);
			await mounted.dispose();
		});
	});

	it("the older run_command still types into the terminal and does not run in the background", async () => {
		settingsStore.setSmartSelectionRules([markdownRule("run_command")]);
		const mounted = await rightClickAndChoose("see README.md now", 6, "b8", "tb8");

		await waitFor(() => expect(typedCalls().length).toBeGreaterThan(0));
		expect(backgroundCalls()).toHaveLength(0);
		// Typed as-is — unquoted, because a person reads it before it runs.
		expect(typedCalls().some((c) => String((c.args as { data?: string }).data).includes("imark README.md"))).toBe(true);
		await mounted.dispose();
	});

	it("Send Text with Press Enter: when safe types exactly what the older run_command did", async () => {
		settingsStore.setSmartSelectionRules([
			markdownRule("run_command", {
				actions: [
					{
						kind: "send_text",
						title: "Open in Imark",
						parameter: "imark \\0",
						isDefault: true,
						sendReturn: "when_safe",
					},
				],
			}),
		]);
		const mounted = await rightClickAndChoose("see README.md now", 6, "b8b", "tb8b");

		await waitFor(() => expect(typedCalls().length).toBeGreaterThan(0));
		expect(backgroundCalls()).toHaveLength(0);
		const typed = typedCalls().map((c) => String((c.args as { data?: string }).data));
		expect(typed.some((data) => data.includes("imark README.md"))).toBe(true);
		await mounted.dispose();
	});

	describe("left-clicking a link whose rule's default TYPES text (Send Text)", () => {
		const rowText = "see https://example.com/a here";
		const typingRule = () =>
			userRule({
				regex: "https://example\\.com/\\S*",
				actions: [
					{ kind: "send_text", title: "Type it", parameter: "open \\0", isDefault: true, sendReturn: "when_safe" },
				],
			});

		/** What `has_foreground_process` answers: null = the shell itself is at the prompt, a name = that
		 *  program is in the foreground, "fail" = the probe itself errors. */
		async function clickTheLink(sessionId: string, terminalId: string, foreground: string | null = null) {
			fakeTransport.current!.setInvokeHandler("has_foreground_process", () => {
				if (foreground === "fail") throw new Error("process exited during probe");
				return foreground;
			});
			settingsStore.setLinkActivation("click");
			fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => rowText);
			fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
			settingsStore.setSmartSelectionRules([typingRule()]);
			const mounted = await mountCanvasTerminal({ sessionId, terminalId });
			fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));
			const point = cellPoint(6, 0);
			fireEvent.mouseMove(document, point);
			await new Promise((r) => setTimeout(r, 150));
			fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
			fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });
			fireEvent.click(mounted.canvas, { button: 0, detail: 1, ...point });
			return mounted;
		}

		it("types the text when no agent is running", async () => {
			const mounted = await clickTheLink("ag0", "tag0");

			await waitFor(() => expect(typedCalls().length).toBeGreaterThan(0));
			expect(handleOpenUrl).not.toHaveBeenCalled();
			await mounted.dispose();
		});

		it("with vim (or any non-shell program) in the foreground and no agent detected, types NOTHING and opens the link", async () => {
			const mounted = await clickTheLink("fg1", "tfg1", "vim");

			await waitFor(() => expect(handleOpenUrl).toHaveBeenCalledWith("https://example.com/a"));
			expect(typedCalls()).toHaveLength(0);
			await mounted.dispose();
		});

		it("when the foreground probe itself fails, it is treated as busy: nothing typed, link opened", async () => {
			const mounted = await clickTheLink("fg2", "tfg2", "fail");

			await waitFor(() => expect(handleOpenUrl).toHaveBeenCalledWith("https://example.com/a"));
			expect(typedCalls()).toHaveLength(0);
			await mounted.dispose();
		});

		it("with an agent running, types NOTHING into it and opens the link normally instead", async () => {
			registeredTerminal = terminalsStore.add({
				sessionId: "ag1",
				name: "agent",
				fontSize: 14,
				cwd: null,
				awaitingInput: null,
			});
			terminalsStore.update(registeredTerminal, { agentType: "claude" });
			const mounted = await clickTheLink("ag1", "tag1");

			await waitFor(() => expect(handleOpenUrl).toHaveBeenCalledWith("https://example.com/a"));
			expect(typedCalls()).toHaveLength(0);
			await mounted.dispose();
		});

		it("with an agent running, a rule that does NOT type (Run Command) still runs on the click", async () => {
			registeredTerminal = terminalsStore.add({
				sessionId: "ag2",
				name: "agent",
				fontSize: 14,
				cwd: null,
				awaitingInput: null,
			});
			terminalsStore.update(registeredTerminal, { agentType: "claude" });
			settingsStore.setLinkActivation("click");
			fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => rowText);
			fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
			settingsStore.setSmartSelectionRules([
				userRule({
					regex: "https://example\\.com/\\S*",
					actions: [{ kind: "run_background", title: "Open elsewhere", parameter: "open \\0", isDefault: true }],
				}),
			]);
			const mounted = await mountCanvasTerminal({ sessionId: "ag2", terminalId: "tag2" });
			fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));
			const point = cellPoint(6, 0);
			fireEvent.mouseMove(document, point);
			await new Promise((r) => setTimeout(r, 150));
			fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
			fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });
			fireEvent.click(mounted.canvas, { button: 0, detail: 1, ...point });

			await waitFor(() => expect(backgroundCalls()).toHaveLength(1));
			expect(typedCalls()).toHaveLength(0);
			await mounted.dispose();
		});
	});

	it("left-clicking a link runs a Run Command rule's default in the background, typing nothing", async () => {
		settingsStore.setLinkActivation("click");
		const rowText = "see https://example.com/a here";
		fakeTransport.current!.setInvokeHandler("terminal_get_row_text", () => rowText);
		fakeTransport.current!.setInvokeHandler("terminal_hyperlink_span", () => null);
		settingsStore.setSmartSelectionRules([
			userRule({
				regex: "https://example\\.com/\\S*",
				actions: [{ kind: "run_background", title: "Open elsewhere", parameter: "open \\0", isDefault: true }],
			}),
		]);
		const mounted = await mountCanvasTerminal({ sessionId: "b9", terminalId: "tb9" });
		fakeTransport.current!.pushFrame(buildTextFrame([rowText], 40));
		const point = cellPoint(6, 0);
		fireEvent.mouseMove(document, point);
		await new Promise((r) => setTimeout(r, 150));

		fireEvent.mouseDown(mounted.canvas, { button: 0, ...point });
		fireEvent.mouseUp(mounted.canvas, { button: 0, ...point });
		fireEvent.click(mounted.canvas, { button: 0, detail: 1, ...point });

		await waitFor(() => expect(backgroundCalls()).toHaveLength(1));
		expect(backgroundCalls()[0].args).toEqual({
			command: `open "${shVar("TUIC_M0")}"`,
			cwd: null,
			env: { TUIC_M0: "https://example.com/a" },
			shell: null,
		});
		expect(typedCalls()).toHaveLength(0);
		expect(handleOpenUrl).not.toHaveBeenCalled();
		await mounted.dispose();
	});
});
