import { describe, expect, it } from "vitest";
import type { DecodedRow } from "../canvasTerminalUtils";
import { rowText } from "../canvasTerminalUtils";
import {
	answerBlockRanges,
	answerExtent,
	type ChatBlock,
	continuationRowsAfterSuggest,
	isSuggestBlock,
	paintOverlayBlocks,
	planSuggestOverlay,
	type RowSnapshot,
} from "../suggestOverlay";

/** Build a `getRow` lookup from a compact string/bool list, with null past end. */
function rows(snapshots: Array<[string, boolean]>): (i: number) => RowSnapshot | null {
	return (i) => {
		if (i < 0 || i >= snapshots.length) return null;
		const [text, isWrapped] = snapshots[i];
		return { text, isWrapped };
	};
}

describe("continuationRowsAfterSuggest", () => {
	it("returns [] for a single-line bracketed suggest (closes on the anchor)", () => {
		const get = rows([
			["suggest: [ A | B | C ]", false], // anchor closes here
			["bash: some unrelated next line", false],
		]);
		expect(continuationRowsAfterSuggest(0, 2, get)).toEqual([]);
	});

	it("does not swallow trailing pipe content after a closed single-line suggest", () => {
		// The `]` bounds the token; a following mermaid/table pipe row is ignored.
		const get = rows([
			["suggest: [ A | B | C ]", false], // closed
			["software_product ||--o{ software_signature | product_id", false], // mermaid — must NOT be hidden
		]);
		expect(continuationRowsAfterSuggest(0, 2, get)).toEqual([]);
	});

	it("hides wrapped continuation rows up to and including the closing ] row", () => {
		const get = rows([
			["suggest: [ 1) very long first item that wraps | 2) second", false], // anchor, no ]
			[" item also long | 3) third item that keeps going", true], // wrapped
			[" onto another row ]", true], // closing ] here
			["bash$ ", false], // after ] — must NOT be hidden
		]);
		expect(continuationRowsAfterSuggest(0, 4, get)).toEqual([1, 2]);
	});

	it("hides a non-wrapped tail row carrying the closing ]", () => {
		const get = rows([
			["suggest: [ A | B |", false], // anchor, open
			["C ]", false], // tail with closing ]
			["unrelated next row |", false], // after ] — must NOT be hidden
		]);
		expect(continuationRowsAfterSuggest(0, 3, get)).toEqual([1]);
	});

	it("stops at a new suggest anchor before the ] arrives", () => {
		const get = rows([
			["suggest: [ A | B", false], // anchor, unclosed
			["suggest: [ X | Y | Z ]", false], // new suggest — stop before it
		]);
		expect(continuationRowsAfterSuggest(0, 2, get)).toEqual([]);
	});

	it("stops at a new intent token before the ] arrives", () => {
		const get = rows([
			["suggest: [ A | B", false], // anchor, unclosed
			["intent: doing something new", false], // new intent — stop
		]);
		expect(continuationRowsAfterSuggest(0, 2, get)).toEqual([]);
	});

	it("handles empty buffer past the anchor", () => {
		const get = rows([["suggest: [ A | B ]", false]]);
		expect(continuationRowsAfterSuggest(0, 1, get)).toEqual([]);
	});

	it("stops when getRow returns null (gap in the buffer)", () => {
		const get = (i: number) => {
			if (i === 0) return { text: "suggest: [ A | B", isWrapped: false };
			return null;
		};
		expect(continuationRowsAfterSuggest(0, 5, get)).toEqual([]);
	});
});

describe("isSuggestBlock", () => {
	it("returns true for a bracketed suggest with pipe on the same line", () => {
		const get = rows([["suggest: [ A | B | C ]", false]]);
		expect(isSuggestBlock(0, 1, get)).toBe(true);
	});

	it("returns true when the pipe is on a wrapped continuation row", () => {
		const get = rows([
			["suggest: [ 1) Testa il popup con Shift+Cmd+I su un upstream r", false],
			["eale | 2) Continua con la story (clippy cleanup)", true],
			["| 3) Crea una PR per questi cambiamenti ]", true],
		]);
		expect(isSuggestBlock(0, 3, get)).toBe(true);
	});

	it("returns false for prose starting with suggest: but no bracket", () => {
		const get = rows([
			["suggest: we should refactor the codebase", false],
			["to improve performance and readability", true],
		]);
		expect(isSuggestBlock(0, 2, get)).toBe(false);
	});

	it("returns false for a bracketed suggest with a single item (no pipe)", () => {
		const get = rows([["suggest: [ just one option ]", false]]);
		expect(isSuggestBlock(0, 1, get)).toBe(false);
	});

	it("returns false for a row that does not start with suggest:", () => {
		const get = rows([["I suggest: [ try something | maybe ]", false]]);
		expect(isSuggestBlock(0, 1, get)).toBe(false);
	});

	it("returns true with Ink bullet prefix", () => {
		const get = rows([["● suggest: [ Run tests | Check logs ]", false]]);
		expect(isSuggestBlock(0, 1, get)).toBe(true);
	});

	it("returns false when row is not the anchor index", () => {
		const get = rows([
			["unrelated row", false],
			["suggest: [ A | B ]", false],
		]);
		expect(isSuggestBlock(0, 2, get)).toBe(false);
	});
});

describe("planSuggestOverlay", () => {
	it("names every row the overlay must mask, in order", () => {
		const get = rows([
			["bash: unrelated", false],
			["suggest: [ A |", false],
			["B | C ]", true],
			["intent: doing a thing (Thing)", false],
		]);
		const plan = planSuggestOverlay(4, get);
		expect(plan.blocks).toEqual([
			{ row: 1, kind: "suggest" },
			{ row: 2, kind: "continuation" },
			{ row: 3, kind: "intent" },
		]);
	});

	// The key exists so an unchanged screen skips the DOM rebuild. Computing it
	// from freshly built <div>s defeats the point: the elements are created and
	// dropped on every frame that repaints, which is most of them.
	it("gives identical screens the same key and different screens different keys", () => {
		const screen = (): Array<[string, boolean]> => [
			["suggest: [ A | B | C ]", false],
			["intent: x (X)", false],
		];
		const a = planSuggestOverlay(2, rows(screen()));
		const b = planSuggestOverlay(2, rows(screen()));
		expect(a.key).toBe(b.key);
		expect(a.key).not.toBe("");

		const moved = planSuggestOverlay(
			2,
			rows([
				["other", false],
				["intent: x (X)", false],
			]),
		);
		expect(moved.key).not.toBe(a.key);
	});

	// The highlight pattern is looser than the walk's stop pattern on purpose: an
	// agent prints its intent line behind a bullet, and that row must still be
	// tinted. Using the stop pattern here would silently leave it untinted.
	it("tints an intent line that sits behind a bullet", () => {
		const plan = planSuggestOverlay(1, rows([["\u23fa intent: doing a thing (Thing)", false]]));
		expect(plan.blocks).toEqual([{ row: 0, kind: "intent" }]);
	});

	// ...but the same looseness must NOT end a suggest block early, or the row
	// carrying the closing bracket stops being masked and the raw token shows.
	it("does not let an indented intent mention cut a suggest block short", () => {
		const plan = planSuggestOverlay(
			3,
			rows([
				["suggest: [ A |", false],
				["  intent: mentioned in passing", true],
				["B | C ]", true],
			]),
		);
		expect(plan.blocks).toEqual([
			{ row: 0, kind: "suggest" },
			{ row: 1, kind: "continuation" },
			{ row: 2, kind: "continuation" },
		]);
	});

	it("has an empty key when nothing needs masking", () => {
		expect(
			planSuggestOverlay(
				2,
				rows([
					["ls -la", false],
					["file.txt", false],
				]),
			),
		).toEqual({
			key: "",
			blocks: [],
		});
	});
});

describe("planSuggestOverlay — 💬 answer marker", () => {
	it("tints a marker line", () => {
		const get = rows([
			["some tool output", false],
			["💬 The build passes.", false],
		]);
		expect(planSuggestOverlay(2, get).blocks).toEqual([{ row: 1, kind: "answer" }]);
	});

	it("tints every row a wrapped marker line spans", () => {
		// catches: highlight lost on a long answer, where only the first row carries the marker
		const get = rows([
			["💬 A long answer that does not fit on one row and", false],
			["wraps onto a second row and even", true],
			["a third one.", true],
			["next unrelated line", false],
		]);
		expect(planSuggestOverlay(4, get).blocks).toEqual([
			{ row: 0, kind: "answer" },
			{ row: 1, kind: "answer" },
			{ row: 2, kind: "answer" },
		]);
	});

	it("tints a multi-line answer to its last row and stops before the next tool call", () => {
		// catches: only the marker row highlighted, or the tint bleeding into the next bullet
		const get = rows([
			["⏺ 💬 The build passes because the cache was warm and", false],
			["  the lockfile did not change.", true],
			["", false],
			["  Second paragraph of the same answer.", false],
			["  - a bullet item inside the answer", false],
			["⏺ Bash(make test)", false],
			["  ⎿  ok", false],
		]);
		expect(planSuggestOverlay(7, get).blocks.map((b) => b.row)).toEqual([0, 1, 2, 3, 4]);
	});

	it("ends an answer at a column-0 row and does not tint trailing blank rows", () => {
		// catches: the highlight swallowing the next user prompt or status line
		const get = rows([
			["⏺ 💬 Done.", false],
			["  One more line.", false],
			["", false],
			["✻ Cooked for 5s", false],
			["❯ next question", false],
		]);
		expect(planSuggestOverlay(5, get).blocks.map((b) => b.row)).toEqual([0, 1]);
	});

	it("starts a new answer at a second marker instead of extending the first", () => {
		// catches: the marker break deleted from answerExtent, so the first answer swallows the second
		const get = rows([
			["⏺ 💬 First.", false],
			["  more first", false],
			["⏺ 💬 Second.", false],
		]);
		expect(answerExtent(0, 3, get)).toBe(1);
		expect(answerExtent(2, 3, get)).toBe(2);
	});

	it("does not treat an emoji that a wrap lands on as a marker", () => {
		// catches: a mid-sentence 💬 pushed to a row start by wrapping being highlighted as an answer
		const get = rows([
			["some ordinary output that fills the row up to the very", false],
			["💬 end", true],
		]);
		expect(planSuggestOverlay(2, get).blocks).toEqual([]);
	});

	it("does not match a marker in the middle of a line", () => {
		const get = rows([["see the 💬 emoji", false]]);
		expect(planSuggestOverlay(1, get).blocks).toEqual([]);
	});

	it("matches a marker behind the agent bullet", () => {
		const get = rows([["⏺ 💬 Done.", false]]);
		expect(planSuggestOverlay(1, get).blocks).toEqual([{ row: 0, kind: "answer" }]);
	});

	it("detects a marker row whose cells carry bold styling and a wide-char spacer", () => {
		// catches: styling (ANSI bold/colour) or the wire's zero spacer cell after the wide emoji hiding the marker
		const codepoints = [..."💬 Styled answer"].map((c) => c.codePointAt(0) ?? 32);
		codepoints.splice(1, 0, 0); // wide-char spacer cell
		const row = {
			index: 0,
			count: codepoints.length,
			wrapped: false,
			codepoints: Uint32Array.from(codepoints),
			fg: new Uint32Array(codepoints.length).fill(0xff0000),
			bg: new Uint32Array(codepoints.length),
			attrs: new Uint8Array(codepoints.length).fill(1), // bold
		} as unknown as DecodedRow;
		const get = (i: number): RowSnapshot | null => (i === 0 ? { text: rowText(row), isWrapped: row.wrapped } : null);
		expect(planSuggestOverlay(1, get).blocks).toEqual([{ row: 0, kind: "answer" }]);
	});

	it("changes the key when the marker set changes", () => {
		const without = planSuggestOverlay(1, rows([["plain", false]]));
		const withMarker = planSuggestOverlay(1, rows([["💬 yes", false]]));
		expect(withMarker.key).not.toBe(without.key);
	});
});

describe("paintOverlayBlocks", () => {
	it("masks suggest rows with the background and keeps answer rows translucent with a gutter", () => {
		// catches: an answer row masked like a suggest row, hiding the answer text
		const container = document.createElement("div");
		paintOverlayBlocks(
			container,
			[
				{ row: 1, kind: "suggest" },
				{ row: 2, kind: "answer" },
			],
			20,
			"rgb(10, 10, 10)",
		);
		const [masked, answer] = Array.from(container.children) as HTMLElement[];
		expect(masked.style.top).toBe("20px");
		expect(masked.style.background).toBe("rgb(10, 10, 10)");
		expect(answer.style.top).toBe("40px");
		expect(answer.style.background).toContain("0.14");
		expect(answer.style.boxShadow).toContain("inset 3px");
	});

	it("replaces the previous strips instead of stacking them", () => {
		const container = document.createElement("div");
		paintOverlayBlocks(container, [{ row: 0, kind: "answer" }], 20, "#000");
		paintOverlayBlocks(container, [], 20, "#000");
		expect(container.children.length).toBe(0);
	});
});

describe("answerBlockRanges", () => {
	const p = (text: string, marker = false): ChatBlock => ({ kind: "paragraph", text, marker });
	const code = (text: string): ChatBlock => ({ kind: "code", text, marker: false });
	const other = (text: string): ChatBlock => ({ kind: "other", text, marker: false });

	// Catches: the tint stopping at the marker paragraph, or a second answer swallowed into the first.
	it("spans each answer from its marker to the next marker or the end", () => {
		const blocks = [p("intro"), p("A", true), other("- x"), p("B", true), other("- y"), p("tail")];
		expect(answerBlockRanges(blocks)).toEqual([
			[1, 2],
			[3, 5],
		]);
	});

	// Catches: a literal 💬 line inside a code block ending the answer, as the grid never does.
	it("keeps a code block that shows a marker inside the answer", () => {
		expect(answerBlockRanges([p("A", true), code("💬 example"), p("after")])).toEqual([[0, 2]]);
	});
});

describe("submitted prompt highlight", () => {
	const turn = rows([
		["> a long question that wraps", false],
		["onto a second row", true],
		["● Bash(ls)", false],
		["  ⎿ output", false],
		["❯ half-typed reply", false],
	]);
	const plan = (promptRows: number[]) => planSuggestOverlay(5, turn, (row) => promptRows.includes(row));

	// Catches: only the first row of a wrapped question coloured.
	it("colours the prompt row and the rows it wraps onto", () => {
		expect(plan([0]).blocks).toEqual([
			{ row: 0, kind: "prompt" },
			{ row: 1, kind: "prompt" },
		]);
	});

	// Catches: the composer glyph or tool output coloured as if it were a submitted prompt.
	it("never colours the composer or tool output", () => {
		expect(plan([0]).blocks.map((block) => block.row)).not.toContain(4);
		expect(plan([0]).blocks.map((block) => block.row)).not.toContain(3);
		expect(planSuggestOverlay(5, turn).blocks).toEqual([]);
	});

	// Catches: the overlay not repainting when a prompt appears on rows whose text did not change.
	it("changes the plan key when the prompt set changes", () => {
		expect(plan([0]).key).not.toBe(plan([]).key);
	});

	// Catches: a hard-coded prompt colour that ignores the theme.
	it("paints the prompt with theme tokens, distinct from the answer green", () => {
		const container = document.createElement("div");
		paintOverlayBlocks(container, [{ row: 0, kind: "prompt" }], 20, "#000");
		const strip = container.firstElementChild as HTMLElement;
		expect(strip.style.cssText).toContain("var(--prompt-tint)");
		expect(strip.style.cssText).toContain("var(--prompt-bar)");
		expect(strip.style.cssText).not.toContain("94,190,140");
	});
});
