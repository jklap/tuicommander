import { fireEvent, render } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import { RepoScriptsTab } from "../../../components/SettingsPanel/tabs/RepoScriptsTab";
import type { RepoDefaults } from "../../../stores/repoDefaults";
import type { RepoSettings } from "../../../stores/repoSettings";

function makeSettings(overrides: Partial<RepoSettings> = {}): RepoSettings {
	return {
		path: "/repo",
		displayName: "my-repo",
		autoConsolidateWorktrees: false,
		baseBranch: "automatic",
		copyIgnoredFiles: false,
		copyUntrackedFiles: false,
		setupScript: null,
		runScript: null,
		archiveScript: null,
		color: "",
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
	baseBranch: "automatic",
	copyIgnoredFiles: false,
	copyUntrackedFiles: false,
	setupScript: "npm ci",
	runScript: "npm start",
	archiveScript: "docker compose down -v",
	worktreeStorage: "sibling",
	promptOnCreate: true,
	deleteBranchOnRemove: false,
	autoArchiveMerged: false,
	orphanCleanup: "ask",
	prMergeStrategy: "merge",
	afterMerge: "ask",
	autoFetchIntervalMinutes: 0,
	autoDeleteOnPrClose: "off",
};

describe("RepoScriptsTab", () => {
	let onUpdate: <K extends keyof RepoSettings>(key: K, value: RepoSettings[K]) => void;

	beforeEach(() => {
		onUpdate = vi.fn();
	});

	it("renders the Setup, Run, and Archive Script fields with their current values", () => {
		const settings = makeSettings({ setupScript: "npm install", runScript: "npm run dev", archiveScript: null });
		const { container } = render(() => <RepoScriptsTab settings={settings} defaults={defaults} onUpdate={onUpdate} />);
		const textareas = Array.from(container.querySelectorAll("textarea")) as HTMLTextAreaElement[];
		expect(textareas).toHaveLength(3);
		expect(textareas[0].value).toBe("npm install");
		expect(textareas[1].value).toBe("npm run dev");
		expect(textareas[2].value).toBe("");
	});

	it("shows the inherited-default placeholder when a script override is null", () => {
		const settings = makeSettings({ setupScript: null });
		const { container } = render(() => <RepoScriptsTab settings={settings} defaults={defaults} onUpdate={onUpdate} />);
		const setupTextarea = container.querySelectorAll("textarea")[0] as HTMLTextAreaElement;
		expect(setupTextarea.placeholder).toBe("Inheriting: npm ci");
	});

	it("falls back to a canned example placeholder when null AND no global default is set", () => {
		const settings = makeSettings({ setupScript: null });
		const emptyDefaults: RepoDefaults = { ...defaults, setupScript: "" };
		const { container } = render(() => (
			<RepoScriptsTab settings={settings} defaults={emptyDefaults} onUpdate={onUpdate} />
		));
		const setupTextarea = container.querySelectorAll("textarea")[0] as HTMLTextAreaElement;
		expect(setupTextarea.placeholder).toBe("#!/bin/bash\nnpm install");
	});

	it("calls onUpdate with the field key and value when the setup script changes", () => {
		const settings = makeSettings();
		const { container } = render(() => <RepoScriptsTab settings={settings} defaults={defaults} onUpdate={onUpdate} />);
		const setupTextarea = container.querySelectorAll("textarea")[0];
		fireEvent.input(setupTextarea, { target: { value: "#!/bin/bash\npnpm install" } });
		expect(onUpdate).toHaveBeenCalledWith("setupScript", "#!/bin/bash\npnpm install");
	});

	it("calls onUpdate with null when a script is cleared back to empty (reverting to inherit)", () => {
		const settings = makeSettings({ runScript: "npm run dev" });
		const { container } = render(() => <RepoScriptsTab settings={settings} defaults={defaults} onUpdate={onUpdate} />);
		const runTextarea = container.querySelectorAll("textarea")[1];
		fireEvent.input(runTextarea, { target: { value: "" } });
		expect(onUpdate).toHaveBeenCalledWith("runScript", null);
	});

	it("shows the 'Using global default' hint only when the override is null", () => {
		const settings = makeSettings({ setupScript: null, runScript: "custom", archiveScript: null });
		const { getAllByText } = render(() => (
			<RepoScriptsTab settings={settings} defaults={defaults} onUpdate={onUpdate} />
		));
		// setupScript and archiveScript are null (inheriting) -> 2 hints; runScript is overridden -> none.
		expect(getAllByText(/Using global default\./)).toHaveLength(2);
	});
});
