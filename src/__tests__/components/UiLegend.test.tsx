import { render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import { UiLegend } from "../../components/HelpPanel/UiLegend";

/**
 * The legend explains what a branch row shows, so it renders the same
 * component rather than a hand-drawn copy: a copy kept showing filled pills
 * after the sidebar moved to compact markers.
 */
describe("UiLegend PR markers", () => {
	it("renders every PR state with the sidebar's compact marker", () => {
		const { container } = render(() => <UiLegend />);
		const badges = container.querySelectorAll(".prBadge.prBadgeCompact");

		expect(badges.length).toBe(10);
		expect(container.querySelector(".prMarkConflict")?.getAttribute("data-tooltip")).toBe("PR #42 · Conflicts");
		expect(container.querySelectorAll(".prMarkPending").length).toBe(2);
	});
});

/**
 * Every marker the sidebar or toolbar renders must be explained. Add a marker
 * there, add its legend entry here: the row is keyed by what the user sees.
 */
describe("UiLegend branch markers", () => {
	it("explains the unmerged-commits marker with the sidebar's own component", () => {
		const { container } = render(() => <UiLegend />);
		const marker = container.querySelector(".branchUnmergedMarker");

		expect(marker?.getAttribute("aria-label")).toBe("Unmerged commits");
		expect(container.textContent).toContain("Not a dirty worktree");
	});

	it("explains the toolbar ahead and behind counts", () => {
		const { container } = render(() => <UiLegend />);
		const text = container.textContent ?? "";

		expect(text).toContain("↑N");
		expect(text).toContain("↓N");
		expect(text).toContain("Absent without an upstream");
	});
});
