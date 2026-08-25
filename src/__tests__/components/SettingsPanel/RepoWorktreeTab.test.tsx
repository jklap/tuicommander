import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

const { mockIsMacOS } = vi.hoisted(() => ({ mockIsMacOS: vi.fn().mockReturnValue(false) }));

vi.mock("../../../platform", () => ({ isMacOS: mockIsMacOS }));

vi.mock("../../../stores/settings", () => ({
	settingsStore: {
		state: { prHideDrafts: false, prHideConflicting: true, prHideCiFailing: false },
	},
}));

import { RepoWorktreeTab } from "../../../components/SettingsPanel/tabs/RepoWorktreeTab";
import type { RepoDefaults } from "../../../stores/repoDefaults";
import type { RepoSettings } from "../../../stores/repoSettings";

function makeSettings(overrides: Partial<RepoSettings> = {}): RepoSettings {
	return {
		path: "/repo",
		displayName: "my-repo",
		autoConsolidateWorktrees: false,
		baseBranch: null,
		copyIgnoredFiles: null,
		copyUntrackedFiles: null,
		setupScript: null,
		runScript: null,
		archiveScript: null,
		color: "#58a6ff",
		terminalMetaHotkeys: null,
		worktreeStorage: null,
		promptOnCreate: null,
		deleteBranchOnRemove: null,
		autoArchiveMerged: null,
		orphanCleanup: null,
		prMergeStrategy: null,
		afterMerge: null,
		autoFetchIntervalMinutes: null,
		autoDeleteOnPrClose: null,
		mcpUpstreams: null,
		prHideDrafts: null,
		prHideConflicting: null,
		prHideCiFailing: null,
		branchLabels: {},
		...overrides,
	};
}

const defaults: RepoDefaults = {
	baseBranch: "main",
	copyIgnoredFiles: true,
	copyUntrackedFiles: false,
	setupScript: "",
	runScript: "",
	archiveScript: "",
	worktreeStorage: "sibling",
	promptOnCreate: true,
	deleteBranchOnRemove: false,
	autoArchiveMerged: true,
	orphanCleanup: "ask",
	prMergeStrategy: "merge",
	afterMerge: "ask",
	autoFetchIntervalMinutes: 15,
	autoDeleteOnPrClose: "off",
};

describe("RepoWorktreeTab", () => {
	let onUpdate: <K extends keyof RepoSettings>(key: K, value: RepoSettings[K]) => void;

	beforeEach(() => {
		onUpdate = vi.fn();
		mockIsMacOS.mockReturnValue(false);
	});

	function selectByOptionValue(container: HTMLElement, optionValue: string): HTMLSelectElement {
		const selects = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
		return selects.find((sel) => Array.from(sel.options).some((o) => o.value === optionValue))!;
	}

	it("renders the Repository, Worktree Configuration, and Worktree Settings headings", () => {
		const { container } = render(() => (
			<RepoWorktreeTab settings={makeSettings()} defaults={defaults} onUpdate={onUpdate} />
		));
		const headings = Array.from(container.querySelectorAll("h3")).map((h) => h.textContent);
		expect(headings).toEqual(["Repository", "Worktree Configuration", "Worktree Settings"]);
	});

	it("calls onUpdate when the display name input changes", () => {
		const { getByPlaceholderText } = render(() => (
			<RepoWorktreeTab settings={makeSettings()} defaults={defaults} onUpdate={onUpdate} />
		));
		fireEvent.input(getByPlaceholderText("Custom name..."), { target: { value: "backend-svc" } });
		expect(onUpdate).toHaveBeenCalledWith("displayName", "backend-svc");
	});

	it("shows the global-default option text interpolated with the actual default, and selects it when baseBranch is null", () => {
		const { container, getByText } = render(() => (
			<RepoWorktreeTab settings={makeSettings({ baseBranch: null })} defaults={defaults} onUpdate={onUpdate} />
		));
		expect(getByText("Use global default (main)")).toBeTruthy();
		const select = selectByOptionValue(container, "master");
		expect(select.value).toBe("__inherit__");
	});

	it("converts the inherit sentinel back to null when baseBranch is reset to global default", () => {
		const { container } = render(() => (
			<RepoWorktreeTab settings={makeSettings({ baseBranch: "master" })} defaults={defaults} onUpdate={onUpdate} />
		));
		const select = selectByOptionValue(container, "master");
		fireEvent.change(select, { target: { value: "__inherit__" } });
		expect(onUpdate).toHaveBeenCalledWith("baseBranch", null);
	});

	it("passes a concrete branch value through unchanged when baseBranch is overridden", () => {
		const { container } = render(() => (
			<RepoWorktreeTab settings={makeSettings({ baseBranch: null })} defaults={defaults} onUpdate={onUpdate} />
		));
		const select = selectByOptionValue(container, "master");
		fireEvent.change(select, { target: { value: "develop" } });
		expect(onUpdate).toHaveBeenCalledWith("baseBranch", "develop");
	});

	it("converts the auto-fetch-interval select value to a number, or null for the inherit option", () => {
		const { container } = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ autoFetchIntervalMinutes: null })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		const select = selectByOptionValue(container, "30");
		fireEvent.change(select, { target: { value: "30" } });
		expect(onUpdate).toHaveBeenCalledWith("autoFetchIntervalMinutes", 30);
	});

	/** Find the tri-state radiogroup for a given row label. */
	function triGroup(container: HTMLElement, label: string): HTMLElement {
		return container.querySelector(`[role="radiogroup"][aria-label="${label}"]`) as HTMLElement;
	}

	function triSegment(group: HTMLElement, kind: "off" | "global" | "on"): HTMLElement {
		return group.querySelector(`[data-kind="${kind}"]`) as HTMLElement;
	}

	/** The row's full text (segments + trailing label/hint), for hint assertions. */
	function triRowText(group: HTMLElement): string {
		return group.closest(".triToggle")?.textContent ?? "";
	}

	it("resolves copyIgnoredFiles/copyUntrackedFiles against the global default when null, and selecting On/Off overrides it", () => {
		const { container } = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ copyIgnoredFiles: null, copyUntrackedFiles: null })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		// copyIgnoredFiles inherits defaults.copyIgnoredFiles=true, copyUntrackedFiles inherits false.
		const ignoredGroup = triGroup(container, "Copy ignored files");
		const untrackedGroup = triGroup(container, "Copy untracked files");
		expect(triSegment(ignoredGroup, "global").getAttribute("aria-checked")).toBe("true");
		expect(triSegment(untrackedGroup, "global").getAttribute("aria-checked")).toBe("true");

		fireEvent.click(triSegment(ignoredGroup, "off"));
		expect(onUpdate).toHaveBeenCalledWith("copyIgnoredFiles", false);
	});

	it("shows the 'Use global default' hint only while the field is null, and clears it once overridden", () => {
		const nullCase = render(() => (
			<RepoWorktreeTab settings={makeSettings({ copyIgnoredFiles: null })} defaults={defaults} onUpdate={onUpdate} />
		));
		expect(triRowText(triGroup(nullCase.container, "Copy ignored files"))).toContain("Use global default: On");
		nullCase.unmount();

		const overriddenCase = render(() => (
			<RepoWorktreeTab settings={makeSettings({ copyIgnoredFiles: true })} defaults={defaults} onUpdate={onUpdate} />
		));
		expect(triRowText(triGroup(overriddenCase.container, "Copy ignored files"))).not.toContain("Use global default");
	});

	it("calls onUpdate for autoConsolidateWorktrees (no null/inherit state — always a concrete bool)", () => {
		const { container } = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ autoConsolidateWorktrees: false })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		const consolidateToggle = container.querySelector('input[type="checkbox"]') as HTMLInputElement;
		fireEvent.change(consolidateToggle, { target: { checked: true } });
		expect(onUpdate).toHaveBeenCalledWith("autoConsolidateWorktrees", true);
	});

	it("updates worktreeStorage, orphanCleanup, prMergeStrategy, afterMerge, and autoDeleteOnPrClose selects", () => {
		const { container } = render(() => (
			<RepoWorktreeTab settings={makeSettings()} defaults={defaults} onUpdate={onUpdate} />
		));
		fireEvent.change(selectByOptionValue(container, "app-dir"), { target: { value: "app-dir" } });
		expect(onUpdate).toHaveBeenCalledWith("worktreeStorage", "app-dir");

		fireEvent.change(selectByOptionValue(container, "delete"), { target: { value: "delete" } });
		expect(onUpdate).toHaveBeenCalledWith("orphanCleanup", "delete");

		fireEvent.change(selectByOptionValue(container, "squash"), { target: { value: "squash" } });
		expect(onUpdate).toHaveBeenCalledWith("prMergeStrategy", "squash");

		fireEvent.change(selectByOptionValue(container, "archive"), { target: { value: "archive" } });
		expect(onUpdate).toHaveBeenCalledWith("afterMerge", "archive");

		fireEvent.change(selectByOptionValue(container, "auto"), { target: { value: "auto" } });
		expect(onUpdate).toHaveBeenCalledWith("autoDeleteOnPrClose", "auto");
	});

	it("selecting a PR-visibility segment calls onUpdate with the matching nullable bool", () => {
		const { container } = render(() => (
			<RepoWorktreeTab settings={makeSettings({ prHideDrafts: null })} defaults={defaults} onUpdate={onUpdate} />
		));
		const group = triGroup(container, "Hide Draft PRs");
		fireEvent.click(triSegment(group, "on"));
		expect(onUpdate).toHaveBeenCalledWith("prHideDrafts", true);
		fireEvent.click(triSegment(group, "off"));
		expect(onUpdate).toHaveBeenCalledWith("prHideDrafts", false);
		fireEvent.click(triSegment(group, "global"));
		expect(onUpdate).toHaveBeenCalledWith("prHideDrafts", null);
	});

	it("resolves PR-visibility fields against the global settingsStore value when null", () => {
		const { container } = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ prHideDrafts: null, prHideConflicting: null })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		// mocked settingsStore.state: prHideDrafts=false, prHideConflicting=true
		expect(triRowText(triGroup(container, "Hide Draft PRs"))).toContain("Use global default: Off");
		expect(triRowText(triGroup(container, "Hide Conflicting PRs"))).toContain("Use global default: On");
	});

	it("hides the macOS-only Terminal section when not on macOS", () => {
		mockIsMacOS.mockReturnValue(false);
		const { queryByText } = render(() => (
			<RepoWorktreeTab settings={makeSettings()} defaults={defaults} onUpdate={onUpdate} />
		));
		expect(queryByText("Enable Cmd+1-9 terminal hotkeys")).toBeNull();
	});

	it("shows the macOS-only Terminal section and toggles terminalMetaHotkeys on macOS", () => {
		mockIsMacOS.mockReturnValue(true);
		const { container, getByText } = render(() => (
			<RepoWorktreeTab settings={makeSettings({ terminalMetaHotkeys: null })} defaults={defaults} onUpdate={onUpdate} />
		));
		expect(getByText("Enable Cmd+1-9 terminal hotkeys")).toBeTruthy();
		const group = triGroup(container, "Enable Cmd+1-9 terminal hotkeys");
		// terminalMetaHotkeys has no real global setting — its "global" resolves to a
		// hardcoded true (On), unlike the other tri-state rows above.
		expect(triRowText(group)).toContain("Use global default: On");
		fireEvent.click(triSegment(group, "off"));
		expect(onUpdate).toHaveBeenCalledWith("terminalMetaHotkeys", false);
	});
});
