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
			<RepoWorktreeTab settings={makeSettings({ autoFetchIntervalMinutes: null })} defaults={defaults} onUpdate={onUpdate} />
		));
		const select = selectByOptionValue(container, "30");
		fireEvent.change(select, { target: { value: "30" } });
		expect(onUpdate).toHaveBeenCalledWith("autoFetchIntervalMinutes", 30);
	});

	it("resolves copyIgnoredFiles/copyUntrackedFiles against the global default when null, and flips onChange", () => {
		const { container } = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ copyIgnoredFiles: null, copyUntrackedFiles: null })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		const checkboxes = Array.from(container.querySelectorAll('input[type="checkbox"]')) as HTMLInputElement[];
		// copyIgnoredFiles inherits defaults.copyIgnoredFiles=true, copyUntrackedFiles inherits false.
		expect(checkboxes[0].checked).toBe(true);
		expect(checkboxes[1].checked).toBe(false);
		fireEvent.change(checkboxes[0], { target: { checked: false } });
		expect(onUpdate).toHaveBeenCalledWith("copyIgnoredFiles", false);
	});

	// Every other nullable-with-hint field is pinned to a concrete value below so only
	// copyIgnoredFiles's own hint is in play — several fields share this exact "(Global
	// Default)" hint text.
	const noOtherHints: Partial<RepoSettings> = {
		copyUntrackedFiles: true,
		promptOnCreate: true,
		deleteBranchOnRemove: true,
		autoArchiveMerged: true,
	};

	it("shows the (Global Default) hint only while the field is null, and clears it once overridden", () => {
		const nullCase = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ ...noOtherHints, copyIgnoredFiles: null })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		expect(nullCase.getAllByText("(Global Default)")).toHaveLength(1);
		nullCase.unmount();

		const overriddenCase = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ ...noOtherHints, copyIgnoredFiles: true })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		expect(overriddenCase.queryByText("(Global Default)")).toBeNull();
	});

	it("calls onUpdate for autoConsolidateWorktrees (no null/inherit state — always a concrete bool)", () => {
		const { container } = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ autoConsolidateWorktrees: false })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		const consolidateToggle = Array.from(container.querySelectorAll('input[type="checkbox"]'))[2] as HTMLInputElement;
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

	it("cycles a PR-visibility TriStateToggle and converts hide/show back to a nullable bool", () => {
		const { getAllByRole } = render(() => (
			<RepoWorktreeTab settings={makeSettings({ prHideDrafts: null })} defaults={defaults} onUpdate={onUpdate} />
		));
		const toggles = getAllByRole("button").filter((el) => el.getAttribute("data-value") !== null);
		// Draft PRs is the first TriStateToggle; starts at "default" (null), cycles to "show".
		fireEvent.click(toggles[0]);
		expect(onUpdate).toHaveBeenCalledWith("prHideDrafts", false);
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
		const { getByText } = render(() => (
			<RepoWorktreeTab settings={makeSettings({ terminalMetaHotkeys: null })} defaults={defaults} onUpdate={onUpdate} />
		));
		const label = getByText("Enable Cmd+1-9 terminal hotkeys");
		const checkbox = label.closest("span")?.parentElement?.querySelector('input[type="checkbox"]') as HTMLInputElement;
		expect(checkbox.checked).toBe(true); // null -> effectiveBool(null, true) -> true
		fireEvent.change(checkbox, { target: { checked: false } });
		expect(onUpdate).toHaveBeenCalledWith("terminalMetaHotkeys", false);
	});
});
