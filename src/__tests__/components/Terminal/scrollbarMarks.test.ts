import { describe, expect, it } from "vitest";
import {
	buildScrollbarMarksHtml,
	type ScrollbarMarksInput,
	shouldShowScrollbar,
} from "../../../components/Terminal/scrollbarMarks";

/** Colours are the only way to tell the four tick kinds apart in the output. */
const FAILED = "#f85149";
const OK = "rgba(88,166,255,0.5)";
const PROMPT = "#3fb950";
const SEARCH = "#e8984c";

const countOf = (html: string, color: string) => html.split(`background:${color}`).length - 1;

function input(over: Partial<ScrollbarMarksInput> = {}): ScrollbarMarksInput {
	return {
		blocks: [
			{ promptLine: 0, exitCode: 0 },
			{ promptLine: 50, exitCode: 1 },
		],
		promptLines: [10, 60],
		matchRows: [20, 80],
		totalRows: 100,
		trackH: 200,
		showBlockMarks: true,
		showPromptMarks: true,
		...over,
	};
}

describe("buildScrollbarMarksHtml", () => {
	it("draws block, prompt and search ticks when history markers are on", () => {
		const html = buildScrollbarMarksHtml(input());
		expect(countOf(html, OK)).toBe(1);
		expect(countOf(html, FAILED)).toBe(1);
		expect(countOf(html, PROMPT)).toBe(2);
		expect(countOf(html, SEARCH)).toBe(2);
	});

	// The decision this module exists to hold (story 723-6b02). A terminal display
	// preference may hide command history; it may not silently disable the feedback
	// for a search the user just ran.
	it("keeps search ticks when history markers are off", () => {
		const html = buildScrollbarMarksHtml(input({ showBlockMarks: false, showPromptMarks: false }));
		expect(countOf(html, SEARCH), "search hits must survive showScrollbarMarks=false").toBe(2);
		expect(countOf(html, OK) + countOf(html, FAILED), "no block ticks").toBe(0);
		expect(countOf(html, PROMPT), "no user-prompt ticks").toBe(0);
	});

	// The paired half: with the setting off and nothing searched, the overlay must
	// come back EMPTY rather than be skipped. The caller assigns this to
	// `innerHTML`, so an empty string is what actually erases marks drawn before
	// the user turned the setting off — returning early instead left them on
	// screen indefinitely.
	it("returns an empty overlay with history off and no search", () => {
		expect(buildScrollbarMarksHtml(input({ showBlockMarks: false, showPromptMarks: false, matchRows: [] }))).toBe("");
	});

	it("collapses search hits that round onto the same pixel", () => {
		// 300 rows over a 10px track: rows 0 and 1 both land on pixel 0.
		const html = buildScrollbarMarksHtml(
			input({ showBlockMarks: false, showPromptMarks: false, matchRows: [0, 1, 2, 150], totalRows: 300, trackH: 10 }),
		);
		expect(countOf(html, SEARCH)).toBe(2);
	});

	it("colours a block by its exit code, treating a still-running block as fine", () => {
		const html = buildScrollbarMarksHtml(
			input({
				blocks: [
					{ promptLine: 0, exitCode: null },
					{ promptLine: 10, exitCode: 0 },
					{ promptLine: 20, exitCode: 130 },
				],
				promptLines: [],
				matchRows: [],
			}),
		);
		expect(countOf(html, OK), "null and 0 are both non-failures").toBe(2);
		expect(countOf(html, FAILED)).toBe(1);
	});

	it("places a tick at the track-relative position of its row", () => {
		const html = buildScrollbarMarksHtml(
			input({ blocks: [{ promptLine: 25, exitCode: 0 }], promptLines: [], matchRows: [] }),
		);
		// 25/100 of a 200px track.
		expect(html).toContain("top:50px");
	});
});

describe("shouldShowScrollbar", () => {
	// Regression: a tab with no scrollback (historySize === 0) — the common case for a
	// short Claude Code turn that never scrolled past one screen — used to hide the whole
	// scrollbar track unconditionally, which hid any block/prompt marks on it too, even
	// with real data and both toggles on.
	const visBase = {
		historySize: 0,
		showBlockMarks: true,
		showPromptMarks: true,
		blocks: [{ promptLine: 3 }],
		promptLines: [3],
	};

	it("is true when there's scrollable history, regardless of marks", () => {
		expect(shouldShowScrollbar({ ...visBase, blocks: [], promptLines: [], historySize: 1 })).toBe(true);
	});

	it("is true with no history but a block mark present and its toggle on", () => {
		expect(shouldShowScrollbar({ ...visBase, promptLines: [] })).toBe(true);
	});

	it("is true with no history but a prompt mark present and its toggle on", () => {
		expect(shouldShowScrollbar({ ...visBase, blocks: [] })).toBe(true);
	});

	it("is false with no history and a block mark present but its toggle OFF", () => {
		expect(shouldShowScrollbar({ ...visBase, showBlockMarks: false, promptLines: [] })).toBe(false);
	});

	it("is false with no history and a prompt mark present but its toggle OFF", () => {
		expect(shouldShowScrollbar({ ...visBase, showPromptMarks: false, blocks: [] })).toBe(false);
	});

	it("is false with no history and nothing to mark", () => {
		expect(shouldShowScrollbar({ ...visBase, blocks: [], promptLines: [] })).toBe(false);
	});
});
