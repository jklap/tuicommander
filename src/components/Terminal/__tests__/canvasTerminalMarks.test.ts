import { describe, expect, it } from "vitest";
import {
	type MarkBlock,
	type ScrollbarMarksInput,
	scrollbarMarksHtml,
	scrollbarMarksKey,
	shouldShowScrollbar,
} from "../canvasTerminalMarks";

function block(overrides: Partial<MarkBlock> = {}): MarkBlock {
	return { promptLine: 10, endLine: 20, exitCode: null, ...overrides };
}

function baseInput(overrides: Partial<ScrollbarMarksInput> = {}): ScrollbarMarksInput {
	return {
		showBlockMarks: true,
		showPromptMarks: true,
		blocks: [block()],
		promptLines: [5],
		totalRows: 100,
		matches: [],
		...overrides,
	};
}

describe("scrollbarMarksKey", () => {
	// Regression for 1c0c0291: paintScrollbarMarks gated its whole block/prompt
	// section on one transient flag, and the memo key collapsed both counts to 0
	// while hidden — so re-enabling either toggle with otherwise-unchanged data
	// could return the SAME key as while it was hidden, and the repaint would be
	// skipped as a no-op.
	it("changes when showBlockMarks flips, even with unchanged blocks/prompts/totalRows", () => {
		const hidden = scrollbarMarksKey(baseInput({ showBlockMarks: false }));
		const shown = scrollbarMarksKey(baseInput({ showBlockMarks: true }));
		expect(hidden).not.toBe(shown);
	});

	it("changes when showPromptMarks flips, even with unchanged blocks/prompts/totalRows", () => {
		const hidden = scrollbarMarksKey(baseInput({ showPromptMarks: false }));
		const shown = scrollbarMarksKey(baseInput({ showPromptMarks: true }));
		expect(hidden).not.toBe(shown);
	});

	it("does not churn when a HIDDEN category's underlying data changes", () => {
		const a = scrollbarMarksKey(baseInput({ showBlockMarks: false, blocks: [block({ promptLine: 1 })] }));
		const b = scrollbarMarksKey(
			baseInput({ showBlockMarks: false, blocks: [block({ promptLine: 1 }), block({ promptLine: 2 })] }),
		);
		expect(a).toBe(b);
	});

	it("still changes when a VISIBLE category's underlying data changes", () => {
		const a = scrollbarMarksKey(baseInput({ showBlockMarks: true, blocks: [block({ promptLine: 1 })] }));
		const b = scrollbarMarksKey(
			baseInput({ showBlockMarks: true, blocks: [block({ promptLine: 1 }), block({ promptLine: 2 })] }),
		);
		expect(a).not.toBe(b);
	});

	it("the two categories are independent — toggling one doesn't affect the other's contribution", () => {
		const blockOnly = scrollbarMarksKey(baseInput({ showBlockMarks: true, showPromptMarks: false }));
		const promptOnly = scrollbarMarksKey(baseInput({ showBlockMarks: false, showPromptMarks: true }));
		expect(blockOnly).not.toBe(promptOnly);
	});
});

describe("scrollbarMarksHtml", () => {
	it("gates block ticks on showBlockMarks and prompt ticks on showPromptMarks independently", () => {
		const blockOnly = scrollbarMarksHtml(baseInput({ showBlockMarks: true, showPromptMarks: false }), 200);
		expect(blockOnly).toContain("rgba(88,166,255,0.5)");
		expect(blockOnly).not.toContain("#3fb950");

		const promptOnly = scrollbarMarksHtml(baseInput({ showBlockMarks: false, showPromptMarks: true }), 200);
		expect(promptOnly).toContain("#3fb950");
		expect(promptOnly).not.toContain("rgba(88,166,255,0.5)");
		expect(promptOnly).not.toContain("#f85149");
	});

	it("renders a red tick for a non-zero exit code, blue for zero/null", () => {
		const input = baseInput({
			blocks: [
				block({ promptLine: 1, exitCode: 1 }),
				block({ promptLine: 2, exitCode: 0 }),
				block({ promptLine: 3, exitCode: null }),
			],
		});
		const html = scrollbarMarksHtml(input, 200);
		expect((html.match(/#f85149/g) ?? []).length).toBe(1);
		expect((html.match(/rgba\(88,166,255,0\.5\)/g) ?? []).length).toBe(2);
	});

	it("draws prompt ticks after block ticks so they sit on top", () => {
		const html = scrollbarMarksHtml(baseInput(), 200);
		const blockIdx = html.indexOf("rgba(88,166,255,0.5)");
		const promptIdx = html.indexOf("#3fb950");
		expect(blockIdx).toBeGreaterThanOrEqual(0);
		expect(promptIdx).toBeGreaterThan(blockIdx);
	});

	it("draws deduplicated search-match ticks last", () => {
		const html = scrollbarMarksHtml(
			baseInput({
				matches: [
					{ row: 50, col_start: 0, col_end: 1 },
					{ row: 50, col_start: 2, col_end: 3 },
				],
			}),
			200,
		);
		expect((html.match(/#e8984c/g) ?? []).length).toBe(1);
	});

	it("emits nothing when everything is hidden/empty", () => {
		const html = scrollbarMarksHtml(
			baseInput({ showBlockMarks: false, showPromptMarks: false, promptLines: [], blocks: [] }),
			200,
		);
		expect(html).toBe("");
	});
});

describe("shouldShowScrollbar", () => {
	// Regression: a tab with no scrollback (historySize === 0) — the common case for a
	// short Claude Code turn that never scrolled past one screen — used to hide the whole
	// scrollbar track unconditionally, which hid any block/prompt marks on it too, even
	// with real data and both toggles on.
	it("is true when there's scrollable history, regardless of marks", () => {
		expect(shouldShowScrollbar({ ...baseInput({ blocks: [], promptLines: [] }), historySize: 1 })).toBe(true);
	});

	it("is true with no history but a block mark present and its toggle on", () => {
		expect(shouldShowScrollbar({ ...baseInput({ showBlockMarks: true, promptLines: [] }), historySize: 0 })).toBe(true);
	});

	it("is true with no history but a prompt mark present and its toggle on", () => {
		expect(shouldShowScrollbar({ ...baseInput({ showPromptMarks: true, blocks: [] }), historySize: 0 })).toBe(true);
	});

	it("is false with no history and a block mark present but its toggle OFF", () => {
		expect(shouldShowScrollbar({ ...baseInput({ showBlockMarks: false, promptLines: [] }), historySize: 0 })).toBe(
			false,
		);
	});

	it("is false with no history and no marks at all — preserves the plain-terminal look", () => {
		expect(shouldShowScrollbar({ ...baseInput({ blocks: [], promptLines: [] }), historySize: 0 })).toBe(false);
	});
});
