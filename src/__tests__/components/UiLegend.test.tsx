import { fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { UiLegend } from "../../components/HelpPanel/UiLegend";
import { settingsStore } from "../../stores/settings";

describe("UiLegend", () => {
	it("renders from the registry — six section headings, no more hand-maintained 'Panels' section", () => {
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
			"PR Status Badges",
			"Git Repo Status",
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

	it("lists sidebar symbols, including the previously-undocumented 'other branch' and 'no open terminal' rows", () => {
		const { container } = render(() => <UiLegend />);
		const text = container.textContent ?? "";
		for (const label of ["Main branch", "Linked worktree", "Other branch", "Shell (non-git)", "No open terminal"]) {
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
			["Open PR", "Open PR (number)"],
			["Ready", "Approved and mergeable"],
			["Draft", "PR is a draft"],
			["Conflicts", "Merge conflicts"],
			["Checking", "GitHub is recomputing mergeability"],
			["CI Failed", "CI checks failed"],
			["Changes Req.", "Changes requested"],
			["Review Req.", "Awaiting review"],
			["CI Running", "CI in progress"],
			["Merged", "PR merged"],
			["Closed", "PR closed without merging"],
		]) {
			expect(text).toContain(label);
			expect(text).toContain(description);
		}
	});

	it("gives conflict, checking, and ci-pending PR badges their own --ind-anim-pr-* animation", () => {
		const { container } = render(() => <UiLegend />);
		const badges = Array.from(container.querySelectorAll(".badge")) as HTMLElement[];
		for (const suffix of ["conflict", "checking", "ci-pending"]) {
			const match = badges.find((el) => el.style.animation.includes(`--ind-anim-pr-${suffix}`));
			expect(match, `no badge animating --ind-anim-pr-${suffix}`).toBeTruthy();
		}
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
		expect(container.querySelectorAll(".previewBtn").length).toBe(0);
		expect(container.querySelector(".resetAllBtn")).toBeNull();
	});

	it("wraps every row's preview in one clickable button when editable — no more separate per-capability buttons", () => {
		const { container } = render(() => <UiLegend editable />);
		// terminalStatus(7) + tabType(6) + sidebarSymbol(7) + prBadge(11) + gitState(6) + diffStat(2) = 39
		expect(container.querySelectorAll(".previewBtn").length).toBe(39);
		expect(container.querySelectorAll(".editSwatch").length).toBe(0);
		expect(container.querySelectorAll(".editIconBtn").length).toBe(0);
		expect(container.querySelectorAll(".editAnimBtn").length).toBe(0);
	});

	it("shows the 'Reset all indicators' button when editable", () => {
		const { getByText } = render(() => <UiLegend editable />);
		expect(getByText("Reset all indicators")).toBeTruthy();
	});

	it("clicking a row's preview opens the combined dialog with that indicator's label as the title", () => {
		const { container, getByRole } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		fireEvent.click(busyRow?.querySelector(".previewBtn") as HTMLButtonElement);

		expect(getByRole("heading", { name: "Busy" })).toBeTruthy();
	});

	it("clicking a preset in the opened combined dialog calls setIndicatorColor with that indicator's id", () => {
		const { container, getByTitle } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		fireEvent.click(busyRow?.querySelector(".previewBtn") as HTMLButtonElement);

		fireEvent.click(getByTitle("Blue"));

		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "terminal.busy", color: "#4A9EFF" }]);
	});

	it("clicking 'No color' in the opened dialog clears only the color, not a sibling icon override", () => {
		settingsStore.setIndicatorColor("terminal.busy", "#ff00ff");
		settingsStore.setIndicatorIcon("terminal.busy", "ring");
		const { container, getByTitle } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		fireEvent.click(busyRow?.querySelector(".previewBtn") as HTMLButtonElement);

		fireEvent.click(getByTitle("No color"));

		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "terminal.busy", icon: "ring" }]);
	});

	it("the combined dialog for the icon-only 'shell' row shows an icon grid but no color swatches or animation list", () => {
		const { container, getByTitle } = render(() => <UiLegend editable />);
		const shellRow = Array.from(container.querySelectorAll(".row")).find((row) =>
			row.textContent?.includes("Shell (non-git)"),
		);
		fireEvent.click(shellRow?.querySelector(".previewBtn") as HTMLButtonElement);

		expect(getByTitle("shellTerminal")).toBeTruthy();
		expect(container.querySelector(".colorSwatch")).toBeNull();
		expect(container.querySelectorAll(".list .row").length).toBe(0);
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

	it("clicking an icon in the opened combined dialog calls setIndicatorIcon with that indicator's id", () => {
		const { container, getByTitle } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		fireEvent.click(busyRow?.querySelector(".previewBtn") as HTMLButtonElement);

		fireEvent.click(getByTitle("ring"));

		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "terminal.busy", icon: "ring" }]);
	});

	it("clicking an option in the opened combined dialog calls setIndicatorAnimation with that indicator's id", () => {
		const { container, getByText } = render(() => <UiLegend editable />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		fireEvent.click(busyRow?.querySelector(".previewBtn") as HTMLButtonElement);

		fireEvent.click(getByText("Blink"));

		expect(settingsStore.state.indicatorOverrides).toEqual([{ id: "terminal.busy", animation: "blink" }]);
	});

	it("restricts the combined dialog's animation list to a badge entry's narrower set (no Glow/Spin for pr.conflict)", () => {
		const { container, queryByText } = render(() => <UiLegend editable />);
		const conflictRow = Array.from(container.querySelectorAll(".row")).find((row) =>
			row.textContent?.includes("Conflicts"),
		);
		fireEvent.click(conflictRow?.querySelector(".previewBtn") as HTMLButtonElement);

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

	it("reflects an icon override in the row's own left-hand preview, not just the picker", () => {
		// Regression guard: IndicatorPreview previously rendered entry.defaultIconId
		// verbatim instead of resolving the override, so changing an indicator's icon
		// left the legend's own preview showing the old shape.
		settingsStore.setIndicatorIcon("terminal.busy", "diamond");
		const { container } = render(() => <UiLegend />);
		const busyRow = Array.from(container.querySelectorAll(".row")).find((row) => row.textContent?.includes("Busy"));
		const path = busyRow?.querySelector(".previewIcon path");
		expect(path?.getAttribute("d")).toBe("M8 1.2 14.8 8 8 14.8 1.2 8z");
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
