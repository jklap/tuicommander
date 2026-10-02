import { describe, expect, it } from "vitest";
import { buildAnswersTurn, readTurnRows, rowCopyText, TURN_FETCH_CHUNK, turnStart } from "../answersTurn";
import type { DecodedRow, StyledRange } from "../canvasTerminalUtils";
import type { RowSnapshot } from "../suggestOverlay";

const row = (text: string, isWrapped = false): RowSnapshot => ({ text, isWrapped });

function decoded(text: string, wrapped = false): DecodedRow {
	const cps = Uint32Array.from([...text].map((c) => c.codePointAt(0) ?? 0));
	return {
		index: 0,
		count: cps.length,
		wrapped,
		codepoints: cps,
		fg: new Uint32Array(cps.length),
		bg: new Uint32Array(cps.length),
		attrs: new Uint8Array(cps.length),
	} as unknown as DecodedRow;
}

describe("buildAnswersTurn", () => {
	it("keeps the prompt and only the answers, joining wrapped rows", () => {
		const turn = buildAnswersTurn(
			[
				row("❯ is it green?"),
				row("Read(package.json)"),
				row("💬 Yes, the long answer wraps and"),
				row(" continues here.   ", true),
				row("Bash(pnpm test)"),
				row("💬 Second."),
			],
			true,
		);
		expect(turn).toEqual({
			prompt: "❯ is it green?",
			answers: ["💬 Yes, the long answer wraps and continues here.", "💬 Second."],
		});
	});

	it("finds an answer whose first row is above what was visible (scrollback rows are just rows)", () => {
		// catches: answers that scrolled off the screen being lost from the compact view
		const rows = [row("❯ q")];
		for (let i = 0; i < 200; i++) rows.push(row(`tool output ${i}`));
		rows.splice(2, 0, row("💬 first answer, far above the screen"));
		rows.push(row("💬 last answer"));
		expect(buildAnswersTurn(rows, true).answers).toEqual(["💬 first answer, far above the screen", "💬 last answer"]);
	});

	it("does not treat a wrapped row that starts with the emoji as an answer", () => {
		// catches: wrap landing on a mid-sentence 💬 listing it as a separate answer
		const turn = buildAnswersTurn(
			[row("❯ q"), row("some output that fills the row to the very"), row("💬 end", true)],
			true,
		);
		expect(turn.answers).toEqual([]);
	});

	it("drops the agent bullet in front of the marker", () => {
		expect(buildAnswersTurn([row("❯ q"), row("⏺ 💬 Done.")], true).answers).toEqual(["💬 Done."]);
	});

	it("has no prompt when none is known", () => {
		expect(buildAnswersTurn([row("💬 a")], false)).toEqual({ prompt: null, answers: ["💬 a"] });
	});

	it("returns no answers for a turn without markers", () => {
		expect(buildAnswersTurn([row("❯ q"), row("plain output")], true).answers).toEqual([]);
	});
});

describe("rowCopyText", () => {
	it("drops the spacer cell after a wide emoji and keeps ordinary empty cells", () => {
		// catches: a doubled space after 💬 in copied answers
		const r = decoded("💬 hi  x");
		r.codepoints = Uint32Array.from([0x1f4ac, 0, 0x20, 0x68, 0x69, 0, 0, 0x78]);
		r.count = 8;
		expect(rowCopyText(r)).toBe("💬 hi  x");
	});
});

describe("turnStart", () => {
	it("starts at the last prompt, shifted to the all-time index", () => {
		expect(turnStart([3, 40, 90], 1000, 1200)).toEqual({ startAbs: 1090, hasPrompt: true });
	});

	it("ignores prompts beyond the end and negative ones", () => {
		expect(turnStart([-1, 500], 1000, 1200)).toEqual({ startAbs: 1000, hasPrompt: false });
	});

	it("falls back to a bounded window of the end when no prompt is known", () => {
		expect(turnStart([], 0, 20000)).toEqual({ startAbs: 15000, hasPrompt: false });
	});
});

describe("readTurnRows", () => {
	const range = (start: number, count: number, endAbs: number): StyledRange => ({
		startAbs: start,
		historySize: 0,
		cols: 10,
		rows: Array.from({ length: Math.min(count, endAbs - start) }, (_, i) => ({
			abs: start + i,
			row: decoded(`r${start + i}`, (start + i) % 2 === 1),
		})),
	});

	it("reads the whole turn across chunk boundaries in order, keeping wrap flags", async () => {
		const calls: Array<[number, number]> = [];
		const end = 10 + TURN_FETCH_CHUNK + 5;
		const rows = await readTurnRows(
			async (s, c) => {
				calls.push([s, c]);
				return range(s, c, end);
			},
			10,
			end,
		);
		expect(calls).toEqual([
			[10, TURN_FETCH_CHUNK],
			[10 + TURN_FETCH_CHUNK, 5],
		]);
		expect(rows?.length).toBe(TURN_FETCH_CHUNK + 5);
		expect(rows?.[0]).toEqual({ text: "r10", isWrapped: false });
		expect(rows?.[1]).toEqual({ text: "r11", isWrapped: true });
		expect(rows?.[TURN_FETCH_CHUNK + 4].text).toBe(`r${end - 1}`);
	});

	it("aborts on a failed chunk instead of returning a partial turn", async () => {
		// catches: a half-read turn rendered as if it were the whole answer set
		let n = 0;
		const rows = await readTurnRows(async (s, c) => (n++ === 0 ? range(s, c, 10_000) : null), 0, TURN_FETCH_CHUNK * 2);
		expect(rows).toBeNull();
	});

	it("keeps alignment with blank rows when the backend skipped some", async () => {
		const rows = await readTurnRows(
			async () => ({ startAbs: 0, historySize: 0, cols: 4, rows: [{ abs: 2, row: decoded("c") }] }),
			0,
			4,
		);
		expect(rows?.map((r) => r.text)).toEqual(["", "", "c", ""]);
	});
});
