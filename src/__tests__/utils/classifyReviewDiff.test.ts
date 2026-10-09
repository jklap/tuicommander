import { describe, expect, it } from "vitest";
import { classifyFingerprintedDiff, classifyNewSteps } from "../../utils/classifyReviewDiff";

describe("classifyFingerprintedDiff", () => {
	it("reports a key absent from prev as new", () => {
		const prev = [{ key: "a", fingerprint: "1" }];
		const next = [
			{ key: "a", fingerprint: "1" },
			{ key: "b", fingerprint: "1" },
		];
		const result = classifyFingerprintedDiff(prev, next, new Set());
		expect(result.newKeys).toEqual(["b"]);
		expect(result.changedVisible).toEqual([]);
		expect(result.changedHidden).toEqual([]);
	});

	it("reports a changed-fingerprint key that is visible as changedVisible", () => {
		const prev = [{ key: "a", fingerprint: "1" }];
		const next = [{ key: "a", fingerprint: "2" }];
		const result = classifyFingerprintedDiff(prev, next, new Set(["a"]));
		expect(result.changedVisible).toEqual(["a"]);
		expect(result.changedHidden).toEqual([]);
		expect(result.newKeys).toEqual([]);
	});

	it("reports a changed-fingerprint key that is NOT visible as changedHidden", () => {
		const prev = [{ key: "a", fingerprint: "1" }];
		const next = [{ key: "a", fingerprint: "2" }];
		const result = classifyFingerprintedDiff(prev, next, new Set());
		expect(result.changedHidden).toEqual(["a"]);
		expect(result.changedVisible).toEqual([]);
	});

	it("reports nothing for a key whose fingerprint is unchanged", () => {
		const prev = [{ key: "a", fingerprint: "1" }];
		const next = [{ key: "a", fingerprint: "1" }];
		const result = classifyFingerprintedDiff(prev, next, new Set(["a"]));
		expect(result.newKeys).toEqual([]);
		expect(result.changedVisible).toEqual([]);
		expect(result.changedHidden).toEqual([]);
	});

	it("handles a mix of new, changed-visible, changed-hidden, and unchanged in one call", () => {
		const prev = [
			{ key: "a", fingerprint: "1" },
			{ key: "b", fingerprint: "1" },
			{ key: "c", fingerprint: "1" },
		];
		const next = [
			{ key: "a", fingerprint: "1" }, // unchanged
			{ key: "b", fingerprint: "2" }, // changed, visible
			{ key: "c", fingerprint: "2" }, // changed, hidden
			{ key: "d", fingerprint: "1" }, // new
		];
		const result = classifyFingerprintedDiff(prev, next, new Set(["a", "b"]));
		expect(result.newKeys).toEqual(["d"]);
		expect(result.changedVisible).toEqual(["b"]);
		expect(result.changedHidden).toEqual(["c"]);
	});

	it("returns all-empty for two identical empty lists", () => {
		const result = classifyFingerprintedDiff([], [], new Set());
		expect(result).toEqual({ newKeys: [], changedVisible: [], changedHidden: [] });
	});

	it("does not report a key removed in next (removal is the caller's own concern)", () => {
		const prev = [{ key: "a", fingerprint: "1" }];
		const result = classifyFingerprintedDiff(prev, [], new Set(["a"]));
		expect(result).toEqual({ newKeys: [], changedVisible: [], changedHidden: [] });
	});
});

describe("classifyNewSteps", () => {
	it("returns steps in next whose tool_use_id is not in prev, preserving next's order", () => {
		const prev = [{ tool_use_id: "t1" }];
		const next = [{ tool_use_id: "t1" }, { tool_use_id: "t2" }, { tool_use_id: "t3" }];
		expect(classifyNewSteps(prev, next)).toEqual([{ tool_use_id: "t2" }, { tool_use_id: "t3" }]);
	});

	it("returns an empty array when nothing is new", () => {
		const steps = [{ tool_use_id: "t1" }];
		expect(classifyNewSteps(steps, steps)).toEqual([]);
	});

	it("returns everything when prev is empty", () => {
		const next = [{ tool_use_id: "t1" }, { tool_use_id: "t2" }];
		expect(classifyNewSteps([], next)).toEqual(next);
	});
});
