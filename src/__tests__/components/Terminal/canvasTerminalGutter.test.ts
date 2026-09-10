import { describe, expect, it } from "vitest";
import { canToggleFold, gutterMarkKind, gutterZoneAt } from "../../../components/Terminal/canvasTerminalGutter";

describe("gutterMarkKind", () => {
	it("returns null while the block hasn't closed (or has no exit-code source)", () => {
		expect(gutterMarkKind({ exitCode: null })).toBeNull();
	});

	it("returns 'success' for exit code 0", () => {
		expect(gutterMarkKind({ exitCode: 0 })).toBe("success");
	});

	it("returns 'failure' for any non-zero exit code", () => {
		expect(gutterMarkKind({ exitCode: 1 })).toBe("failure");
		expect(gutterMarkKind({ exitCode: 127 })).toBe("failure");
		expect(gutterMarkKind({ exitCode: -1 })).toBe("failure");
	});
});

describe("gutterZoneAt", () => {
	it("resolves to 'fold' on the header row (executionLine when set)", () => {
		expect(gutterZoneAt(7, { promptLine: 5, executionLine: 7 })).toBe("fold");
	});

	it("resolves to 'fold' on promptLine when executionLine is still null", () => {
		expect(gutterZoneAt(5, { promptLine: 5, executionLine: null })).toBe("fold");
	});

	it("resolves to 'copy' for every other row in the block's gutter run", () => {
		expect(gutterZoneAt(8, { promptLine: 5, executionLine: 7 })).toBe("copy");
		expect(gutterZoneAt(20, { promptLine: 5, executionLine: 7 })).toBe("copy");
	});

	it("resolves to 'copy' on promptLine itself once executionLine has advanced past it", () => {
		// A multi-line typed command: promptLine is the "$ " row, executionLine is
		// where the command actually ran — the chevron/fold gesture lives on the
		// latter, so promptLine reverts to a normal copy row.
		expect(gutterZoneAt(5, { promptLine: 5, executionLine: 7 })).toBe("copy");
	});
});

describe("canToggleFold", () => {
	it("always allows unfolding, even with nothing foldable", () => {
		expect(canToggleFold(true, false)).toBe(true);
	});

	it("allows folding on when the block has foldable content", () => {
		expect(canToggleFold(false, true)).toBe(true);
	});

	it("refuses to fold on a block with nothing to fold (e.g. still running)", () => {
		// Code-review follow-up on issues #1/#5: clicking a still-running command's
		// header row must not silently pre-fold it before it ever closes.
		expect(canToggleFold(false, false)).toBe(false);
	});
});
