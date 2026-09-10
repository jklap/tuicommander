import { describe, expect, it } from "vitest";
import { ALL_SMART_PLACEMENTS, SMART_PLACEMENT_INFO } from "../../data/smartPlacementLabels";

describe("smartPlacementLabels", () => {
	it("gives every placement a non-empty label and hint", () => {
		for (const placement of ALL_SMART_PLACEMENTS) {
			const info = SMART_PLACEMENT_INFO[placement];
			expect(info, `missing SMART_PLACEMENT_INFO entry for "${placement}"`).toBeTruthy();
			expect(info.label.length).toBeGreaterThan(0);
			expect(info.hint.length).toBeGreaterThan(0);
		}
	});

	it("covers all 8 known SmartPlacement values", () => {
		expect(ALL_SMART_PLACEMENTS).toEqual([
			"toolbar",
			"git-changes",
			"git-branches",
			"pr-popover",
			"issue-popover",
			"terminal-context",
			"command-palette",
			"file-context",
		]);
	});

	it("is derived from SMART_PLACEMENT_INFO's keys, so it can't drift out of sync with the SmartPlacement union", () => {
		// A future 9th SmartPlacement value forces a compile error on
		// SMART_PLACEMENT_INFO (a Record covering the whole union) before this
		// list could ever go stale — this test just documents/locks the
		// derivation itself, not the compile-time guarantee.
		expect(ALL_SMART_PLACEMENTS).toEqual(Object.keys(SMART_PLACEMENT_INFO));
	});
});
