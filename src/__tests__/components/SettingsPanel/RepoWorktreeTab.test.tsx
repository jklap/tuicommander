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
		warmIgnoredDirectories: null,
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
		devServerUrl: null,
		copyPaths: [],
		...overrides,
	};
}

const BASE_REFS = [
	{ name: "main", kind: "local", is_default: true },
	{ name: "master", kind: "local", is_default: false },
	{ name: "develop", kind: "local", is_default: false },
];

const defaults: RepoDefaults = {
	baseBranch: "main",
	copyIgnoredFiles: true,
	copyUntrackedFiles: false,
	warmIgnoredDirectories: true,
	setupScript: "",
	runScript: "",
	archiveScript: "",
	worktreeStorage: "sibling",
	promptOnCreate: true,
	deleteBranchOnRemove: false,
	autoArchiveMerged: true,
	orphanCleanup: "ask",
	orphanCleanupCountdownSeconds: 10,
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
			<RepoWorktreeTab
				settings={makeSettings({ baseBranch: null })}
				defaults={defaults}
				onUpdate={onUpdate}
				baseRefs={BASE_REFS}
			/>
		));
		expect(getByText("Use global default (main)")).toBeTruthy();
		const select = selectByOptionValue(container, "master");
		expect(select.value).toBe("__inherit__");
	});

	it("converts the inherit sentinel back to null when baseBranch is reset to global default", () => {
		const { container } = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ baseBranch: "master" })}
				defaults={defaults}
				onUpdate={onUpdate}
				baseRefs={BASE_REFS}
			/>
		));
		const select = selectByOptionValue(container, "master");
		fireEvent.change(select, { target: { value: "__inherit__" } });
		expect(onUpdate).toHaveBeenCalledWith("baseBranch", null);
	});

	it("passes a concrete branch value through unchanged when baseBranch is overridden", () => {
		const { container } = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ baseBranch: null })}
				defaults={defaults}
				onUpdate={onUpdate}
				baseRefs={BASE_REFS}
			/>
		));
		const select = selectByOptionValue(container, "master");
		fireEvent.change(select, { target: { value: "develop" } });
		expect(onUpdate).toHaveBeenCalledWith("baseBranch", "develop");
	});

	describe("Branch From: dynamic ref list", () => {
		it("offers only Automatic plus 'Use global default' when no baseRefs are loaded yet, with no stale warning", () => {
			const { container, queryByText } = render(() => (
				<RepoWorktreeTab settings={makeSettings({ baseBranch: null })} defaults={defaults} onUpdate={onUpdate} />
			));
			const select = selectByOptionValue(container, "automatic");
			const values = Array.from(select.options).map((o) => o.value);
			expect(values).toEqual(["__inherit__", "automatic"]);
			expect(queryByText(/no longer exists/)).toBeNull();
		});

		it("groups local and remote refs under their own optgroups, in the order the backend returned them", () => {
			const { container } = render(() => (
				<RepoWorktreeTab
					settings={makeSettings({ baseBranch: null })}
					defaults={defaults}
					onUpdate={onUpdate}
					baseRefs={[
						{ name: "main", kind: "local", is_default: true },
						{ name: "feature-x", kind: "local", is_default: false },
						{ name: "origin/release", kind: "remote", is_default: false },
					]}
				/>
			));
			const select = selectByOptionValue(container, "main");
			const groups = Array.from(select.querySelectorAll("optgroup"));
			expect(groups.map((g) => g.label)).toEqual(["Local", "Remote"]);
			expect(Array.from(groups[0].querySelectorAll("option")).map((o) => o.value)).toEqual(["main", "feature-x"]);
			expect(Array.from(groups[1].querySelectorAll("option")).map((o) => o.value)).toEqual(["origin/release"]);
		});

		it("does not offer master/develop when they don't exist in this repo", () => {
			const { container } = render(() => (
				<RepoWorktreeTab
					settings={makeSettings({ baseBranch: null })}
					defaults={defaults}
					onUpdate={onUpdate}
					baseRefs={[{ name: "trunk", kind: "local", is_default: true }]}
				/>
			));
			const select = selectByOptionValue(container, "trunk");
			const values = Array.from(select.options).map((o) => o.value);
			expect(values).not.toContain("master");
			expect(values).not.toContain("develop");
		});

		it("excludes a real branch literally named 'automatic' instead of rendering a second option with the same value as the sentinel", () => {
			const { container } = render(() => (
				<RepoWorktreeTab
					settings={makeSettings({ baseBranch: null })}
					defaults={defaults}
					onUpdate={onUpdate}
					baseRefs={[
						{ name: "main", kind: "local", is_default: true },
						{ name: "automatic", kind: "local", is_default: false },
					]}
				/>
			));
			const select = selectByOptionValue(container, "main");
			const automaticOptions = Array.from(select.options).filter((o) => o.value === "automatic");
			expect(automaticOptions).toHaveLength(1);
			expect(automaticOptions[0].textContent).toBe("Automatic");
		});

		it("shows a stale-branch option plus a warning when the configured branch no longer exists in a loaded list", () => {
			const { container, getByText } = render(() => (
				<RepoWorktreeTab
					settings={makeSettings({ baseBranch: "removed-branch" })}
					defaults={defaults}
					onUpdate={onUpdate}
					baseRefs={BASE_REFS}
				/>
			));
			const select = selectByOptionValue(container, "removed-branch");
			expect(select.value).toBe("removed-branch");
			expect(select.selectedOptions[0].textContent).toContain("no longer exists");
			expect(getByText(/no longer exists in this repo/)).toBeTruthy();
		});

		it("does not warn about a configured branch while baseRefs is still loading (undefined)", () => {
			const { queryByText } = render(() => (
				<RepoWorktreeTab
					settings={makeSettings({ baseBranch: "removed-branch" })}
					defaults={defaults}
					onUpdate={onUpdate}
				/>
			));
			expect(queryByText(/no longer exists/)).toBeNull();
		});

		it("never warns about the 'automatic' sentinel even when baseRefs is loaded", () => {
			const { queryByText } = render(() => (
				<RepoWorktreeTab
					settings={makeSettings({ baseBranch: "automatic" })}
					defaults={defaults}
					onUpdate={onUpdate}
					baseRefs={BASE_REFS}
				/>
			));
			expect(queryByText(/no longer exists/)).toBeNull();
		});

		it("does not show a stale warning once the configured branch is present in baseRefs", () => {
			const { queryByText } = render(() => (
				<RepoWorktreeTab
					settings={makeSettings({ baseBranch: "develop" })}
					defaults={defaults}
					onUpdate={onUpdate}
					baseRefs={BASE_REFS}
				/>
			));
			expect(queryByText(/no longer exists/)).toBeNull();
		});
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

	it("resolves copyIgnoredFiles/copyUntrackedFiles against the global default when null, and flips onChange", () => {
		const { container } = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ copyIgnoredFiles: null, copyUntrackedFiles: null })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		// Nullable booleans are explicit-inherit selects (BooleanOverride): null selects the
		// inherit option, whose label resolves the global default.
		const ignored = container.querySelector(
			'select[data-setting-id="repositories.repos.{key}.copy_ignored_files"]',
		) as HTMLSelectElement;
		const untracked = container.querySelector(
			'select[data-setting-id="repositories.repos.{key}.copy_untracked_files"]',
		) as HTMLSelectElement;
		// copyIgnoredFiles inherits defaults.copyIgnoredFiles=true, copyUntrackedFiles inherits false.
		expect(ignored.value).toBe("__inherit__");
		expect(ignored.options[0].textContent).toBe("Use global default (On)");
		expect(untracked.options[0].textContent).toBe("Use global default (Off)");
		fireEvent.change(ignored, { target: { value: "false" } });
		expect(onUpdate).toHaveBeenCalledWith("copyIgnoredFiles", false);
		fireEvent.change(untracked, { target: { value: "__inherit__" } });
		expect(onUpdate).toHaveBeenCalledWith("copyUntrackedFiles", null);
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

	it("selects the inherit option only while the field is null, and the override once set", () => {
		const selector = 'select[data-setting-id="repositories.repos.{key}.copy_ignored_files"]';
		const nullCase = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ ...noOtherHints, copyIgnoredFiles: null })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		expect((nullCase.container.querySelector(selector) as HTMLSelectElement).value).toBe("__inherit__");
		nullCase.unmount();

		const overriddenCase = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ ...noOtherHints, copyIgnoredFiles: true })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		expect((overriddenCase.container.querySelector(selector) as HTMLSelectElement).value).toBe("true");
	});

	it("resolves warmIgnoredDirectories against the global default when null, and selecting Off overrides it", () => {
		const { container } = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ warmIgnoredDirectories: null })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		// warmIgnoredDirectories inherits defaults.warmIgnoredDirectories=true.
		const warm = container.querySelector(
			'select[data-setting-id="repositories.repos.{key}.warm_ignored_directories"]',
		) as HTMLSelectElement;
		expect(warm.value).toBe("__inherit__");
		expect(warm.options[0].textContent).toBe("Use global default (On)");
		fireEvent.change(warm, { target: { value: "false" } });
		expect(onUpdate).toHaveBeenCalledWith("warmIgnoredDirectories", false);
	});

	it("selects the explicit override for warmIgnoredDirectories once set", () => {
		const overridden = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ warmIgnoredDirectories: false })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		const warm = overridden.container.querySelector(
			'select[data-setting-id="repositories.repos.{key}.warm_ignored_directories"]',
		) as HTMLSelectElement;
		expect(warm.value).toBe("false");
	});

	it("calls onUpdate for autoConsolidateWorktrees (no null/inherit state — always a concrete bool)", () => {
		const { container } = render(() => (
			<RepoWorktreeTab
				settings={makeSettings({ autoConsolidateWorktrees: false })}
				defaults={defaults}
				onUpdate={onUpdate}
			/>
		));
		const consolidateToggle = container.querySelector(
			'input[data-setting-id="repositories.repos.{key}.auto_consolidate_worktrees"]',
		) as HTMLInputElement;
		fireEvent.change(consolidateToggle, { target: { checked: true } });
		expect(onUpdate).toHaveBeenCalledWith("autoConsolidateWorktrees", true);
	});

	it("updates worktreeStorage, orphanCleanup, prMergeStrategy, afterMerge, and autoDeleteOnPrClose selects", () => {
		const { container } = render(() => (
			<RepoWorktreeTab settings={makeSettings()} defaults={defaults} onUpdate={onUpdate} />
		));
		fireEvent.change(selectByOptionValue(container, "app-dir"), { target: { value: "app-dir" } });
		expect(onUpdate).toHaveBeenCalledWith("worktreeStorage", "app-dir");

		fireEvent.change(selectByOptionValue(container, "on"), { target: { value: "on" } });
		expect(onUpdate).toHaveBeenCalledWith("orphanCleanup", "on");

		fireEvent.change(selectByOptionValue(container, "squash"), { target: { value: "squash" } });
		expect(onUpdate).toHaveBeenCalledWith("prMergeStrategy", "squash");

		fireEvent.change(selectByOptionValue(container, "archive"), { target: { value: "archive" } });
		expect(onUpdate).toHaveBeenCalledWith("afterMerge", "archive");

		fireEvent.change(selectByOptionValue(container, "auto"), { target: { value: "auto" } });
		expect(onUpdate).toHaveBeenCalledWith("autoDeleteOnPrClose", "auto");
	});

	it("maps the PR-visibility select's show/hide/inherit choices back to a nullable bool", () => {
		const { container } = render(() => (
			<RepoWorktreeTab settings={makeSettings({ prHideDrafts: null })} defaults={defaults} onUpdate={onUpdate} />
		));
		const select = container.querySelector(
			'select[data-setting-id="repositories.repos.{key}.pr_hide_drafts"]',
		) as HTMLSelectElement;
		expect(select.value).toBe("__inherit__");
		fireEvent.change(select, { target: { value: "false" } }); // Show
		expect(onUpdate).toHaveBeenCalledWith("prHideDrafts", false);
		fireEvent.change(select, { target: { value: "__inherit__" } });
		expect(onUpdate).toHaveBeenCalledWith("prHideDrafts", null);
	});

	describe("copyPaths list", () => {
		it("renders each existing entry with its path and mode", () => {
			const { getByText, container } = render(() => (
				<RepoWorktreeTab
					settings={makeSettings({
						copyPaths: [
							{ path: ".env", mode: "copy" },
							{ path: "node_modules", mode: "symlink" },
						],
					})}
					defaults={defaults}
					onUpdate={onUpdate}
				/>
			));
			expect(getByText(".env")).toBeTruthy();
			expect(getByText("node_modules")).toBeTruthy();
			const modeSelects = Array.from(container.querySelectorAll("select")).filter((sel) =>
				Array.from((sel as HTMLSelectElement).options).some((o) => o.value === "symlink"),
			) as HTMLSelectElement[];
			// One per existing row, plus the add-row's own mode select.
			expect(modeSelects).toHaveLength(3);
			expect(modeSelects[0].value).toBe("copy");
			expect(modeSelects[1].value).toBe("symlink");
		});

		it("adds a new entry with the chosen mode and clears the draft input", () => {
			const { getByPlaceholderText, getByText } = render(() => (
				<RepoWorktreeTab settings={makeSettings({ copyPaths: [] })} defaults={defaults} onUpdate={onUpdate} />
			));
			const input = getByPlaceholderText("e.g. .env or node_modules") as HTMLInputElement;
			fireEvent.input(input, { target: { value: ".env" } });
			fireEvent.click(getByText("Add"));

			expect(onUpdate).toHaveBeenCalledWith("copyPaths", [{ path: ".env", mode: "copy" }]);
			expect(input.value).toBe("");
		});

		it("adds a new entry on Enter without requiring the Add button", () => {
			const { getByPlaceholderText } = render(() => (
				<RepoWorktreeTab settings={makeSettings({ copyPaths: [] })} defaults={defaults} onUpdate={onUpdate} />
			));
			const input = getByPlaceholderText("e.g. .env or node_modules") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "config/local.json" } });
			fireEvent.keyDown(input, { key: "Enter" });

			expect(onUpdate).toHaveBeenCalledWith("copyPaths", [{ path: "config/local.json", mode: "copy" }]);
		});

		it("does not add an entry for a blank or whitespace-only path", () => {
			const { getByPlaceholderText, getByText } = render(() => (
				<RepoWorktreeTab settings={makeSettings({ copyPaths: [] })} defaults={defaults} onUpdate={onUpdate} />
			));
			const input = getByPlaceholderText("e.g. .env or node_modules") as HTMLInputElement;
			fireEvent.input(input, { target: { value: "   " } });
			fireEvent.click(getByText("Add"));
			expect(onUpdate).not.toHaveBeenCalled();
		});

		it("does not add a duplicate path", () => {
			const { getByPlaceholderText, getByText } = render(() => (
				<RepoWorktreeTab
					settings={makeSettings({ copyPaths: [{ path: ".env", mode: "copy" }] })}
					defaults={defaults}
					onUpdate={onUpdate}
				/>
			));
			const input = getByPlaceholderText("e.g. .env or node_modules") as HTMLInputElement;
			fireEvent.input(input, { target: { value: ".env" } });
			fireEvent.click(getByText("Add"));
			expect(onUpdate).not.toHaveBeenCalled();
		});

		it("removes an entry by path", () => {
			const { getByText } = render(() => (
				<RepoWorktreeTab
					settings={makeSettings({
						copyPaths: [
							{ path: ".env", mode: "copy" },
							{ path: "node_modules", mode: "symlink" },
						],
					})}
					defaults={defaults}
					onUpdate={onUpdate}
				/>
			));
			fireEvent.click(getByText(".env").closest("div")!.querySelector("button")!);
			expect(onUpdate).toHaveBeenCalledWith("copyPaths", [{ path: "node_modules", mode: "symlink" }]);
		});

		it("changes an existing entry's mode without touching other entries", () => {
			const { container } = render(() => (
				<RepoWorktreeTab
					settings={makeSettings({
						copyPaths: [
							{ path: ".env", mode: "copy" },
							{ path: "node_modules", mode: "copy" },
						],
					})}
					defaults={defaults}
					onUpdate={onUpdate}
				/>
			));
			const rowSelects = Array.from(container.querySelectorAll("select")).filter((sel) =>
				Array.from((sel as HTMLSelectElement).options).some((o) => o.value === "symlink"),
			) as HTMLSelectElement[];
			fireEvent.change(rowSelects[0], { target: { value: "symlink" } });
			expect(onUpdate).toHaveBeenCalledWith("copyPaths", [
				{ path: ".env", mode: "symlink" },
				{ path: "node_modules", mode: "copy" },
			]);
		});
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
		const select = container.querySelector(
			'select[data-setting-id="repositories.repos.{key}.terminal_meta_hotkeys"]',
		) as HTMLSelectElement;
		expect(select.value).toBe("__inherit__");
		expect(select.options[0].textContent).toBe("Use global default (On)"); // fallback true
		fireEvent.change(select, { target: { value: "false" } });
		expect(onUpdate).toHaveBeenCalledWith("terminalMetaHotkeys", false);
	});
});
