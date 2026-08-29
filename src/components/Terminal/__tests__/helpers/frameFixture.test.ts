import { describe, expect, it } from "vitest";
import { decodeBinaryFrame } from "../../canvasTerminalUtils";
import { buildFrame, buildTextFrame } from "./frameFixture";

describe("frameFixture — buildFrame/buildTextFrame decode round-trip", () => {
	it("decodes plain text lines back out via rowToText-equivalent codepoints", () => {
		const frame = decodeBinaryFrame(buildTextFrame(["foo bar baz", "https://example.com/a"], 24));
		expect(frame).not.toBeNull();
		expect(frame?.rows).toHaveLength(2);
		const text = (row: NonNullable<typeof frame>["rows"][number]) =>
			Array.from(row.codepoints)
				.map((cp) => (cp === 0 ? " " : String.fromCodePoint(cp)))
				.join("")
				.replace(/\s+$/, "");
		expect(text(frame!.rows[0])).toBe("foo bar baz");
		expect(text(frame!.rows[1])).toBe("https://example.com/a");
	});

	it("round-trips header fields (cursor, displayOffset, historySize, screen dims)", () => {
		const frame = decodeBinaryFrame(
			buildFrame({
				rows: [{ text: "x" }],
				cols: 10,
				cursorRow: 3,
				cursorCol: 5,
				cursorVisible: false,
				displayOffset: 7,
				historySize: 42,
				screenRows: 30,
				screenCols: 79,
				historyBase: 100,
			}),
		);
		expect(frame?.cursorRow).toBe(3);
		expect(frame?.cursorCol).toBe(5);
		expect(frame?.cursorVisible).toBe(false);
		expect(frame?.displayOffset).toBe(7);
		expect(frame?.historySize).toBe(42);
		expect(frame?.screenRows).toBe(30);
		expect(frame?.screenCols).toBe(79);
		expect(frame?.historyBase).toBe(100);
	});

	it("round-trips the per-row wrapped flag without corrupting the column count", () => {
		const frame = decodeBinaryFrame(buildFrame({ rows: [{ text: "abc", wrapped: true }], cols: 3 }));
		expect(frame?.rows[0].wrapped).toBe(true);
		expect(frame?.rows[0].count).toBe(3);
	});

	it("truncates text longer than the row's column count", () => {
		const frame = decodeBinaryFrame(buildTextFrame(["abcdef"], 3));
		expect(frame?.rows[0].count).toBe(3);
	});
});
