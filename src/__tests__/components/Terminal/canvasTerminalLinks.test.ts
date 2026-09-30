import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { isOverSpan, linkClaimsPress } from "../../../components/Terminal/canvasTerminalLinks";

/**
 * Claude Code's fullscreen mode reports the mouse, and a bare name such as
 * `followups.md` in its output was underlined (verification is independent of
 * mouse mode) while the click did nothing: the press went to the app and the
 * hover probe that feeds the click never ran.
 */
describe("links under mouse reporting", () => {
	const spans = [{ colStart: 6, colEnd: 18 }];

	it("claims a left press on an underlined span instead of forwarding it to the app", () => {
		expect(isOverSpan(spans, 6)).toBe(true);
		expect(isOverSpan(spans, 17)).toBe(true);
		expect(linkClaimsPress(0, isOverSpan(spans, 10))).toBe(true);
	});

	it("keeps the right button on the link menu and the middle button with the app", () => {
		expect(linkClaimsPress(2, true)).toBe(true);
		expect(linkClaimsPress(1, true)).toBe(false);
	});

	it("forwards a press beside the span, so the app keeps every other click", () => {
		expect(isOverSpan(spans, 5)).toBe(false);
		expect(isOverSpan(spans, 18)).toBe(false);
		expect(isOverSpan(undefined, 10)).toBe(false);
		expect(linkClaimsPress(0, isOverSpan(spans, 30))).toBe(false);
	});

	it("probes the hover target while the app has mouse reporting on", () => {
		const source = readFileSync(join(process.cwd(), "src/components/Terminal/CanvasTerminal.tsx"), "utf8");
		const start = source.indexOf("const onMouseMove = (e: MouseEvent) => {");
		const end = source.indexOf("if (currentFrame.mouseMode >= 3) {", start);
		expect(start).toBeGreaterThan(-1);
		expect(end).toBeGreaterThan(start);
		expect(source.slice(start, end)).toMatch(/scheduleLinkProbe\(e\)/);
	});
});
