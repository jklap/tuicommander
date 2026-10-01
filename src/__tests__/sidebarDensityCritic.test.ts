import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { countSidebarRows } from "../utils/sidebarDensity";

// Critic cases for #1334-b659 round 1.

describe("countSidebarRows (critic)", () => {
	// Catches: the 16-row budget ignoring the terminal-tab rows nested under an expanded
	// branch (BranchTabList), so one repo with 20 open agents counts as 2 rows and stays "comfortable".
	it("counts the nested terminal rows of an expanded branch", () => {
		const terminals = Array.from({ length: 20 }, (_, i) => `t${i}`);
		const rows = countSidebarRows(
			[{ expanded: true, collapsed: false, workspaces: { main: { terminals, tabsCollapsed: false } } }],
			true,
		);
		expect(rows).toBeGreaterThan(16);
	});
});

describe("touch density CSS cascade (critic)", () => {
	const css = readFileSync(resolve(__dirname, "../components/Sidebar/Sidebar.module.css"), "utf8");

	/** Brace depth at `index`: 0 = top level, >=1 = inside an at-rule/block. */
	const depthAt = (index: number) => {
		let depth = 0;
		for (let i = 0; i < index; i++) {
			if (css[i] === "{") depth++;
			else if (css[i] === "}") depth--;
		}
		return depth;
	};

	// Catches: a top-level `.sidebar[data-density="touch"] .branchActions { max-width: 84px }`.
	// It has the same specificity as `.branchItem:hover .branchActions { max-width: 44px }` but comes
	// earlier, so on a hover-capable pointer (forced touch mode, iPad trackpad) the idle actions
	// reserve 84px in every row and on hover are clipped back to 44px, cutting off the second 36px button.
	it("does not set branchActions max-width outside a hover media query", () => {
		const at = css.indexOf('.sidebar[data-density="touch"] .branchActions');
		expect(at).toBeGreaterThan(-1);
		expect(depthAt(at)).toBeGreaterThanOrEqual(1);
	});
});
