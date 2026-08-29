import { fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";

/** Fixture data and mock fns referenced inside `vi.mock` factories below must live in a
 *  `vi.hoisted` block — `vi.mock` calls are hoisted above the rest of this file, so a plain
 *  top-level `const` referenced from inside a factory would still be in its TDZ. */
const fixtures = vi.hoisted(() => {
	/** Two fake "defaults" with known fields, independent of the real 19-rule catalog, so the
	 *  modified/unmodified export-count math can be tested deterministically. */
	const DEFAULT_A = {
		id: "default-a",
		name: "Default A",
		regex: "a+",
		precision: "normal" as const,
		enabled: true,
		actions: [],
	};
	const DEFAULT_B = {
		id: "default-b",
		name: "Default B",
		regex: "b+",
		precision: "normal" as const,
		enabled: true,
		actions: [],
	};

	return {
		DEFAULT_A,
		DEFAULT_B,
		mockSetSmartSelectionRules: vi.fn(),
		toastAdd: vi.fn(),
		exportJsonWithToast: vi.fn().mockResolvedValue(undefined),
		pickJsonImportFile: vi.fn(),
	};
});

const { DEFAULT_A, DEFAULT_B, mockSetSmartSelectionRules, toastAdd, exportJsonWithToast, pickJsonImportFile } =
	fixtures;

/** Store fixture: default-a untouched, default-b regex-edited, one custom rule.
 *  Expected counts: all=3, modified=2 (edited default-b + the custom rule), custom=1. */
let mockState = {
	smartSelectionEnabled: true,
	doubleClickAction: "smart" as "smart" | "word",
	wordSelectionMode: "characters" as "characters" | "regex",
	wordSeparators: "",
	wordSelectionRegex: "",
	smartSelectionRules: [
		{ ...DEFAULT_A },
		{ ...DEFAULT_B, regex: "b-edited" },
		{ id: "custom-1", name: "My Custom Rule", regex: "custom", precision: "normal", enabled: true, actions: [] },
	] as unknown[],
};

vi.mock("../../../stores/settings", () => ({
	DEFAULT_WORD_SEPARATORS: "",
	get settingsStore() {
		return {
			state: mockState,
			setSmartSelectionEnabled: vi.fn(),
			setDoubleClickAction: vi.fn(),
			setWordSelectionMode: vi.fn(),
			setWordSeparators: vi.fn(),
			setWordSelectionRegex: vi.fn(),
			setSmartSelectionRules: mockSetSmartSelectionRules,
		};
	},
}));

vi.mock("../../../components/Terminal/smartSelectionDefaults", () => ({
	DEFAULT_SMART_SELECTION_RULES: [fixtures.DEFAULT_A, fixtures.DEFAULT_B],
	resolveSmartSelectionRules: (userRules: unknown[]) =>
		Array.isArray(userRules) && userRules.length > 0 ? userRules : [fixtures.DEFAULT_A, fixtures.DEFAULT_B],
}));

vi.mock("../../../utils/jsonFileTransfer", () => ({
	exportJsonWithToast: (...args: unknown[]) => fixtures.exportJsonWithToast(...args),
	pickJsonImportFile: (...args: unknown[]) => fixtures.pickJsonImportFile(...args),
}));

vi.mock("../../../stores/toasts", () => ({
	toastsStore: { add: (...args: unknown[]) => fixtures.toastAdd(...args) },
}));

import { SelectionTab } from "../../../components/SettingsPanel/tabs/SelectionTab";

const RULES_EXPORT_KIND = "tuicommander-smart-selection-rules";

// biome-ignore lint/suspicious/noExplicitAny: test fixture payloads are deliberately loose JSON shapes
function rulesEnvelope(rules: Record<string, any>[], overrides: Record<string, unknown> = {}) {
	return JSON.stringify({
		kind: RULES_EXPORT_KIND,
		schemaVersion: 1,
		exportedAt: 0,
		scope: "all",
		rules,
		...overrides,
	});
}

describe("SelectionTab — import/export toolbar", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mockState = {
			smartSelectionEnabled: true,
			doubleClickAction: "smart",
			wordSelectionMode: "characters",
			wordSeparators: "",
			wordSelectionRegex: "",
			smartSelectionRules: [
				{ ...DEFAULT_A },
				{ ...DEFAULT_B, regex: "b-edited" },
				{ id: "custom-1", name: "My Custom Rule", regex: "custom", precision: "normal", enabled: true, actions: [] },
			],
		};
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it("shows live counts for each export scope", () => {
		const { getByTestId } = render(() => <SelectionTab />);
		const select = getByTestId("rule-export-scope-select") as HTMLSelectElement;
		const labels = Array.from(select.options).map((o) => o.textContent);
		expect(labels).toContain("All rules (3)");
		expect(labels).toContain("Modified only (2)");
		expect(labels).toContain("Custom only (1)");
	});

	it("exports 'all' by default with the right filename", async () => {
		const { getByTestId } = render(() => <SelectionTab />);
		fireEvent.click(getByTestId("rule-export-btn"));
		await Promise.resolve();

		expect(exportJsonWithToast).toHaveBeenCalledOnce();
		const [filename, file] = exportJsonWithToast.mock.calls[0];
		expect(filename).toBe("smart-selection-rules-all.json");
		expect(file.scope).toBe("all");
		expect(file.rules).toHaveLength(3);
	});

	it("exports only the selected scope after changing the dropdown", async () => {
		const { getByTestId } = render(() => <SelectionTab />);
		fireEvent.change(getByTestId("rule-export-scope-select"), { target: { value: "custom" } });
		fireEvent.click(getByTestId("rule-export-btn"));
		await Promise.resolve();

		const [filename, file] = exportJsonWithToast.mock.calls[0];
		expect(filename).toBe("smart-selection-rules-custom.json");
		// biome-ignore lint/suspicious/noExplicitAny: test fixture payload
		expect(file.rules.map((r: any) => r.id)).toEqual(["custom-1"]);
	});

	it("opens the review dialog with correct NEW/CONFLICT classification for a valid file", async () => {
		pickJsonImportFile.mockResolvedValue(
			rulesEnvelope([
				{ id: "default-a", name: "Default A", regex: "a+", precision: "normal", enabled: true, actions: [] },
				{ id: "brand-new", name: "Brand New", regex: "new", precision: "normal", enabled: true, actions: [] },
			]),
		);
		const { getByTestId } = render(() => <SelectionTab />);
		fireEvent.click(getByTestId("rule-import-btn"));

		await waitFor(() => expect(getByTestId("import-check-default-a")).toBeTruthy());
		expect(getByTestId("import-check-brand-new")).toBeTruthy();
		expect(getByTestId("import-confirm-btn").textContent).toContain("2");
	});

	it("shows an error toast and no dialog for an invalid file", async () => {
		pickJsonImportFile.mockResolvedValue("not json");
		const { getByTestId, queryByTestId } = render(() => <SelectionTab />);
		fireEvent.click(getByTestId("rule-import-btn"));

		await waitFor(() => expect(toastAdd).toHaveBeenCalled());
		expect(toastAdd.mock.calls[0][0]).toBe("Import failed");
		expect(queryByTestId("import-confirm-btn")).toBeNull();
	});

	it("shows a warning toast for a file with no rules", async () => {
		pickJsonImportFile.mockResolvedValue(rulesEnvelope([]));
		const { getByTestId, queryByTestId } = render(() => <SelectionTab />);
		fireEvent.click(getByTestId("rule-import-btn"));

		await waitFor(() => expect(toastAdd).toHaveBeenCalled());
		expect(toastAdd.mock.calls[0][0]).toBe("Nothing to import");
		expect(queryByTestId("import-confirm-btn")).toBeNull();
	});

	it("does nothing when the file picker is cancelled", async () => {
		pickJsonImportFile.mockResolvedValue(null);
		const { getByTestId, queryByTestId } = render(() => <SelectionTab />);
		fireEvent.click(getByTestId("rule-import-btn"));

		await Promise.resolve();
		await Promise.resolve();
		expect(toastAdd).not.toHaveBeenCalled();
		expect(queryByTestId("import-confirm-btn")).toBeNull();
	});

	it("confirming the dialog merges the selected rules in place and appends new ones", async () => {
		pickJsonImportFile.mockResolvedValue(
			rulesEnvelope([
				{
					id: "default-b",
					name: "Default B replaced",
					regex: "b-replaced",
					precision: "normal",
					enabled: true,
					actions: [],
				},
				{ id: "brand-new", name: "Brand New", regex: "new", precision: "normal", enabled: true, actions: [] },
			]),
		);
		const { getByTestId, queryByTestId } = render(() => <SelectionTab />);
		fireEvent.click(getByTestId("rule-import-btn"));
		await waitFor(() => expect(getByTestId("import-confirm-btn")).toBeTruthy());

		fireEvent.click(getByTestId("import-confirm-btn"));

		expect(mockSetSmartSelectionRules).toHaveBeenCalledOnce();
		const merged = mockSetSmartSelectionRules.mock.calls[0][0] as { id: string; name: string }[];
		expect(merged.map((r) => r.id)).toEqual(["default-a", "default-b", "custom-1", "brand-new"]);
		expect(merged[1].name).toBe("Default B replaced");
		expect(queryByTestId("import-confirm-btn")).toBeNull();
		expect(toastAdd).toHaveBeenCalledWith("Imported 2 rules", "", "info");
	});

	it("cancelling the dialog closes it without merging", async () => {
		pickJsonImportFile.mockResolvedValue(
			rulesEnvelope([
				{ id: "brand-new", name: "Brand New", regex: "new", precision: "normal", enabled: true, actions: [] },
			]),
		);
		const { getByTestId, queryByTestId } = render(() => <SelectionTab />);
		fireEvent.click(getByTestId("rule-import-btn"));
		await waitFor(() => expect(getByTestId("import-cancel-btn")).toBeTruthy());

		fireEvent.click(getByTestId("import-cancel-btn"));

		expect(mockSetSmartSelectionRules).not.toHaveBeenCalled();
		expect(queryByTestId("import-confirm-btn")).toBeNull();
	});

	it("reports a warning toast naming any rules imported disabled, and forces them off", async () => {
		pickJsonImportFile.mockResolvedValue(
			rulesEnvelope([
				{
					id: "risky",
					name: "Prune Branches",
					regex: "prune",
					precision: "normal",
					enabled: true,
					actions: [{ kind: "run_command", title: "Run", parameter: "git branch -D \\0", is_default: false }],
				},
			]),
		);
		const { getByTestId } = render(() => <SelectionTab />);
		fireEvent.click(getByTestId("rule-import-btn"));
		await waitFor(() => expect(getByTestId("import-confirm-btn")).toBeTruthy());
		fireEvent.click(getByTestId("import-confirm-btn"));

		expect(toastAdd).toHaveBeenCalledWith("Imported 1 rule", expect.stringContaining("Prune Branches"), "warn");
		const merged = mockSetSmartSelectionRules.mock.calls[0][0] as { id: string; enabled: boolean }[];
		expect(merged.find((r) => r.id === "risky")?.enabled).toBe(false);
		// Let any in-flight microtasks from the click handler above fully settle before the
		// file's last test ends — otherwise Vitest's async-leak detector can flag an already-
		// resolved promise chain as still pending, purely due to teardown timing.
		await new Promise((resolve) => setTimeout(resolve, 0));
	});
});
