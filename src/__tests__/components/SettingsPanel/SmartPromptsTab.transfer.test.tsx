import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import type { SavedPrompt } from "../../../stores/promptLibrary";

/** Fixture data and mock fns referenced inside `vi.mock` factories below must live in a
 *  `vi.hoisted` block — `vi.mock` calls are hoisted above the rest of this file, so a plain
 *  top-level `const` referenced from inside a factory would still be in its TDZ. */
const fixtures = vi.hoisted(() => {
	/** Two fake "built-ins" with known fields, independent of the real 27-prompt catalog, so
	 *  the modified/unmodified export-count math can be tested deterministically. */
	const FAKE_BUILTIN_A = {
		id: "fake-builtin-a",
		name: "Fake Builtin A",
		content: "Do A",
		category: "custom",
		isFavorite: false,
		tags: ["smart"],
		builtIn: true,
		enabled: true,
		createdAt: 0,
		updatedAt: 0,
	};
	const FAKE_BUILTIN_B = {
		id: "fake-builtin-b",
		name: "Fake Builtin B",
		content: "Do B",
		category: "custom",
		isFavorite: false,
		tags: ["smart"],
		builtIn: true,
		enabled: true,
		createdAt: 0,
		updatedAt: 0,
	};

	/** Store fixture: builtin-a unmodified, builtin-b content-edited, one custom prompt.
	 *  Expected counts: all=3, modified=2 (edited builtin-b + the custom prompt), custom=1. */
	function makeLibraryPrompts() {
		return [
			{ ...FAKE_BUILTIN_A },
			{ ...FAKE_BUILTIN_B, content: "Do B, but different now" },
			{
				id: "custom-1",
				name: "My Custom Prompt",
				content: "custom content",
				category: "custom",
				isFavorite: false,
				builtIn: false,
				enabled: true,
				createdAt: 0,
				updatedAt: 0,
			},
		];
	}

	return {
		FAKE_BUILTIN_A,
		FAKE_BUILTIN_B,
		makeLibraryPrompts,
		importPrompts: vi.fn().mockReturnValue({ imported: 0, disabled: [] }),
		toastAdd: vi.fn(),
		exportJsonWithToast: vi.fn().mockResolvedValue(undefined),
		pickJsonImportFile: vi.fn(),
	};
});

vi.mock("../../../data/smartPromptsBuiltIn", () => ({
	SMART_PROMPTS_BUILTIN: [fixtures.FAKE_BUILTIN_A, fixtures.FAKE_BUILTIN_B],
}));

vi.mock("../../../stores/promptLibrary", () => ({
	promptLibraryStore: {
		getAllPrompts: () => fixtures.makeLibraryPrompts(),
		get state() {
			const byId: Record<string, SavedPrompt> = {};
			for (const p of fixtures.makeLibraryPrompts()) byId[p.id] = p as SavedPrompt;
			return { prompts: byId };
		},
		importPrompts: (...args: unknown[]) => fixtures.importPrompts(...args),
		createPrompt: vi.fn(),
		updatePrompt: vi.fn(),
		deletePrompt: vi.fn(),
		resetToDefault: vi.fn(),
		isOverridden: () => false,
		hasUpdate: () => false,
	},
}));

vi.mock("../../../utils/jsonFileTransfer", () => ({
	exportJsonWithToast: (...args: unknown[]) => fixtures.exportJsonWithToast(...args),
	pickJsonImportFile: (...args: unknown[]) => fixtures.pickJsonImportFile(...args),
}));

vi.mock("../../../stores/toasts", () => ({
	toastsStore: { add: (...args: unknown[]) => fixtures.toastAdd(...args) },
}));

vi.mock("../../../hooks/useAgentDetection", () => ({
	useAgentDetection: () => ({
		detectAll: vi.fn(),
		getAvailable: () => [],
		loading: () => false,
	}),
}));

vi.mock("../../../stores/agentConfigs", () => ({
	agentConfigsStore: {
		getHeadlessAgent: () => null,
		setHeadlessAgent: vi.fn(),
		getRunConfigs: () => [],
	},
}));

import { SmartPromptsTab } from "../../../components/SettingsPanel/tabs/SmartPromptsTab";

const { importPrompts, toastAdd, exportJsonWithToast, pickJsonImportFile } = fixtures;

function exportEnvelope(prompts: Partial<SavedPrompt>[], overrides: Record<string, unknown> = {}) {
	return JSON.stringify({
		kind: "tuicommander-smart-prompts",
		schemaVersion: 1,
		exportedAt: 0,
		scope: "all",
		prompts,
		...overrides,
	});
}

describe("SmartPromptsTab — import/export toolbar", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		importPrompts.mockReturnValue({ imported: 0, disabled: [] });
	});

	it("shows live counts for each export scope", () => {
		const { getByTestId } = render(() => <SmartPromptsTab />);
		const select = getByTestId("export-scope-select") as HTMLSelectElement;
		const labels = Array.from(select.options).map((o) => o.textContent);
		expect(labels).toContain("All prompts (3)");
		expect(labels).toContain("Modified only (2)");
		expect(labels).toContain("Custom only (1)");
	});

	it("shows a hint that the export covers the whole prompt library", () => {
		const { getByText } = render(() => <SmartPromptsTab />);
		expect(getByText(/Exports your entire prompt library, including prompts not listed below/)).toBeTruthy();
	});

	it("exports 'all' by default with the right filename", async () => {
		const { getByTestId } = render(() => <SmartPromptsTab />);
		fireEvent.click(getByTestId("export-btn"));
		await Promise.resolve();

		expect(exportJsonWithToast).toHaveBeenCalledOnce();
		const [filename, file] = exportJsonWithToast.mock.calls[0];
		expect(filename).toBe("prompts-all.json");
		expect(file.scope).toBe("all");
		expect(file.prompts).toHaveLength(3);
	});

	it("exports only the selected scope after changing the dropdown", async () => {
		const { getByTestId } = render(() => <SmartPromptsTab />);
		fireEvent.change(getByTestId("export-scope-select"), { target: { value: "custom" } });
		fireEvent.click(getByTestId("export-btn"));
		await Promise.resolve();

		const [filename, file] = exportJsonWithToast.mock.calls[0];
		expect(filename).toBe("prompts-custom.json");
		expect(file.scope).toBe("custom");
		expect(file.prompts.map((p: SavedPrompt) => p.id)).toEqual(["custom-1"]);
	});

	it("opens the review dialog with correct NEW/CONFLICT classification for a valid file", async () => {
		pickJsonImportFile.mockResolvedValue(
			exportEnvelope([
				{ id: "custom-1", name: "My Custom Prompt", content: "custom content", category: "custom", isFavorite: false },
				{ id: "brand-new", name: "Brand New", content: "new content", category: "custom", isFavorite: false },
			]),
		);
		const { getByTestId } = render(() => <SmartPromptsTab />);
		fireEvent.click(getByTestId("import-btn"));

		await waitFor(() => expect(getByTestId("import-check-custom-1")).toBeTruthy());
		expect(getByTestId("import-check-brand-new")).toBeTruthy();
		expect(getByTestId("import-confirm-btn").textContent).toContain("2");
	});

	it("shows an error toast and no dialog for an invalid file", async () => {
		pickJsonImportFile.mockResolvedValue("not json");
		const { getByTestId, queryByTestId } = render(() => <SmartPromptsTab />);
		fireEvent.click(getByTestId("import-btn"));

		await waitFor(() => expect(toastAdd).toHaveBeenCalled());
		expect(toastAdd.mock.calls[0][0]).toBe("Import failed");
		expect(queryByTestId("import-confirm-btn")).toBeNull();
	});

	it("shows a warning toast for a file with no prompts", async () => {
		pickJsonImportFile.mockResolvedValue(exportEnvelope([]));
		const { getByTestId, queryByTestId } = render(() => <SmartPromptsTab />);
		fireEvent.click(getByTestId("import-btn"));

		await waitFor(() => expect(toastAdd).toHaveBeenCalled());
		expect(toastAdd.mock.calls[0][0]).toBe("Nothing to import");
		expect(queryByTestId("import-confirm-btn")).toBeNull();
	});

	it("does nothing when the file picker is cancelled", async () => {
		pickJsonImportFile.mockResolvedValue(null);
		const { getByTestId, queryByTestId } = render(() => <SmartPromptsTab />);
		fireEvent.click(getByTestId("import-btn"));

		await Promise.resolve();
		await Promise.resolve();
		expect(toastAdd).not.toHaveBeenCalled();
		expect(queryByTestId("import-confirm-btn")).toBeNull();
	});

	it("confirming the dialog imports the selected prompts and closes it", async () => {
		importPrompts.mockReturnValue({ imported: 2, disabled: [] });
		pickJsonImportFile.mockResolvedValue(
			exportEnvelope([
				{ id: "custom-1", name: "My Custom Prompt", content: "custom content", category: "custom", isFavorite: false },
				{ id: "brand-new", name: "Brand New", content: "new content", category: "custom", isFavorite: false },
			]),
		);
		const { getByTestId, queryByTestId } = render(() => <SmartPromptsTab />);
		fireEvent.click(getByTestId("import-btn"));
		await waitFor(() => expect(getByTestId("import-confirm-btn")).toBeTruthy());

		fireEvent.click(getByTestId("import-confirm-btn"));

		expect(importPrompts).toHaveBeenCalledOnce();
		const imported = importPrompts.mock.calls[0][0] as SavedPrompt[];
		expect(imported.map((p) => p.id).sort()).toEqual(["brand-new", "custom-1"]);
		expect(queryByTestId("import-confirm-btn")).toBeNull();
		expect(toastAdd).toHaveBeenCalledWith("Imported 2 prompts", "", "info");
	});

	it("cancelling the dialog closes it without importing", async () => {
		pickJsonImportFile.mockResolvedValue(exportEnvelope([{ id: "brand-new", name: "Brand New", content: "c" }]));
		const { getByTestId, queryByTestId } = render(() => <SmartPromptsTab />);
		fireEvent.click(getByTestId("import-btn"));
		await waitFor(() => expect(getByTestId("import-cancel-btn")).toBeTruthy());

		fireEvent.click(getByTestId("import-cancel-btn"));

		expect(importPrompts).not.toHaveBeenCalled();
		expect(queryByTestId("import-confirm-btn")).toBeNull();
	});

	it("reports a warning toast naming any prompts imported disabled", async () => {
		importPrompts.mockReturnValue({ imported: 1, disabled: ["Prune Branches"] });
		pickJsonImportFile.mockResolvedValue(
			exportEnvelope([
				{
					id: "shell-prompt",
					name: "Prune Branches",
					content: "git branch -D $(git branch --merged)",
					category: "custom",
					isFavorite: false,
					executionMode: "shell",
				},
			]),
		);
		const { getByTestId } = render(() => <SmartPromptsTab />);
		fireEvent.click(getByTestId("import-btn"));
		await waitFor(() => expect(getByTestId("import-confirm-btn")).toBeTruthy());
		fireEvent.click(getByTestId("import-confirm-btn"));

		expect(toastAdd).toHaveBeenCalledWith("Imported 1 prompt", expect.stringContaining("Prune Branches"), "warn");
		// Let any in-flight microtasks from the click handler above fully settle before the
		// file's last test ends — otherwise Vitest's async-leak detector can flag an already-
		// resolved promise chain as still pending, purely due to teardown timing.
		await new Promise((resolve) => setTimeout(resolve, 0));
	});
});
