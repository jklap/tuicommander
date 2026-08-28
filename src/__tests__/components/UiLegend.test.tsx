import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { PR_BADGE_LEGEND, SIDEBAR_SYMBOL_LEGEND, UiLegend } from "../../components/HelpPanel/UiLegend";
import { PR_STATE_LABELS, prBadgeKind } from "../../components/Sidebar/PrStateBadge";
import { BRANCH_ICON_SHAPES, branchIconShape } from "../../components/Sidebar/RepoSection";
import { getIndicator, INDICATORS } from "../../indicators/registry";
import { settingsStore } from "../../stores/settings";

/** The legend row whose label cell reads exactly `label`. */
function rowLabelled(container: HTMLElement, label: string): Element | undefined {
	return Array.from(container.querySelectorAll(".row")).find(
		(row) => row.querySelector(".label")?.textContent === label,
	);
}

/**
 * The legend explains what a branch row shows, so it renders the same
 * component rather than a hand-drawn copy: a copy kept showing filled pills
 * after the sidebar moved to compact markers.
 */
describe("UiLegend PR markers", () => {
	it("renders every PR state with the sidebar's compact marker", () => {
		const { container } = render(() => <UiLegend />);
		const badges = container.querySelectorAll(".prBadge.prBadgeCompact");

		expect(badges.length).toBe(12);
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

/**
 * Catches a marker shipped without a legend entry (#1315: the unmerged marker
 * and toolbar counts had none; the PR "closed" state still had none). The kinds
 * come from the components that define them, so adding a PR state or an icon
 * shape without a legend row fails here.
 */
describe("UiLegend covers every marker kind the sidebar can render", () => {
	it("has a PR badge entry for every PR state", () => {
		const legendKinds = new Set(PR_BADGE_LEGEND.map((e) => prBadgeKind(e.badge)));
		expect(Object.keys(PR_STATE_LABELS).filter((k) => !legendKinds.has(k as never))).toEqual([]);
	});

	it("has a sidebar icon entry for every branch icon shape", () => {
		const legendShapes = new Set(SIDEBAR_SYMBOL_LEGEND.map((e) => branchIconShape(e.icon)));
		expect(BRANCH_ICON_SHAPES.filter((k) => !legendShapes.has(k))).toEqual([]);
	});
});

/**
 * The editor (Settings > Appearance) recolors PR states through the registry, so
 * every legend PR row must be wired to the registry entry for the state its real
 * marker shows — otherwise editing "Checking" would recolor some other marker.
 */
describe("UiLegend PR rows are wired to the matching registry entry", () => {
	it("each PR legend row edits pr.<the kind its badge renders>", () => {
		for (const entry of PR_BADGE_LEGEND) {
			expect(entry.indicatorId).toBe(`pr.${prBadgeKind(entry.badge)}`);
			expect(getIndicator(entry.indicatorId), entry.indicatorId).toBeTruthy();
		}
	});

	it("each sidebar symbol row's indicatorId exists in the registry's sidebar group", () => {
		for (const entry of SIDEBAR_SYMBOL_LEGEND) {
			if (!entry.indicatorId) continue;
			expect(getIndicator(entry.indicatorId)?.group).toBe("sidebarSymbol");
		}
	});
});

describe("UiLegend", () => {
	it("renders the section headings in order, with no hand-maintained 'Panels' section", () => {
		// The old "Panels" section documented 4 accent colors that no panel
		// component actually applies (verified by grepping every .css for the
		// --tab-*-rgb vars during the customization plan's exploration) — it
		// described behavior that didn't exist and is gone now that the legend
		// renders from indicators/registry.ts instead of a hand-maintained copy.
		const { container } = render(() => <UiLegend />);
		const headings = Array.from(container.querySelectorAll("label")).map((l) => l.textContent);
		expect(headings).toEqual([
			"Terminal Status Dots",
			"Tab Types",
			"Sidebar Symbols",
			"Branch Markers",
			"PR Status Badges",
			"Git Repo Status",
			"Toolbar Branch Counts",
			"Diff Stats",
		]);
	});

	it("lists every terminal status dot state with its label and description", () => {
		const { container } = render(() => <UiLegend />);
		const text = container.textContent ?? "";
		for (const [label, description] of [
			["No session", "Terminal never ran or was reset"],
			["Busy", "Producing output"],
			["Idle", "Agent waiting, no recent output"],
			["Unseen", "Went idle while not viewed"],
			["Exited", "Shell process exited"],
			["Question", "Agent needs input"],
			["Error", "API error or agent stuck"],
		]) {
			expect(text).toContain(label);
			expect(text).toContain(description);
		}
	});

	it("gives busy, question, and error terminal dots an animation referencing their own --ind-anim-* var", () => {
		const { container } = render(() => <UiLegend />);
		const icons = Array.from(container.querySelectorAll(".previewIcon")) as HTMLElement[];
		for (const suffix of ["busy", "question", "error"]) {
			const match = icons.find((el) => el.style.animation.includes(`--ind-anim-terminal-${suffix}`));
			expect(match, `no previewIcon animating --ind-anim-terminal-${suffix}`).toBeTruthy();
		}
	});

	it('gives idle and unseen their own --ind-anim-terminal-* var too (defaults to "none", but independently overridable)', () => {
		const { container } = render(() => <UiLegend />);
		const icons = Array.from(container.querySelectorAll(".previewIcon")) as HTMLElement[];
		for (const suffix of ["idle", "unseen"]) {
			const match = icons.find((el) => el.style.animation.includes(`--ind-anim-terminal-${suffix}`));
			expect(match, `no previewIcon referencing --ind-anim-terminal-${suffix}`).toBeTruthy();
		}
	});

	it("lists every tab type, including the previously-orphaned HTML Preview type", () => {
		const { container } = render(() => <UiLegend />);
		const text = container.textContent ?? "";
		for (const label of ["Diff", "Editor", "Markdown", "Panel", "HTML Preview", "PTY"]) {
			expect(text).toContain(label);
		}
	});

	it("lists sidebar symbols, including the 'feature branch' and 'idle' rows and the merged/remote badges", () => {
		const { container } = render(() => <UiLegend />);
		const text = container.textContent ?? "";
		for (const label of [
			"Main branch",
			"Feature branch",
			"Worktree",
			"Shell",
			"Idle",
			"Merged badge",
			"Remote badge",
		]) {
			expect(text).toContain(label);
		}
	});

	it("renders real IndicatorIcon shapes for sidebar symbols, not the old text glyphs", () => {
		const { container } = render(() => <UiLegend />);
		// The old legend rendered "✱" and "⎇" — literal characters the sidebar
		// hasn't rendered in a while (RepoSection.tsx uses inline SVG paths).
		expect(container.textContent).not.toContain("✱");
		expect(container.textContent).not.toContain("⎇");
		expect(container.querySelectorAll(".previewIcon").length).toBeGreaterThan(0);
	});

	it("lists every PR badge state with its label and description, including previously-missing Closed and Checking", () => {
		const { container } = render(() => <UiLegend />);
		const text = container.textContent ?? "";
		for (const [label, description] of [
			["Open", "Open PR"],
			["Ready", "Approved and mergeable"],
			["Draft", "PR is a draft"],
			["Conflicts", "Merge conflicts"],
			["Checking", "GitHub is recomputing mergeability"],
			["CI Failed", "CI checks failed"],
			["Changes Req.", "Changes requested"],
			["Comments", "Unresolved review threads"],
			["Review", "Awaiting review"],
			["CI Running", "CI in progress"],
			["Merged", "PR merged"],
			["Closed", "PR closed without merging"],
		]) {
			expect(text).toContain(label);
			expect(text).toContain(description);
		}
	});

	it("gives Checking its own marker modifier so it is customizable apart from CI Running", () => {
		// The animation itself lives in Sidebar.module.css (.prMarkPending.prMarkChecking .prMark →
		// --ind-anim-pr-checking), pinned by registryParity.test.ts — here, only that the legend's
		// real marker carries the per-state class that rule hangs on.
		const { container } = render(() => <UiLegend />);
		expect(container.querySelectorAll(".prMarkPending.prMarkChecking").length).toBe(1);
		expect(container.querySelectorAll(".prMarkReview.prMarkComments").length).toBe(1);
		expect(container.querySelectorAll(".prMarkOpen.prMarkReady").length).toBe(1);
		expect(container.querySelectorAll(".prMarkClosed.prMarkCiFailed").length).toBe(1);
	});

	it("lists diff stat symbols", () => {
		const { container } = render(() => <UiLegend />);
		const text = container.textContent ?? "";
		expect(text).toContain("Additions");
		expect(text).toContain("Deletions");
	});
});

describe("UiLegend editable mode (Settings → Appearance)", () => {
	afterEach(() => {
		settingsStore.resetAllIndicators();
		settingsStore.setShowDiffStats(true);
		settingsStore.setShowPrBadges(true);
		settingsStore.setShowGitState(true);
		settingsStore.setTabTypeHighlighting(true);
	});

	it("shows no edit controls when not editable (HelpPanel's read-only reference view)", () => {
		const { container } = render(() => <UiLegend />);
		expect(container.querySelectorAll(".editSwatch").length).toBe(0);
		expect(container.querySelector(".resetAllBtn")).toBeNull();
	});

	it("shows an edit swatch for every color-capable row when editable", () => {
		const { container } = render(() => <UiLegend editable />);
		// terminalStatus(7) + tabType(6) + sidebarSymbol color-capable(6, all but
		// "shell") + prBadge(12) + gitState(6) + diffStat(2) = 39
		expect(container.querySelectorAll(".editSwatch").length).toBe(39);
	});

	it("gives every registry entry exactly one editor row, so no override is left without UI", () => {
		const { container } = render(() => <UiLegend editable />);
		const editorRows = Array.from(container.querySelectorAll(".row")).filter((row) =>
			row.querySelector(".editSwatch, .editIconBtn, .editAnimBtn"),
		);
		expect(editorRows.length).toBe(INDICATORS.length);
	});

	it("puts the PR color editor on the real compact marker row of the same state", () => {
		const { container } = render(() => <UiLegend editable />);
		const checkingRow = rowLabelled(container, "Checking");
		expect(checkingRow?.querySelector(".prBadgeCompact.prMarkChecking")).not.toBeNull();
		expect(checkingRow?.querySelector(".editSwatch")).not.toBeNull();
		expect(checkingRow?.querySelector(".editAnimBtn")).not.toBeNull();
	});

	it("does not show an edit swatch for an icon-only entry (sidebar.shell has no color)", () => {
		const { container } = render(() => <UiLegend editable />);
		const shellRow = rowLabelled(container, "Shell");
		expect(shellRow?.querySelector(".editSwatch")).toBeNull();
	});

	it("shows the 'Reset all indicators' button when editable", () => {
		const { getByText } = render(() => <UiLegend editable />);
		expect(getByText("Reset all indicators")).toBeTruthy();
	});

	it("clicking a preset in the opened color picker calls setIndicatorColor with that indicator's id", () => {
		const { container, getByTitle } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		const swatch = busyRow?.querySelector(".editSwatch") as HTMLButtonElement;
		fireEvent.click(swatch);

		fireEvent.click(getByTitle("Blue"));

		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "terminal.busy", color: "#4A9EFF" }]);
	});

	it("clicking 'No color' in the opened picker clears the override instead of setting an empty color", () => {
		settingsStore.setIndicatorColor("terminal.busy", "#ff00ff");
		const { container, getByTitle } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		fireEvent.click(busyRow?.querySelector(".editSwatch") as HTMLButtonElement);

		fireEvent.click(getByTitle("No color"));

		expect(settingsStore.state.indicatorOverrides).toEqual([]);
	});

	it("shows a reset '×' only for a row with an active override", () => {
		settingsStore.setIndicatorColor("terminal.busy", "#ff00ff");
		const { container } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		const idleRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Idle"));

		expect(busyRow?.querySelector(".resetSwatch")).not.toBeNull();
		expect(idleRow?.querySelector(".resetSwatch")).toBeNull();
	});

	it("clicking the reset '×' clears exactly that indicator's override", () => {
		settingsStore.setIndicatorColor("terminal.busy", "#ff00ff");
		settingsStore.setIndicatorColor("pr.conflict", "#00ff00");
		const { container } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		fireEvent.click(busyRow?.querySelector(".resetSwatch") as HTMLButtonElement);

		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "pr.conflict", color: "#00ff00" }]);
	});

	it("clicking 'Reset all indicators' clears every override", () => {
		settingsStore.setIndicatorColor("terminal.busy", "#ff00ff");
		settingsStore.setIndicatorColor("pr.conflict", "#00ff00");
		const { getByText } = render(() => <UiLegend editable />);
		fireEvent.click(getByText("Reset all indicators"));

		expect(settingsStore.state.indicatorOverrides).toEqual([]);
	});

	it("shows an icon-edit button for the icon-only 'shell' row but no color swatch or animation button", () => {
		const { container } = render(() => <UiLegend editable />);
		const shellRow = rowLabelled(container, "Shell");
		expect(shellRow?.querySelector(".editIconBtn")).not.toBeNull();
		expect(shellRow?.querySelector(".editSwatch")).toBeNull();
		expect(shellRow?.querySelector(".editAnimBtn")).toBeNull();
	});

	it("clicking an icon in the opened icon picker calls setIndicatorIcon with that indicator's id", () => {
		const { container, getByTitle } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		fireEvent.click(busyRow?.querySelector(".editIconBtn") as HTMLButtonElement);

		fireEvent.click(getByTitle("ring"));

		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "terminal.busy", icon: "ring" }]);
	});

	it("clicking an option in the opened animation picker calls setIndicatorAnimation with that indicator's id", () => {
		const { container, getByText } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		fireEvent.click(busyRow?.querySelector(".editAnimBtn") as HTMLButtonElement);

		fireEvent.click(getByText("Blink"));

		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "terminal.busy", animation: "blink" }]);
	});

	it("restricts the animation picker to a badge entry's narrower animations list (no Glow/Spin for pr.conflict)", () => {
		const { container, queryByText } = render(() => <UiLegend editable />);
		const conflictRow = Array.from(container.querySelectorAll(".row")).find((row) =>
			row.textContent?.includes("Conflicts"),
		);
		fireEvent.click(conflictRow?.querySelector(".editAnimBtn") as HTMLButtonElement);

		expect(queryByText("Pulse")).toBeTruthy();
		expect(queryByText("Glow")).toBeNull();
		expect(queryByText("Spin")).toBeNull();
	});

	it("shows a reset '×' for an icon-only override (not just a color override)", () => {
		settingsStore.setIndicatorIcon("terminal.busy", "ring");
		const { container } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		expect(busyRow?.querySelector(".resetSwatch")).not.toBeNull();
	});

	it("shows no group-header toggles when not editable", () => {
		const { getByText } = render(() => <UiLegend />);
		const tabTypesHeading = getByText("Tab Types");
		const group = tabTypesHeading.parentElement!;
		expect(group.querySelector('input[type="checkbox"]')).toBeNull();
	});

	it("shows a group-header toggle for Tab Types, PR Status Badges, Git Repo Status, and Diff Stats, but not the others", () => {
		const { getByText } = render(() => <UiLegend editable />);
		const togglesFor = (heading: string) => {
			const group = getByText(heading).parentElement!;
			return group.querySelectorAll('input[type="checkbox"]').length;
		};
		expect(togglesFor("Terminal Status Dots")).toBe(0);
		expect(togglesFor("Tab Types")).toBe(1);
		expect(togglesFor("Sidebar Symbols")).toBe(0);
		expect(togglesFor("PR Status Badges")).toBe(1);
		expect(togglesFor("Git Repo Status")).toBe(1);
		expect(togglesFor("Diff Stats")).toBe(1);
	});

	it("clicking the Tab Types toggle calls setTabTypeHighlighting", () => {
		const { getByText } = render(() => <UiLegend editable />);
		const group = getByText("Tab Types").parentElement!;
		const checkbox = group.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.click(checkbox);
		expect(settingsStore.state.tabTypeHighlighting).toBe(false);
	});

	it("clicking the PR Status Badges toggle calls setShowPrBadges", () => {
		const { getByText } = render(() => <UiLegend editable />);
		const group = getByText("PR Status Badges").parentElement!;
		const checkbox = group.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.click(checkbox);
		expect(settingsStore.state.showPrBadges).toBe(false);
	});

	it("clicking the Git Repo Status toggle calls setShowGitState", () => {
		const { getByText } = render(() => <UiLegend editable />);
		const group = getByText("Git Repo Status").parentElement!;
		const checkbox = group.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.click(checkbox);
		expect(settingsStore.state.showGitState).toBe(false);
	});

	it("clicking the Diff Stats toggle calls setShowDiffStats", () => {
		const { getByText } = render(() => <UiLegend editable />);
		const group = getByText("Diff Stats").parentElement!;
		const checkbox = group.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.click(checkbox);
		expect(settingsStore.state.showDiffStats).toBe(false);
	});

	it("the Git Repo Status group lists every in-progress-operation kind plus conflicts", () => {
		const { container } = render(() => <UiLegend />);
		const text = container.textContent ?? "";
		for (const [label, description] of [
			["Rebasing", "Rebase in progress"],
			["Merging", "Merge in progress"],
			["Cherry-picking", "Cherry-pick in progress"],
			["Reverting", "Revert in progress"],
			["Bisecting", "Bisect in progress"],
			["Conflicts", "Unmerged files in the working tree"],
		]) {
			expect(text).toContain(label);
			expect(text).toContain(description);
		}
	});
});
