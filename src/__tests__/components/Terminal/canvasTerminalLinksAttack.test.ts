import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { filePathRegex } from "../../../components/Terminal/linkProvider";

const source = readFileSync(join(process.cwd(), "src/components/Terminal/CanvasTerminal.tsx"), "utf8");

function sliceBetween(start: string, end: string): string {
	const a = source.indexOf(start);
	expect(a, `missing ${start}`).toBeGreaterThan(-1);
	const b = source.indexOf(end, a);
	expect(b, `missing ${end}`).toBeGreaterThan(a);
	return source.slice(a, b);
}

function candidates(row: string): string[] {
	const re = filePathRegex();
	const out: string[] = [];
	let m: RegExpExecArray | null;
	while ((m = re.exec(row)) !== null) out.push(m[1]);
	return out;
}

describe("criterion 1: a bare file name reaches real detection", () => {
	// Catches: a bare name only linked when followed by a space/EOL, so the
	// decorations an agent TUI puts around a path leave it un-underlined.
	it.each([
		["⏺ Wrote followups.md", "followups.md"],
		["⏺ Update(followups.md)", "followups.md"],
		["  - followups.md:12", "followups.md:12"],
		["see `followups.md`.", "followups.md"],
		["followups.md", "followups.md"],
	])("row %j yields candidate %j", (row, want) => {
		expect(candidates(row)).toContain(want);
	});

	// Catches: box-drawing borders of a TUI panel glued to the name hide it.
	it("finds a bare name directly after a box-drawing border", () => {
		expect(candidates("│followups.md │")).toContain("followups.md");
	});
});

describe("click must belong to the press that was claimed", () => {
	const clickHandler = sliceBetween('bindings.listen(canvasRef, "click"', 'bindings.listen(canvasRef, "contextmenu"');

	// Catches: press forwarded to the app (drag-select, menu click elsewhere), then the
	// click event opens whatever hoveredLink was left over from an earlier hover or
	// from the drag's own probe. The click must only open when mousedown claimed it.
	it("opens a link only when the press was claimed from the app", () => {
		expect(clickHandler).toMatch(/claim|reportedDown|pressClaimed|linkPress/i);
	});

	// Catches: stale hoveredLink (probe still in its 100ms throttle) opened at a
	// position that is no longer over it.
	it("re-checks the pointer position against the link before opening", () => {
		expect(clickHandler).toMatch(/canvasToGrid|isOverSpan|clientX/);
	});
});

describe("a claimed press must not leave the app a half gesture", () => {
	const moveHandler = sliceBetween("const onMouseMove = (e: MouseEvent) => {", "const onMouseUp");

	// Catches: left press on a link swallowed, but drag motion (button held, mode 1002)
	// still reported to the app -> app sees a drag with no press.
	it("does not report drag motion for a press it claimed", () => {
		const reporting = moveHandler.slice(moveHandler.indexOf("mouseMode > 0"));
		expect(reporting).toMatch(/claimed|reportedDown\.has/i);
	});
});
